# Crossword Solver Service

A Rust web service (Axum + Tokio) that loads a wordlist into an in-memory bitset index and serves pattern-based word matches with pagination. Words are normalized to lowercase ASCII, bucketed by length, and indexed with positional bitsets so each query ANDs the relevant positions to rapidly filter candidates; optional must/cannot letter filters use contains bitsets. A simple Bootstrap front-end at `/` lets you pick word length, type a pattern (letters + blanks), and scroll through results; the API lives at `/v1/matches`, and `/healthz` reports readiness. Robots are disallowed via `/robots.txt`.

WordNet is bundled for dictionary + related-word lookups (used by the popovers and the synonyms page) via `/v1/wordnet/dictionary` and `/v1/wordnet/related`.

Word list attribution: sourced from [SpreadTheWordlist.com](https://www.spreadthewordlist.com/) under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).

## Workspace crates
- `crosswordsolver-jw`: Axum HTTP service + UI (binary only, not published).
- `wordnet-types`: Basic shared types for WordNet data.
- `wordnet-db`: Memory-mapped access to a prepared WordNet database file.
- `wordnet-morphy`: WordNet morphology helpers and tests.
- `xtask`: Internal tooling for tag checks/publishing (runs via `cargo run -p xtask ...`).

## Running locally
1) Prereqs: Rust toolchain (`rustup`), and a word list file (default `words.txt` in the repo).
2) Install deps and run tests:
   ```bash
   cargo test -p crosswordsolver-jw
   ```
3) Ensure WordNet data is present (default `open_english_wordnet_2024/oewn2024` from `download_wordnet.py`, or supply `--wordnet-dir <path>`).
4) Start the server (defaults to `0.0.0.0:8080` and `WORDLIST_PATH=words.txt`):
   ```bash
   cargo run -p crosswordsolver-jw --bin crosswordsolver
   ```
5) Open http://localhost:8080/ to use the UI, or call the API:
   ```bash
   curl "http://localhost:8080/v1/matches?pattern=a__le&page=1&page_size=50"
   curl "http://localhost:8080/v1/wordnet/dictionary?word=dogs"
   ```

## Configuration
- `HOST` (default `0.0.0.0`)
- `PORT` (default `8080`)
- `WORDLIST_PATH` (default `/app/words.txt`; override to point at your list)
- `WORDNET_DIR` (default `/app/wordnet` in Docker or `open_english_wordnet_2024/oewn2024` locally)
- `WORDNET_LOAD_MODE` (`mmap` default, `owned` to read files into memory)
- `RUST_LOG` (set log level, e.g., `debug`)
- CLI flag: `--no-cache` disables cache-control headers (useful during local dev or when proxies get in the way)
- CLI flags: `--wordnet-dir <path>` to point at a downloaded dict; `--wordnet-mode=owned|mmap` to override load mode
- `RATE_LIMIT_RPS` (default 5) and `RATE_LIMIT_BURST` (default 10) control the per-IP rate limiter (only applied when `Fly-Client-IP` header is present)

## CI/CD
- GitHub Actions (`.github/workflows/ci.yml`) runs fmt, clippy, tests, and `xtask check-versions` on pushes/PRs to keep crates publishable.
- Tag pushes `vX.Y.Z` trigger release workflow (`.github/workflows/release.yml`) running fmt/clippy/test, `xtask check-tag`, then `xtask publish` (needs `CARGO_REGISTRY_TOKEN` secret).
- CI caches Cargo registry/git and `target` using a job-local `CARGO_HOME`.

## Helper scripts
- `scripts/build_wordlist.py`: regenerate a normalized `words.txt` from source lists.
- `download_wordnet.py`: download/extract Open English WordNet data for local testing.

## xtask helpers
- `cargo run -p xtask -- bump-version --version vX.Y.Z`: bump workspace version, sync path-dependency versions, and refresh `Cargo.lock`.
- `cargo run -p xtask -- check-versions`: verify workspace crate versions match and internal deps use explicit matching versions.
- `cargo run -p xtask -- check-tag --tag vX.Y.Z`: ensure tag matches workspace version and publishable crates are aligned.
- `cargo run -p xtask -- publish --tag vX.Y.Z [--dry-run]`: publish crates in dependency order.
- `cargo run -p xtask -- use-path-deps`: switch workspace internal deps to path-only for local development and update `Cargo.lock`.

## Build container
```bash
docker build -t crosswordsolver .
docker run -p 8080:8080 crosswordsolver
```
The image downloads and bundles Open English WordNet under `/app/wordnet` by default.
