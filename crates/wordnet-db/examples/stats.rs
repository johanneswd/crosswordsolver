use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use wordnet_db::{LoadMode, WordNet};
use wordnet_types::Pos;

fn main() -> Result<()> {
    let dict_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .context("usage: cargo run -p wordnet-db --example stats -- <path-to-wordnet-dir>")?;

    let wn = WordNet::load_with_mode(&dict_dir, LoadMode::Mmap)
        .with_context(|| format!("loading WordNet from {}", dict_dir.display()))?;

    let mut word_count = 0usize;
    let mut pointer_count = 0usize;
    let mut gloss_example_count = 0usize;
    let mut verb_frame_instances = 0usize;

    for syn in wn.iter_synsets() {
        word_count += syn.words.len();
        pointer_count += syn.pointers.len();
        gloss_example_count += syn.gloss.examples.len();
        if syn.id.pos == Pos::Verb {
            verb_frame_instances += syn.frames.len();
        }
    }

    println!("Dictionary: {}", dict_dir.display());
    println!("Index entries: {}", wn.index_count());
    println!("Lemma keys   : {}", wn.lemma_count());
    println!("Synsets      : {}", wn.synset_count());
    println!("Words in synsets: {}", word_count);
    println!("Pointers     : {}", pointer_count);
    println!("Gloss examples: {}", gloss_example_count);
    println!(
        "Verb frame templates (frames.vrb): {}",
        wn.verb_frame_templates_count()
    );
    println!("Verb frame instances in synsets: {}", verb_frame_instances);
    println!("Sense-count entries: {}", wn.sense_count_entries());

    // Spot-check a couple of lemmas to confirm lookup.
    for (pos, lemma) in [(Pos::Noun, "dog"), (Pos::Verb, "run")] {
        println!(
            "Lemma '{}' ({:?}) exists? {}",
            lemma,
            pos,
            wn.lemma_exists(pos, lemma)
        );
    }

    Ok(())
}
