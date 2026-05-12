use rand::prelude::*;
use std::collections::HashMap;
use std::fs;

const WINDOW_LENGTH: usize = 5;

fn sample_next(
    window: &[char; WINDOW_LENGTH],
    window_to_probs: &HashMap<[char; WINDOW_LENGTH], Vec<(char, f64)>>,
    rng: &mut impl Rng,
) -> Option<char> {
    let nexts = window_to_probs.get(window)?;
    let roll: f64 = rng.random();
    let mut cumulative = 0.0;
    for &(ch, prob) in nexts {
        cumulative += prob;
        if roll < cumulative {
            return Some(ch);
        }
    }
    None
}
fn main() -> std::io::Result<()> {
    let mut counts: HashMap<[char; WINDOW_LENGTH], Vec<(char, u32)>> = HashMap::new();
    {
        let content: String = fs::read_to_string("data/enwik8.txt")?;
        let mut chars_iter = content.trim().chars();
        let mut window: [char; WINDOW_LENGTH] = [' '; WINDOW_LENGTH];
        for slot in window.iter_mut() {
            *slot = chars_iter.next().unwrap();
        }
        for next_char in chars_iter {
            let entry = counts.entry(window).or_default();
            if let Some(slot) = entry.iter_mut().find(|(c, _)| *c == next_char) {
                slot.1 += 1;
            } else {
                entry.push((next_char, 1));
            }
            window.rotate_left(1);
            window[WINDOW_LENGTH - 1] = next_char;
        }
    }
    let window_to_probs: HashMap<[char; WINDOW_LENGTH], Vec<(char, f64)>> = counts
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
    // generate text
    let seed: Vec<char> = "There".chars().collect();
    let mut window: [char; WINDOW_LENGTH] = seed.as_slice().try_into().unwrap();
    for c in &window {
        print!("{c}");
    }
    let mut rng = rand::rng();
    for _ in 0..500 {
        if let Some(next) = sample_next(&window, &window_to_probs, &mut rng) {
            print!("{next}");
            window.rotate_left(1);
            *window.last_mut().unwrap() = next;
        } else {
            break;
        }
    }
    println!("");
    Ok(())
}
