use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tower::{Layer, Service};
use tracing::warn;

const LOG_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct RateLimiter<S> {
    inner: S,
    state: SharedState,
    rate_per_sec: f64,
    burst: f64,
}

#[derive(Clone)]
struct SharedState {
    buckets: std::sync::Arc<DashMap<String, Bucket>>,
    dropped_since_log: std::sync::Arc<std::sync::atomic::AtomicU64>,
    last_log: std::sync::Arc<std::sync::Mutex<Instant>>,
}

#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

#[derive(Clone)]
pub struct RateLimiterLayer {
    rate_per_sec: f64,
    burst: f64,
}

impl RateLimiterLayer {
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            rate_per_sec: rate_per_sec as f64,
            burst: burst as f64,
        }
    }
}

impl<S> Layer<S> for RateLimiterLayer {
    type Service = RateLimiter<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimiter {
            inner,
            state: SharedState {
                buckets: std::sync::Arc::new(DashMap::new()),
                dropped_since_log: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                last_log: std::sync::Arc::new(std::sync::Mutex::new(Instant::now())),
            },
            rate_per_sec: self.rate_per_sec,
            burst: self.burst,
        }
    }
}

impl<S, ReqBody> Service<axum::http::Request<ReqBody>> for RateLimiter<S>
where
    S: Service<axum::http::Request<ReqBody>, Response = axum::http::Response<axum::body::Body>>
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<ReqBody>) -> Self::Future {
        if let Some(client_id) = client_id(&req) {
            if !self.check_and_consume(&client_id) {
                self.state
                    .dropped_since_log
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                log_drops_if_needed(&self.state);
                return Box::pin(async move {
                    Ok(axum::http::Response::builder()
                        .status(axum::http::StatusCode::TOO_MANY_REQUESTS)
                        .body(axum::body::Body::from("rate limited"))
                        .unwrap())
                });
            }
        }

        let fut = self.inner.call(req);
        Box::pin(async move { fut.await })
    }
}

fn client_id<B>(req: &axum::http::Request<B>) -> Option<String> {
    // Trust Fly's proxy header when present.
    req.headers()
        .get("Fly-Client-IP")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
}

impl<S> RateLimiter<S> {
    fn check_and_consume(&self, client: &str) -> bool {
        let mut entry = self
            .state
            .buckets
            .entry(client.to_string())
            .or_insert(Bucket {
                tokens: self.burst,
                last_refill: Instant::now(),
            });
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(entry.last_refill)
            .as_secs_f64();
        if elapsed > 0.0 {
            entry.tokens = (entry.tokens + elapsed * self.rate_per_sec).min(self.burst);
            entry.last_refill = now;
        }
        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

fn log_drops_if_needed(state: &SharedState) {
    let now = Instant::now();
    let mut last = state.last_log.lock().unwrap();
    if now.saturating_duration_since(*last) >= LOG_INTERVAL {
        let dropped = state
            .dropped_since_log
            .swap(0, std::sync::atomic::Ordering::Relaxed);
        if dropped > 0 {
            warn!("rate limiter dropped {dropped} requests in the last minute");
        }
        *last = now;
    }
}
