use rustc_hash::FxHashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

type Merges = Vec<(u16, u16)>;
type MergeMap = FxHashMap<(u16, u16), u16>;

pub struct BpeModel {
    pub merges: Merges,
    pub merge_map: MergeMap,
    pub tokens: Vec<u16>,
}

const DEAD: u16 = u16::MAX;

fn build_counts_and_positions(
    tokens: &[u16],
) -> (
    FxHashMap<(u16, u16), u32>,
    FxHashMap<(u16, u16), Vec<(usize, usize)>>,
) {
    let mut counts: FxHashMap<(u16, u16), u32> = FxHashMap::default();
    let mut positions: FxHashMap<(u16, u16), Vec<(usize, usize)>> = FxHashMap::default();
    for i in 0..tokens.len().saturating_sub(1) {
        let pair = (tokens[i], tokens[i + 1]);
        *counts.entry(pair).or_insert(0) += 1;
        positions.entry(pair).or_default().push((i, i + 1));
    }
    (counts, positions)
}

fn update_counts(
    tokens: &mut Vec<u16>,
    counts: &mut FxHashMap<(u16, u16), u32>,
    positions: &mut FxHashMap<(u16, u16), Vec<(usize, usize)>>,
    pair: (u16, u16),
    new_token: u16,
) {
    let pair_positions = positions
        .remove(&pair)
        .expect("pair in counts must have positions");

    for (lpos, rpos) in pair_positions {
        if tokens[lpos] != pair.0 || tokens[rpos] != pair.1 {
            continue;
        }
        let left = (0..lpos).rev().find(|&p| tokens[p] != DEAD);
        let right = (rpos + 1..tokens.len()).find(|&p| tokens[p] != DEAD);

        if let Some(lp) = left {
            let old_left = (tokens[lp], pair.0);
            counts
                .entry(old_left)
                .and_modify(|c| *c = c.saturating_sub(1));
            if counts.get(&old_left) == Some(&0) {
                counts.remove(&old_left);
            }
            let new_left = (tokens[lp], new_token);
            *counts.entry(new_left).or_insert(0) += 1;
            positions.entry(new_left).or_default().push((lp, lpos));
        }

        if let Some(rp) = right {
            let old_right = (pair.1, tokens[rp]);
            counts
                .entry(old_right)
                .and_modify(|c| *c = c.saturating_sub(1));
            if counts.get(&old_right) == Some(&0) {
                counts.remove(&old_right);
            }
            let new_right = (new_token, tokens[rp]);
            *counts.entry(new_right).or_insert(0) += 1;
            positions.entry(new_right).or_default().push((lpos, rp));
        }

        tokens[lpos] = new_token;
        tokens[rpos] = DEAD;
    }

    counts.remove(&pair);
}

//TODO: optimize this
fn most_frequent_u16(counts: &FxHashMap<(u16, u16), u32>) -> Option<((u16, u16), u32)> {
    counts.iter().max_by_key(|(_, &v)| v).map(|(k, v)| (*k, *v))
}

pub fn decode_token(tok: u16, decode_map: &FxHashMap<u16, (u16, u16)>, out: &mut Vec<u8>) {
    if tok < 256 {
        out.push(tok as u8);
    } else {
        let (a, b) = decode_map[&tok];
        decode_token(a, decode_map, out);
        decode_token(b, decode_map, out);
    }
}

pub fn encode(text: &str, merge_map: &MergeMap) -> Vec<u16> {
    let mut tokens: Vec<u16> = text.bytes().map(|b| b as u16).collect();

    loop {
        // To replicate the BPE process we need to find the pair with smalles token_id
        let mut best: Option<(usize, u16)> = None;
        for i in 0..tokens.len().saturating_sub(1) {
            let pair = (tokens[i], tokens[i + 1]);
            if let Some(&new_tok) = merge_map.get(&pair) {
                if best.map_or(true, |(_, b)| new_tok < b) {
                    best = Some((i, new_tok));
                }
            }
        }
        // No more pairs to merge
        let Some((pos, new_tok)) = best else {
            break;
        };

        let mut write = 0;
        let mut read = 0;
        let pair = (tokens[pos], tokens[pos + 1]);
        while read < tokens.len() {
            if read + 1 < tokens.len() && tokens[read] == pair.0 && tokens[read + 1] == pair.1 {
                tokens[write] = new_tok;
                read += 2;
            } else {
                tokens[write] = tokens[read];
                read += 1;
            }
            write += 1;
        }
        tokens.truncate(write);
    }

    tokens
}

pub fn bpe(file_path: &Path, num_merges: usize) -> std::io::Result<BpeModel> {
    assert!(
        num_merges <= (u16::MAX - 255) as usize,
        "num_merges {} would overflow u16 token space (max {})",
        num_merges,
        u16::MAX - 255
    );
    let bytes = fs::read(file_path)?;
    let mut tokens: Vec<u16> = bytes.iter().map(|&b| b as u16).collect();
    let bytes_len = bytes.len();
    drop(bytes);

    let mut merges = Merges::new();
    let mut merge_map = MergeMap::default();

    let t0 = Instant::now();
    let (mut counts, mut positions) = build_counts_and_positions(&tokens);
    eprintln!("BPE build init: {:?}", t0.elapsed());

    let mut time_most_freq = std::time::Duration::ZERO;
    let mut time_update = std::time::Duration::ZERO;

    for i in 0..num_merges {
        let t = Instant::now();
        let Some((pair, freq)) = most_frequent_u16(&counts) else {
            break;
        };
        // This should never happen
        if freq == 0 {
            break;
        }
        time_most_freq += t.elapsed();

        let new_token = 256 + i as u16;

        let t = Instant::now();
        update_counts(&mut tokens, &mut counts, &mut positions, pair, new_token);
        time_update += t.elapsed();

        merges.push(pair);
        merge_map.insert(pair, new_token);
    }

    eprintln!("BPE most_frequent total: {:?}", time_most_freq);
    eprintln!("BPE update_counts total: {:?}", time_update);

    tokens.retain(|&t| t != DEAD); // removes DEADS

    println!(
        "Done! {} merges, {} tokens remaining (was {})",
        merges.len(),
        tokens.len(),
        bytes_len
    );

    Ok(BpeModel {
        merges,
        merge_map,
        tokens,
    })
}
