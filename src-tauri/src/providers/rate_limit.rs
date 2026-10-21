//! Per-provider minimum-interval limiter with an optional daily budget.
//!
//! Openverse publishes a 20/min burst and a 200/day anonymous budget; the other providers get a
//! conservative serial interval. Waiting happens *before* the request so the provider never sees a
//! burst from Stockcraft.

use std::time::{Duration, Instant, SystemTime};

use crate::error::{AppError, AppResult};

pub struct RateLimiter {
    min_interval: Duration,
    daily_limit: Option<u32>,
    last_request: Option<Instant>,
    window_start: SystemTime,
    used_today: u32,
}

impl RateLimiter {
    pub fn new(min_interval: Duration, daily_limit: Option<u32>) -> Self {
        Self {
            min_interval,
            daily_limit,
            last_request: None,
            window_start: SystemTime::now(),
            used_today: 0,
        }
    }

    pub async fn acquire(&mut self) -> AppResult<()> {
        if self
            .window_start
            .elapsed()
            .map(|elapsed| elapsed >= Duration::from_secs(86_400))
            .unwrap_or(true)
        {
            self.window_start = SystemTime::now();
            self.used_today = 0;
        }
        if let Some(limit) = self.daily_limit {
            if self.used_today >= limit {
                let reset_in = self
                    .window_start
                    .elapsed()
                    .map(|elapsed| Duration::from_secs(86_400).saturating_sub(elapsed))
                    .unwrap_or_default();
                return Err(AppError::RateLimited(format!(
                    "daily anonymous budget exhausted ({limit} requests); resets in {} min",
                    reset_in.as_secs() / 60 + 1
                )));
            }
        }
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                tokio::time::sleep(self.min_interval - elapsed).await;
            }
        }
        self.last_request = Some(Instant::now());
        self.used_today = self.used_today.saturating_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spaces_requests_and_enforces_daily_budget() {
        let mut limiter = RateLimiter::new(Duration::from_millis(30), Some(2));
        let start = Instant::now();
        limiter.acquire().await.unwrap();
        limiter.acquire().await.unwrap();
        assert!(
            start.elapsed() >= Duration::from_millis(30),
            "second request must wait"
        );
        let error = limiter.acquire().await.unwrap_err();
        assert_eq!(error.code(), "rate_limited");
        assert!(error.to_string().contains("daily"));
    }
}
