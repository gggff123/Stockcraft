//! Shared HTTP client: one user agent, conservative timeouts, bounded retries and policy-aware
//! status classification. Providers never construct their own clients.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, RETRY_AFTER};
use reqwest::{Client, Method, StatusCode};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};

use super::rate_limit::RateLimiter;

pub const USER_AGENT: &str =
    "Stockcraft/0.1 (free image discovery desktop app; +https://openverse.org)";

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    limits: Arc<HashMap<&'static str, Arc<Mutex<RateLimiter>>>>,
}

impl HttpClient {
    pub fn new() -> AppResult<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.stop();
                }
                match crate::util::validate_remote_url(attempt.url().as_str()) {
                    Ok(_) => attempt.follow(),
                    Err(_) => attempt.stop(),
                }
            }))
            .build()
            .map_err(|error| AppError::internal(format!("could not build HTTP client: {error}")))?;

        let mut limits = HashMap::new();
        limits.insert(
            "openverse",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(3_100),
                Some(200),
            ))),
        );
        limits.insert(
            "wikimedia",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(1_000),
                None,
            ))),
        );
        limits.insert(
            "nasa",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(1_000),
                None,
            ))),
        );
        limits.insert(
            "cleveland",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(500),
                None,
            ))),
        );
        limits.insert(
            "pexels",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(500),
                None,
            ))),
        );
        limits.insert(
            "pixabay",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(700),
                None,
            ))),
        );
        limits.insert(
            "freepik",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(1_000),
                None,
            ))),
        );
        limits.insert(
            "wellcome",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(1_000),
                None,
            ))),
        );
        limits.insert(
            "smk",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(1_000),
                None,
            ))),
        );
        // The Met needs one request per object, so a short interval keeps an object page
        // responsive while staying well inside the museum's published guidance.
        limits.insert(
            "met",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(120),
                None,
            ))),
        );
        limits.insert(
            "aihorde",
            Arc::new(Mutex::new(RateLimiter::new(
                Duration::from_millis(1_000),
                None,
            ))),
        );
        Ok(Self {
            client,
            limits: Arc::new(limits),
        })
    }

    /// A client for streaming downloads: long read timeout, same redirect safety.
    pub fn download_client() -> AppResult<Client> {
        Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.stop();
                }
                match crate::util::validate_remote_url(attempt.url().as_str()) {
                    Ok(_) => attempt.follow(),
                    Err(_) => attempt.stop(),
                }
            }))
            .build()
            .map_err(|error| {
                AppError::internal(format!("could not build download client: {error}"))
            })
    }

    pub async fn acquire(&self, provider: &str) -> AppResult<()> {
        if let Some(limiter) = self.limits.get(provider) {
            limiter.lock().await.acquire().await?;
        }
        Ok(())
    }

    pub async fn get_json(&self, url: &str, authorization: Option<&str>) -> AppResult<Value> {
        self.send_json(Method::GET, url, None, authorization, None)
            .await
    }

    /// Some providers use a custom authentication header instead of `Authorization`.
    pub async fn get_json_with_header(
        &self,
        url: &str,
        header: &str,
        value: &str,
    ) -> AppResult<Value> {
        self.send_json(Method::GET, url, None, None, Some((header, value)))
            .await
    }

    /// JSON `POST`, used by providers whose search is a job submission (e.g. AI Horde).
    pub async fn post_json_with_header(
        &self,
        url: &str,
        body: &Value,
        header: &str,
        value: &str,
    ) -> AppResult<Value> {
        self.send_json(Method::POST, url, Some(body), None, Some((header, value)))
            .await
    }

    async fn send_json(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
        authorization: Option<&str>,
        extra_header: Option<(&str, &str)>,
    ) -> AppResult<Value> {
        let url = crate::util::validate_remote_url(url)?.to_string();
        // A POST here submits a job (AI Horde), so a retry could create a second job. Only GETs are
        // retried; a failed submission is handled by the caller.
        let attempts = if method == Method::POST { 1 } else { 3 };
        let mut last_error = AppError::Unavailable("request failed".into());
        for attempt in 0..attempts {
            let mut request = self
                .client
                .request(method.clone(), &url)
                .header(ACCEPT, "application/json");
            if let Some(token) = authorization {
                request = request.header(AUTHORIZATION, token);
            }
            if let Some((name, value)) = extra_header {
                request = request.header(name, value);
            }
            if let Some(body) = body {
                request = request.json(body);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return response.json::<Value>().await.map_err(|error| {
                            AppError::Malformed(format!("provider returned invalid JSON: {error}"))
                        });
                    }
                    let retry_after = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok());
                    let body = response.text().await.unwrap_or_default();
                    let snippet = body.chars().take(180).collect::<String>();
                    match status {
                        StatusCode::TOO_MANY_REQUESTS => {
                            let wait = retry_after.unwrap_or(60).min(900);
                            return Err(AppError::RateLimited(format!(
                                "provider rate limit reached (retry after {wait}s)"
                            )));
                        }
                        StatusCode::UNAUTHORIZED => {
                            return Err(AppError::Policy(
                                "the provider rejected the API key (401)".into(),
                            ));
                        }
                        StatusCode::FORBIDDEN | StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => {
                            return Err(AppError::Policy(format!(
                                "provider refused this request (HTTP {status})"
                            )));
                        }
                        StatusCode::NOT_FOUND => {
                            // Never echo the request URL: keyed providers put secrets in the query.
                            return Err(AppError::NotFound(
                                "provider returned 404 for this resource".into(),
                            ));
                        }
                        StatusCode::BAD_REQUEST => {
                            return Err(AppError::Unavailable(format!(
                                "provider rejected the query (HTTP 400){}{}",
                                if snippet.is_empty() { "" } else { ": " },
                                snippet
                            )));
                        }
                        _ if status.is_server_error() => {
                            last_error =
                                AppError::Unavailable(format!("provider error (HTTP {status})"));
                        }
                        _ => {
                            return Err(AppError::Network(format!(
                                "provider returned HTTP {status}{}{}",
                                if snippet.is_empty() { "" } else { ": " },
                                snippet
                            )));
                        }
                    }
                }
                Err(error) => {
                    if error.is_timeout() {
                        last_error = AppError::Timeout("provider request timed out".into());
                    } else {
                        last_error = AppError::Network(format!("network error: {error}"));
                    }
                }
            }
            if attempt + 1 < attempts {
                let jitter = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_millis())
                    .unwrap_or(0)
                    % 120) as u64;
                tokio::time::sleep(Duration::from_millis(300 * 2u64.pow(attempt) + jitter)).await;
            }
        }
        Err(last_error)
    }
}
