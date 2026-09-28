//! Fixed-window per-IP rate limiter for the loopback REST API (R4 #4).
//!
//! Dependency-free actix-web middleware: each client IP gets `max_requests`
//! requests per `window`; excess requests get `429 Too Many Requests` with a
//! `Retry-After` header. Old timestamps are evicted on every check, so the
//! window slides and bursts decay naturally.
//!
//! The API binds 127.0.0.1 only, so this is a backstop against runaway local
//! scripts hammering the API — not a substitute for the API-key gate. It is
//! deliberately applied to the REST API server only, never to the streaming
//! server, where legitimate video range-request bursts must not be throttled.

use std::collections::{HashMap, VecDeque};
use std::future::{ready, Future, Ready};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header;
use actix_web::{Error, HttpResponse};

/// Generous defaults: the frontend only makes user-driven requests, so 240
/// requests per minute per IP never trips in normal use.
pub const DEFAULT_MAX_REQUESTS: usize = 240;
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct RateLimitState {
    max_requests: usize,
    window: Duration,
    hits: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl RateLimitState {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Record a hit for `ip`. Returns `Ok(())` when allowed, or `Err(wait)`
    /// with how long the client should back off when over the limit.
    pub fn check(&self, ip: IpAddr) -> Result<(), Duration> {
        let mut hits = self.hits.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        // Evict timestamps outside the window; drop the entry entirely when
        // it goes quiet so the map cannot grow without bound.
        let mut queue = hits.remove(&ip).unwrap_or_default();
        while queue
            .front()
            .is_some_and(|seen| now.duration_since(*seen) >= self.window)
        {
            queue.pop_front();
        }
        if queue.len() >= self.max_requests {
            let wait = queue
                .front()
                .map(|seen| self.window.saturating_sub(now.duration_since(*seen)))
                .unwrap_or(self.window)
                .max(Duration::from_secs(1));
            if !queue.is_empty() {
                hits.insert(ip, queue);
            }
            return Err(wait);
        }
        queue.push_back(now);
        hits.insert(ip, queue);
        Ok(())
    }
}

pub struct RateLimit {
    state: Arc<RateLimitState>,
}

impl RateLimit {
    pub fn with_defaults() -> Self {
        Self {
            state: Arc::new(RateLimitState::new(DEFAULT_MAX_REQUESTS, DEFAULT_WINDOW)),
        }
    }
}

pub struct RateLimitMiddleware<S> {
    service: S,
    state: Arc<RateLimitState>,
}

impl<S, B> Transform<S, ServiceRequest> for RateLimit
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RateLimitMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitMiddleware {
            service,
            state: self.state.clone(),
        }))
    }
}

impl<S, B> Service<ServiceRequest> for RateLimitMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    // Boxed so the allow- and deny-paths share one future type with no
    // manual pin projection and no extra dependencies.
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Without peer info (unit tests, unix sockets) there is nothing to
        // key on — allow the request through.
        let over_limit = match req.peer_addr() {
            Some(addr) => self.state.check(addr.ip()).err(),
            None => None,
        };
        match over_limit {
            None => {
                let fut = self.service.call(req);
                Box::pin(async move { fut.await.map(|res| res.map_into_left_body()) })
            }
            Some(wait) => Box::pin(ready(Ok(req
                .into_response(
                    HttpResponse::TooManyRequests()
                        .insert_header((header::RETRY_AFTER, wait.as_secs().to_string()))
                        .body("Too many requests, slow down"),
                )
                .map_into_right_body()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    const TEST_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    const OTHER_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

    #[test]
    fn allows_up_to_the_limit_then_rejects_with_backoff() {
        let state = RateLimitState::new(3, Duration::from_secs(60));
        assert!(state.check(TEST_IP).is_ok());
        assert!(state.check(TEST_IP).is_ok());
        assert!(state.check(TEST_IP).is_ok());
        let wait = state.check(TEST_IP).expect_err("4th hit must be rejected");
        assert!(wait >= Duration::from_secs(1));
        assert!(wait <= Duration::from_secs(60));
    }

    #[test]
    fn tracks_each_ip_independently() {
        let state = RateLimitState::new(1, Duration::from_secs(60));
        assert!(state.check(TEST_IP).is_ok());
        assert!(state.check(TEST_IP).is_err());
        assert!(state.check(OTHER_IP).is_ok());
    }

    #[test]
    fn window_expiry_reallows_requests() {
        let state = RateLimitState::new(1, Duration::from_millis(50));
        assert!(state.check(TEST_IP).is_ok());
        assert!(state.check(TEST_IP).is_err());
        std::thread::sleep(Duration::from_millis(70));
        assert!(state.check(TEST_IP).is_ok());
    }

    #[test]
    fn quiet_entries_are_evicted_so_the_map_stays_small() {
        let state = RateLimitState::new(1, Duration::from_millis(20));
        assert!(state.check(TEST_IP).is_ok());
        std::thread::sleep(Duration::from_millis(40));
        // This check evicts the stale timestamps and drops the empty entry.
        assert!(state.check(TEST_IP).is_ok());
        let hits = state.hits.lock().unwrap();
        assert_eq!(hits.get(&TEST_IP).map(|q| q.len()), Some(1));
    }
}
