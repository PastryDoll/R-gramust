use rustc_hash::FxHashMap;
use std::io;
use std::io::Write;
use std::path::Path;
use std::time::Instant;
mod bpe;
mod ngram;
// mod ngram_old;

fn main() -> std::io::Result<()> {
    let file_path = Path::new("data/shakespeare_full.txt");

    // println!("\n--- NO BPE VERSION ---");
    // let chars = ngram_old::read_chars(file_path)?;
    // let ngram_old = ngram_old::build_ngram(&chars, 5);
    // ngram_old::print_samples(&ngram_old, "There", 500);

    println!("\n--- BUILDING BPE AND NGRAM ---\n");
    let tbe = Instant::now();
    let model = bpe::bpe(file_path, 2000)?;
    eprintln!("BPE time: {:?}", tbe.elapsed());

    let tngram = Instant::now();
    let ngram = ngram::build_ngram(&model.tokens, 5);
    eprintln!("Ngram time: {:?}", tngram.elapsed());

    println!("\n--- DONE ---\n");
    let decode_map: FxHashMap<u16, (u16, u16)> = model
        .merges
        .iter()
        .enumerate()
        .map(|(i, &pair)| (256 + i as u16, pair))
        .collect();
    print!("Enter some text that will be autocompleted: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    let seed = bpe::encode(input, &model.merge_map);
    ngram::generate(&ngram, &seed, 5500, &decode_map);

    Ok(())
}
