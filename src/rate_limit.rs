use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rocket::request::{FromRequest, Outcome, Request};

/// Fixed-window rate limiter keyed by arbitrary string (e.g. client IP).
pub struct RateLimiter {
    window: Duration,
    default_limit: u64,
    buckets: Mutex<HashMap<String, (Instant, u64)>>,
}

/// Client IP address extracted from the request.
/// Checks: X-Forwarded-For → X-Real-Ip → socket peer → "unknown".
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientIp {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Some(xff) = request.headers().get_one("X-Forwarded-For") {
            if let Some(first_ip) = xff.split(',').next() {
                let ip = first_ip.trim();
                if !ip.is_empty() {
                    return Outcome::Success(ClientIp(ip.to_string()));
                }
            }
        }
        if let Some(real_ip) = request.headers().get_one("X-Real-Ip") {
            let ip = real_ip.trim();
            if !ip.is_empty() {
                return Outcome::Success(ClientIp(ip.to_string()));
            }
        }
        if let Some(addr) = request.client_ip() {
            return Outcome::Success(ClientIp(addr.to_string()));
        }
        Outcome::Success(ClientIp("unknown".to_string()))
    }
}

/// Result of a rate limit check.
#[derive(Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub limit: u64,
    pub remaining: u64,
    pub reset_secs: u64,
}

impl RateLimiter {
    pub fn new(window: Duration, default_limit: u64) -> Self {
        RateLimiter {
            window,
            default_limit,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn check_default(&self, key_id: &str) -> RateLimitResult {
        self.check(key_id, self.default_limit)
    }

    pub fn check(&self, key_id: &str, limit: u64) -> RateLimitResult {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();

        let entry = buckets
            .entry(key_id.to_string())
            .or_insert_with(|| (now, 0));

        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }

        let reset_secs = self
            .window
            .checked_sub(now.duration_since(entry.0))
            .unwrap_or(Duration::ZERO)
            .as_secs();

        if entry.1 >= limit {
            RateLimitResult {
                allowed: false,
                limit,
                remaining: 0,
                reset_secs,
            }
        } else {
            entry.1 += 1;
            RateLimitResult {
                allowed: true,
                limit,
                remaining: limit.saturating_sub(entry.1),
                reset_secs,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_limit() {
        let rl = RateLimiter::new(Duration::from_secs(60), 10);
        let r = rl.check_default("ip1");
        assert!(r.allowed);
        assert_eq!(r.remaining, 9);
    }

    #[test]
    fn blocks_at_limit() {
        let rl = RateLimiter::new(Duration::from_secs(60), 3);
        for _ in 0..3 {
            rl.check_default("ip1");
        }
        assert!(!rl.check_default("ip1").allowed);
    }

    #[test]
    fn separate_keys_independent() {
        let rl = RateLimiter::new(Duration::from_secs(60), 3);
        for _ in 0..3 {
            rl.check_default("ip1");
        }
        assert!(!rl.check_default("ip1").allowed);
        assert!(rl.check_default("ip2").allowed);
    }
}
