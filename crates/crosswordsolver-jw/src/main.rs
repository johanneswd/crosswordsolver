use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{Level, info};
use tracing_subscriber::EnvFilter;

use crosswordsolver_jw::rate_limit::RateLimiterLayer;
use crosswordsolver_jw::{AppState, WordIndex, router};

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_WORDLIST: &str = "words.txt";
const MAX_PAGE_SIZE: usize = 500;
const DEFAULT_RATE_LIMIT_RPS: u32 = 5;
const DEFAULT_RATE_LIMIT_BURST: u32 = 10;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = load_config();
    info!("binding to {}:{}", config.host, config.port);
    info!("using wordlist at {}", config.wordlist_path.display());
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

    let state = AppState {
        index: Arc::clone(&index),
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
    disable_cache: bool,
    rate_limit_rps: u32,
    rate_limit_burst: u32,
}

fn load_config() -> Config {
    let host = env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let wordlist_path = env::var("WORDLIST_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_WORDLIST));
    let disable_cache = env::args().any(|arg| arg == "--no-cache");
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
        disable_cache,
        rate_limit_rps,
        rate_limit_burst,
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
