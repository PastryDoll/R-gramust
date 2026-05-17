use rand::prelude::*;
use rustc_hash::{FxHashMap, FxHasher};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::thread;
use std::time::Duration;

type NgramShard<'a> = FxHashMap<&'a [u16], Vec<(u16, u32)>>;

pub struct Ngram<'a> {
    pub shards: Vec<NgramShard<'a>>,
    pub max_window: usize,
}

fn get_shard(key: &[u16], num_shards: usize) -> usize {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    (hasher.finish() % num_shards as u64) as usize
}

fn sample_next(window: &[u16], shards: &[NgramShard], rng: &mut impl Rng) -> Option<u16> {
    // Try progressively shorter windows
    for size in (1..=window.len()).rev() {
        let key: &[u16] = &window[window.len() - size..];
        let shard = &shards[get_shard(key, shards.len())];
        if let Some(nexts) = shard.get(key) {
            let total: u32 = nexts.iter().map(|(_, c)| *c).sum();
            let mut roll: u32 = rng.random_range(0..total);
            for &(tok, count) in nexts {
                if roll < count {
                    return Some(tok);
                }
                roll -= count;
            }
        }
    }
    None
}

/// Scans tokens[start..end], builds N shards with counts.
fn count_chunk<'a>(
    tokens: &'a [u16],
    start: usize,
    end: usize,
    num_shards: usize,
    max_window: usize,
) -> Vec<NgramShard<'a>> {
    let mut shards: Vec<NgramShard<'a>> = (0..num_shards).map(|_| FxHashMap::default()).collect();

    for i in start..end {
        let next_tok = tokens[i];
        for size in 1..=max_window {
            if i < size {
                continue;
            }
            let window: &'a [u16] = &tokens[i - size..i];

            let s = get_shard(window, shards.len());
            let entry = shards[s].entry(window).or_default();
            if let Some(slot) = entry.iter_mut().find(|(t, _)| *t == next_tok) {
                slot.1 += 1;
            } else {
                entry.push((next_tok, 1));
            }
        }
    }
    shards
}

fn merge_shards<'a>(submaps: Vec<NgramShard<'a>>) -> NgramShard<'a> {
    let mut merged: NgramShard = FxHashMap::default();
    for submap in submaps {
        for (window, nexts) in submap {
            let entry = merged.entry(window).or_default();
            for (tok, count) in nexts {
                if let Some(slot) = entry.iter_mut().find(|(t, _)| *t == tok) {
                    slot.1 += count;
                } else {
                    entry.push((tok, count));
                }
            }
        }
    }
    merged
}

pub fn build_ngram<'a>(tokens: &'a [u16], max_window: usize) -> Ngram<'a> {
    let n = tokens.len();
    let num_threads = thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(8);

    let num_shards = num_threads;
    let chunk_size = n.div_ceil(num_threads);

    let counts: Vec<NgramShard> = thread::scope(|scope| {
        let mut count_handles = Vec::new();
        for t in 0..num_threads {
            let start = t * chunk_size;
            let end = ((t + 1) * chunk_size).min(n);
            let tokens = &tokens;
            count_handles
                .push(scope.spawn(move || count_chunk(tokens, start, end, num_shards, max_window)));
        }

        let grid: Vec<Vec<NgramShard>> = count_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();

        let mut columns: Vec<Vec<NgramShard>> = (0..num_shards).map(|_| Vec::new()).collect();
        for row in grid {
            for (k, submap) in row.into_iter().enumerate() {
                columns[k].push(submap);
            }
        }

        let mut merge_handles = Vec::new();
        for column in columns {
            merge_handles.push(scope.spawn(move || merge_shards(column)));
        }
        let shard_maps: Vec<NgramShard> = merge_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();

        shard_maps
    });

    Ngram {
        shards: counts,
        max_window,
    }
}

pub fn generate(
    ngram: &Ngram,
    seed: &[u16],
    num_sample: usize,
    decode_map: &FxHashMap<u16, (u16, u16)>,
) -> Vec<u16> {
    let take = seed.len().min(ngram.max_window);
    let tail = &seed[seed.len() - take..];

    let mut window: Vec<u16> = Vec::new();
    for size in (1..=tail.len()).rev() {
        let candidate: &[u16] = &tail[tail.len() - size..];
        if ngram.shards[get_shard(candidate, ngram.shards.len())].contains_key(candidate) {
            window = candidate.to_vec();
            break;
        }
    }
    // Fallback pick a random key
    if window.is_empty() {
        let mut rng = rand::rng();
        let non_empty: Vec<&NgramShard> = ngram.shards.iter().filter(|s| !s.is_empty()).collect();
        if let Some(shard) = non_empty.choose(&mut rng) {
            if let Some((key, _)) = shard.iter().choose(&mut rng) {
                window = key.to_vec();
            }
        }
    }

    let mut output: Vec<u16> = seed.to_vec();
    let mut byte_buf: Vec<u8> = Vec::new();
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();

    for &tok in seed {
        byte_buf.clear();
        crate::bpe::decode_token(tok, decode_map, &mut byte_buf);
        lock.write_all(&byte_buf).ok();
    }

    let mut rng = rand::rng();
    for _ in 0..num_sample {
        if let Some(next) = sample_next(&window, &ngram.shards, &mut rng) {
            output.push(next);

            byte_buf.clear();
            crate::bpe::decode_token(next, decode_map, &mut byte_buf);
            lock.write_all(&byte_buf).ok();
            lock.flush().ok();
            thread::sleep(Duration::from_millis(20));
            if window.len() == ngram.max_window {
                window.remove(0);
            }
            window.push(next);
        } else {
            break;
        }
    }
    lock.write_all(b"\n").ok();
    output
}
