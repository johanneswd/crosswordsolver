# wordnet-db

Load WordNet dictionaries with zero-copy text and expose full-fidelity records. Backed by [`wordnet-types`](https://crates.io/crates/wordnet-types) for shared layouts and designed to plug straight into [`wordnet-morphy`](https://crates.io/crates/wordnet-morphy) for lemmatization checks.

## Why it's fast
- Memory-map or buffer the canonical `data.*`/`index.*` files (choose at runtime) and borrow all text directly from them.
- Minimal copying: lemmas, pointer symbols, glosses, and indices stay as `&str`; numeric fields keep their raw offsets and IDs.
- Single-pass parsing builds dense in-memory maps for lemma existence, synset lookup, and streaming iteration.

## Related crates
- [`wordnet-types`](https://crates.io/crates/wordnet-types): shared zero-copy structs used by this loader.
- [`wordnet-morphy`](https://crates.io/crates/wordnet-morphy): uses `WordNet::lemma_exists` (or any equivalent predicate) to verify candidates.

## Docs
- API reference: https://docs.rs/wordnet-db
