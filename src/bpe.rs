use std::fs;
use std::path::Path;

fn count_pairs(bytes: &[u8]) -> Box<[[u32; 256]; 256]> {
    let mut counts = Box::new([[0u32; 256]; 256]);
    for pair in bytes.windows(2) {
        counts[pair[0] as usize][pair[1] as usize] += 1;
    }
    counts
}

fn most_frequent_pair(counts: &[[u32; 256]; 256]) -> (u8, u8, u32) {
    let (mut best_a, mut best_b, mut best_count) = (0u8, 0u8, 0u32);
    for a in 0..256 {
        for b in 0..256 {
            if counts[a][b] > best_count {
                best_count = counts[a][b];
                best_a = a as u8;
                best_b = b as u8;
            }
        }
    }
    (best_a, best_b, best_count)
}

pub fn bpe(file_path: &Path) -> std::io::Result<Vec<u8>> {
    let bytes = fs::read(file_path)?;
    let counts = count_pairs(&bytes);
    let (a, b, count) = most_frequent_pair(&counts);
    println!(
        "Most frequent pair: ({} {}) -> {}",
        a as char, b as char, count
    );

    let counts_ref = &counts;
    let mut pairs: Vec<(u8, u8, u32)> = (0..256usize)
        .flat_map(|a| (0..256usize).map(move |b| (a as u8, b as u8, counts_ref[a][b])))
        .filter(|&(_, _, c)| c > 0)
        .collect();
    pairs.sort_unstable_by(|a, b| b.2.cmp(&a.2));
    for (a, b, count) in pairs.iter().take(50) {
        println!("({} {}) -> {}", *a as char, *b as char, count);
    }

    Ok(bytes)
}

