use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{Level, info};
use tracing_subscriber::EnvFilter;
use wordnet_db::{LoadMode, WordNet};
use wordnet_morphy::Morphy;

use crosswordsolver_jw::rate_limit::RateLimiterLayer;
use crosswordsolver_jw::{AppState, WordIndex, router};

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_WORDLIST: &str = "words.txt";
const DEFAULT_WORDNET_PATH: &str = "open_english_wordnet_2024/oewn2024";
const DEFAULT_WORDNET_IMAGE_PATH: &str = "/app/wordnet";
const MAX_PAGE_SIZE: usize = 500;
const DEFAULT_RATE_LIMIT_RPS: u32 = 5;
const DEFAULT_RATE_LIMIT_BURST: u32 = 10;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = load_config();
    info!("binding to {}:{}", config.host, config.port);
    info!("using wordlist at {}", config.wordlist_path.display());
    info!(
        "using wordnet at {} (mode: {:?})",
        config.wordnet_path.display(),
        config.wordnet_mode
    );
    if config.disable_cache {
        info!("cache headers disabled");
    }
    info!(
        "rate limit: {} req/s (burst {})",
        config.rate_limit_rps, config.rate_limit_burst
    );

    let start = Instant::now();
    let index = WordIndex::build_from_file(&config.wordlist_path)?;
    let elapsed = start.elapsed();
    info!("index built in {} ms", elapsed.as_millis());

    let wn_start = Instant::now();
    let wordnet = Arc::new(WordNet::load_with_mode(
        &config.wordnet_path,
        config.wordnet_mode,
    )?);
    let morphy = Arc::new(Morphy::load(&config.wordnet_path)?);
    info!("wordnet loaded in {} ms", wn_start.elapsed().as_millis());

    let state = AppState {
        index: Arc::clone(&index),
        wordnet,
        morphy,
        max_page_size: MAX_PAGE_SIZE,
        disable_cache: config.disable_cache,
    };

    let rate_limiter = RateLimiterLayer::new(config.rate_limit_rps, config.rate_limit_burst);
    let app = router(state)
        .layer(rate_limiter)
        .layer(TraceLayer::new_for_http());
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("invalid listen address");
    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct Config {
    host: String,
    port: u16,
    wordlist_path: PathBuf,
    wordnet_path: PathBuf,
    wordnet_mode: LoadMode,
    disable_cache: bool,
    rate_limit_rps: u32,
    rate_limit_burst: u32,
}

fn load_config() -> Config {
    let mut disable_cache = false;
    let mut cli_wordnet_dir: Option<PathBuf> = None;
    let mut cli_wordnet_mode: Option<LoadMode> = None;
    let mut args = env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-cache" => disable_cache = true,
            "--wordnet-dir" => {
                if let Some(path) = args.next() {
                    cli_wordnet_dir = Some(PathBuf::from(path));
                }
            }
            _ => {
                if let Some(path) = arg.strip_prefix("--wordnet-dir=") {
                    cli_wordnet_dir = Some(PathBuf::from(path));
                } else if let Some(mode) = arg.strip_prefix("--wordnet-mode=") {
                    cli_wordnet_mode = parse_load_mode(mode);
                }
            }
        }
    }

    let host = env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let wordlist_path = env::var("WORDLIST_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_WORDLIST));
    let wordnet_path = cli_wordnet_dir
        .or_else(|| env::var("WORDNET_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(default_wordnet_path);
    let wordnet_mode = cli_wordnet_mode
        .or_else(|| {
            env::var("WORDNET_LOAD_MODE")
                .ok()
                .as_deref()
                .and_then(parse_load_mode)
        })
        .unwrap_or(LoadMode::Mmap);
    let rate_limit_rps = env::var("RATE_LIMIT_RPS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_RATE_LIMIT_RPS);
    let rate_limit_burst = env::var("RATE_LIMIT_BURST")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_RATE_LIMIT_BURST);

    Config {
        host,
        port,
        wordlist_path,
        wordnet_path,
        wordnet_mode,
        disable_cache,
        rate_limit_rps,
        rate_limit_burst,
    }
}

fn default_wordnet_path() -> PathBuf {
    let local = PathBuf::from(DEFAULT_WORDNET_PATH);
    if local.exists() {
        return local;
    }
    PathBuf::from(DEFAULT_WORDNET_IMAGE_PATH)
}

fn parse_load_mode(raw: &str) -> Option<LoadMode> {
    match raw.to_ascii_lowercase().as_str() {
        "mmap" => Some(LoadMode::Mmap),
        "owned" => Some(LoadMode::Owned),
        _ => None,
    }
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let max_level = env_filter
        .max_level_hint()
        .and_then(|hint| hint.into_level())
        .unwrap_or(Level::INFO);
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_level(true)
        .with_max_level(max_level)
        .init();
}
