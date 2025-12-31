# Crossword Solver Service

A Rust web service (Axum + Tokio) that loads a wordlist into an in-memory bitset index and serves pattern-based word matches with pagination. Words are normalized to lowercase ASCII, bucketed by length, and indexed with positional bitsets so each query ANDs the relevant positions to rapidly filter candidates; optional must/cannot letter filters use contains bitsets. A simple Bootstrap front-end at `/` lets you pick word length, type a pattern (letters + blanks), and scroll through results; the API lives at `/v1/matches`, and `/healthz` reports readiness. Robots are disallowed via `/robots.txt`.

Word list attribution: sourced from [SpreadTheWordlist.com](https://www.spreadthewordlist.com/) under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).

## Running locally
1) Prereqs: Rust toolchain (`rustup`), and a word list file (default `words.txt` in the repo).
2) Install deps and run tests:
   ```bash
   cargo test
   ```
3) Start the server (defaults to `0.0.0.0:8080` and `WORDLIST_PATH=words.txt`):
   ```bash
   cargo run
   ```
4) Open http://localhost:8080/ to use the UI, or call the API:
   ```bash
   curl "http://localhost:8080/v1/matches?pattern=a__le&page=1&page_size=50"
   ```

## Configuration
- `HOST` (default `0.0.0.0`)
- `PORT` (default `8080`)
- `WORDLIST_PATH` (default `/app/words.txt`; override to point at your list)
- `RUST_LOG` (set log level, e.g., `debug`)
- CLI flag: `--no-cache` disables cache-control headers (useful during local dev or when proxies get in the way)
- `RATE_LIMIT_RPS` (default 5) and `RATE_LIMIT_BURST` (default 10) control the per-IP rate limiter (only applied when `Fly-Client-IP` header is present)

## Build container
```bash
docker build -t crosswordsolver .
docker run -p 8080:8080 crosswordsolver
```
