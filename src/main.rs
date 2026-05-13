use rand::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

const MAX_WINDOW: usize = 5;

fn sample_next(
    window: &[char],
    window_to_probs: &HashMap<String, Vec<(char, f64)>>,
    rng: &mut impl Rng,
) -> Option<char> {
    // Try progressively shorter windows
    for size in (1..=window.len()).rev() {
        let key: String = window[window.len() - size..].iter().collect();
        if let Some(nexts) = window_to_probs.get(&key) {
            let roll: f64 = rng.random();
            let mut cumulative = 0.0;
            for &(ch, prob) in nexts {
                cumulative += prob;
                if roll < cumulative {
                    return Some(ch);
                }
            }
        } else {
            eprintln!("Window {key} dont exist in vocabulary");
        }
    }
    None
}

fn main() -> std::io::Result<()> {
    let file_name = "shakespeare_full.txt";
    println!("Generating index for {file_name}");
    let mut counts: HashMap<String, Vec<(char, u32)>> = HashMap::new();
    {
        let content: String = fs::read_to_string(format!("data/{file_name}"))?;
        let chars: Vec<char> = content.trim().chars().collect();

        for i in 0..chars.len() {
            let next_char = chars[i];
            for size in 1..=MAX_WINDOW {
                if i < size {
                    continue;
                }
                let window: String = chars[i - size..i].iter().collect();
                let entry = counts.entry(window).or_default();
                if let Some(slot) = entry.iter_mut().find(|(c, _)| *c == next_char) {
                    slot.1 += 1;
                } else {
                    entry.push((next_char, 1));
                }
            }
        }
    }

    let window_to_probs: HashMap<String, Vec<(char, f64)>> = counts
        .into_iter()
        .map(|(window, nexts)| {
            let total: u32 = nexts.iter().map(|(_, c)| *c).sum();
            let probs: Vec<(char, f64)> = nexts
                .into_iter()
                .map(|(ch, count)| (ch, count as f64 / total as f64))
                .collect();
            (window, probs)
        })
        .collect();
    println!("Indexing done!");

    // generate text
    print!("Enter some text that will be autocompleted: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    let input_chars: Vec<char> = input.chars().collect();
    let take = input_chars.len().min(MAX_WINDOW);
    let tail = &input_chars[input_chars.len() - take..];

    let mut window: Vec<char> = Vec::new();
    for size in (1..=tail.len()).rev() {
        let candidate: String = tail[tail.len() - size..].iter().collect();
        if window_to_probs.contains_key(&candidate) {
            window = tail[tail.len() - size..].to_vec();
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
    for _ in 0..5500 {
        if let Some(next) = sample_next(&window, &window_to_probs, &mut rng) {
            print!("{next}");
            io::stdout().flush().unwrap();
            // Sleep to give the impression of thinking! - Caio
            thread::sleep(Duration::from_millis(20));
            if window.len() == MAX_WINDOW {
                window.remove(0);
            }
            window.push(next);
        } else {
            break;
        }
    }
    println!();
    Ok(())
}

