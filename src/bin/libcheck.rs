use polyglot_tokenizer::{Token, Tokenizer};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

include!("../codegen/packages-set.rs");

fn main() {
    let file = env::args().skip(1).next().expect("No filename provided");
    let file = Path::new(&file);

    let content = fs::read_to_string(&file).expect(&format!("Can't read from file: {:?}", file));
    let tags: HashSet<&'static str> = Tokenizer::new(&content)
        .tokens()
        .filter_map(|token| match token {
            Token::String(_, value, _) | Token::Ident(value) => PACKAGES.get_key(value).copied(),
            _ => None,
        })
        .collect();

    tags.iter().for_each(|tag| println!("{}", tag));
}
