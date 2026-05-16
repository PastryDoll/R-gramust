use rand::prelude::*;
use rustc_hash::{FxHashMap, FxHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Instant;

type NgramShard<'a> = FxHashMap<&'a [char], Vec<(char, u32)>>;

pub struct Ngram<'a> {
    pub shards: Vec<NgramShard<'a>>,
    pub max_window: usize,
}

fn get_shard(key: &[char], num_shards: usize) -> usize {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    (hasher.finish() % num_shards as u64) as usize
}

fn sample_next(window: &[char], shards: &[NgramShard], rng: &mut impl Rng) -> Option<char> {
    // Try progressively shorter windows
    for size in (1..=window.len()).rev() {
        let key: &[char] = &window[window.len() - size..];
        let shard = &shards[get_shard(key, shards.len())];
        if let Some(nexts) = shard.get(key) {
            let total: u32 = nexts.iter().map(|(_, c)| *c).sum();
            let mut roll: u32 = rng.random_range(0..total);
            for &(ch, count) in nexts {
                if roll < count {
                    return Some(ch);
                }
                roll -= count
            }
        } else {
            eprintln!("Window {:?} dont exist in vocabulary", key);
        }
    }
    None
}

/// Scans chars[start..end], return build N shards with counts.
fn count_chunk<'a>(
    chars: &'a [char],
    start: usize,
    end: usize,
    num_shards: usize,
    max_window: usize,
) -> Vec<NgramShard<'a>> {
    let mut shards: Vec<FxHashMap<&'a [char], Vec<(char, u32)>>> =
        (0..num_shards).map(|_| FxHashMap::default()).collect();

    for i in start..end {
        let next_char = chars[i];
        for size in 1..=max_window {
            if i < size {
                continue;
            }
            let window: &'a [char] = &chars[i - size..i];

            let s = get_shard(window, shards.len());
            let entry = shards[s].entry(window).or_default();
            if let Some(slot) = entry.iter_mut().find(|(c, _)| *c == next_char) {
                slot.1 += 1;
            } else {
                entry.push((next_char, 1));
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
            for (ch, count) in nexts {
                if let Some(slot) = entry.iter_mut().find(|(c, _)| *c == ch) {
                    slot.1 += count;
                } else {
                    entry.push((ch, count));
                }
            }
        }
    }
    merged
}

pub fn read_chars(file_path: &Path) -> std::io::Result<Vec<char>> {
    let content = fs::read_to_string(file_path)?;
    Ok(content.trim().chars().collect())
}

pub fn build_ngram<'a>(chars: &'a [char], max_window: usize) -> Ngram<'a> {
    let n = chars.len();
    let num_threads = thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(8);

    let num_shards = num_threads;
    let chunk_size = n.div_ceil(num_threads);
    println!("Using {num_threads} threads / {num_shards} shards to index");

    let t0 = Instant::now();
    // counts = vec of shards each shards is disjoint in the space of keys.
    let counts: Vec<FxHashMap<&[char], Vec<(char, u32)>>> = thread::scope(|scope| {
        // Count into each shard
        let mut count_handles = Vec::new();
        for t in 0..num_threads {
            let start = t * chunk_size;
            let end = ((t + 1) * chunk_size).min(n);
            let chars = &chars;
            count_handles
                .push(scope.spawn(move || count_chunk(chars, start, end, num_shards, max_window)));
        }

        // grid = each row is a vec of shards
        let grid: Vec<Vec<FxHashMap<&[char], Vec<(char, u32)>>>> = count_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();
        eprintln!("count phase: {:?}", t0.elapsed());

        // transpose so each column corresponds to a shard_id.
        let t1 = Instant::now();
        let mut columns: Vec<Vec<FxHashMap<&[char], Vec<(char, u32)>>>> =
            (0..num_shards).map(|_| Vec::new()).collect();
        for row in grid {
            for (k, submap) in row.into_iter().enumerate() {
                columns[k].push(submap);
            }
        }

        // Merge each shard's column independently — disjoint keyspaces.
        let mut merge_handles = Vec::new();
        for column in columns {
            merge_handles.push(scope.spawn(move || merge_shards(column)));
        }
        let shard_maps: Vec<FxHashMap<&[char], Vec<(char, u32)>>> = merge_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();
        eprintln!("merge phase: {:?}", t1.elapsed());

        shard_maps
    });

    println!("Indexing done!");
    Ngram {
        shards: counts,
        max_window,
    }
}

pub fn print_samples(ngram: &Ngram, seed: &str, num_sample: usize) -> () {
    print!("Enter some text that will be autocompleted: ");
    // io::stdout().flush()?;
    // let mut input = String::new();
    // io::stdin().read_line(&mut input)?;
    // let input = input.trim();
    let input = String::from(seed);
    let input_chars: Vec<char> = input.chars().collect();
    let take = input_chars.len().min(ngram.max_window);
    let tail = &input_chars[input_chars.len() - take..];

    let mut window: Vec<char> = Vec::new();
    for size in (1..=tail.len()).rev() {
        let candidate: &[char] = &tail[tail.len() - size..];
        if ngram.shards[get_shard(candidate, ngram.shards.len())].contains_key(candidate) {
            window = candidate.to_vec();
            break;
        }
    }

    if window.is_empty() {
        window.push(' ');
    }

    let mark_start = input_chars.len() - window.len().min(input_chars.len());
    for (i, c) in input_chars.iter().enumerate() {
        if i == mark_start {
            print!("|");
        }
        print!("{c}");
    }
    print!("|");

    let mut rng = rand::rng();
    for _ in 0..num_sample {
        if let Some(next) = sample_next(&window, &ngram.shards, &mut rng) {
            print!("{next}");
            // io::stdout().flush().unwrap();
            // // Sleep to give the impression of thinking! - Caio
            // thread::sleep(Duration::from_millis(20));
            if window.len() == ngram.max_window {
                window.remove(0);
            }
            window.push(next);
        } else {
            break;
        }
    }
    println!();
}
