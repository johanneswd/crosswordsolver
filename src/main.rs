use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

use crosswordsolver::{router, AppState, WordIndex};

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_WORDLIST: &str = "words.txt";
const MAX_PAGE_SIZE: usize = 500;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let (host, port, wordlist_path) = load_config();
    info!("binding to {}:{}", host, port);
    info!("using wordlist at {}", wordlist_path.display());

    let start = Instant::now();
    let index = WordIndex::build_from_file(&wordlist_path)?;
    let elapsed = start.elapsed();
    info!("index built in {} ms", elapsed.as_millis());

    let state = AppState {
        index: Arc::clone(&index),
        max_page_size: MAX_PAGE_SIZE,
    };

    let app = router(state).layer(TraceLayer::new_for_http());
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .expect("invalid listen address");
    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;
    Ok(())
}

fn load_config() -> (String, u16, PathBuf) {
    let host = env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let wordlist_path = env::var("WORDLIST_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_WORDLIST));
    (host, port, wordlist_path)
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_level(true)
        .with_max_level(Level::INFO)
        .init();
}
