use std::env;
use std::path::PathBuf;

use wordnet_db::{LoadMode, WordNet};
use wordnet_morphy::Morphy;
use wordnet_types::Pos;

fn dict_dir() -> Option<PathBuf> {
    env::var("WORDNET_DIR").ok().map(PathBuf::from)
}

#[test]
fn resolves_demo_words_against_open_english_wordnet() {
    let Some(dir) = dict_dir() else {
        eprintln!("skipping: WORDNET_DIR not set");
        return;
    };
    let wn = WordNet::load_with_mode(&dir, LoadMode::Mmap).expect("load wordnet");
    let morph = Morphy::load(&dir).expect("load morph");
    let exists = |pos, lemma: &str| wn.lemma_exists(pos, lemma);

    let running = morph.lemmas_for(Pos::Verb, "running", &exists);
    assert!(running.iter().any(|c| c.lemma == "run"));

    let children = morph.lemmas_for(Pos::Noun, "children", &exists);
    assert!(children.iter().any(|c| c.lemma == "child"));

    let better = morph.lemmas_for(Pos::Adj, "better", &exists);
    assert!(!better.is_empty());
}
