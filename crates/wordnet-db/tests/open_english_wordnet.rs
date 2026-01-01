use std::env;
use std::path::PathBuf;

use wordnet_db::{LoadMode, WordNet};
use wordnet_types::Pos;

fn dict_dir() -> Option<PathBuf> {
    env::var("WORDNET_DIR").ok().map(PathBuf::from)
}

#[test]
fn loads_open_english_wordnet() {
    let Some(dir) = dict_dir() else {
        eprintln!("skipping: WORDNET_DIR not set");
        return;
    };
    let wn = WordNet::load_with_mode(&dir, LoadMode::Mmap).expect("load open english wordnet");

    assert!(wn.index_count() > 10_000, "index too small");
    assert!(wn.synset_count() > 10_000, "synsets too small");
    assert!(wn.lemma_exists(Pos::Noun, "dog"));
    assert!(wn.lemma_exists(Pos::Verb, "run"));
}
