use std::path::Path;

mod bpe;
//mod ngram;

fn main() -> std::io::Result<()> {
    let file_path = Path::new("data/enwik8.txt");
    bpe::bpe(&file_path)?;
    // let chars = ngram::read_chars(file_path)?;
    // let ngram = ngram::build_ngram(&chars, 5);
    //
    // ngram::print_samples(&ngram, "There", 500);

    Ok(())
}
