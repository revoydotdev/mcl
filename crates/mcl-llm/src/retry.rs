use std::future::Future;
use std::time::Duration;

use reqwest::Response;
use tokio::time::sleep;

use mcl_spec::MclError;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_SECS: u64 = 1;
const RETRYABLE_STATUS_CODES: [u16; 4] = [429, 500, 502, 503];

pub fn backoff_duration(attempt: u32) -> Duration {
    Duration::from_secs(INITIAL_BACKOFF_SECS * 2u64.pow(attempt))
}

pub fn is_retryable_status(status: u16) -> bool {
    RETRYABLE_STATUS_CODES.contains(&status)
}

pub async fn with_retry<F, Fut>(make_request: F) -> Result<Response, MclError>
where
    F: Fn(u32) -> Fut,
    Fut: Future<Output = Result<Response, MclError>>,
{
    let mut attempt = 0u32;

    loop {
        let resp = make_request(attempt).await;

        match resp {
            Ok(response) => {
                let status = response.status().as_u16();
                if !is_retryable_status(status) || attempt >= MAX_RETRIES - 1 {
                    return Ok(response);
                }

                let delay =
                    parse_retry_after(&response).unwrap_or_else(|| backoff_duration(attempt));

                drop(response);
                sleep(delay).await;
                attempt += 1;
            }
            Err(e) => {
                if attempt >= MAX_RETRIES - 1 {
                    return Err(e);
                }
                sleep(backoff_duration(attempt)).await;
                attempt += 1;
            }
        }
    }
}

fn parse_retry_after(response: &Response) -> Option<Duration> {
    let header = response.headers().get("retry-after")?.to_str().ok()?;

    if let Ok(secs) = header.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_each_attempt() {
        assert_eq!(backoff_duration(0), Duration::from_secs(1));
        assert_eq!(backoff_duration(1), Duration::from_secs(2));
        assert_eq!(backoff_duration(2), Duration::from_secs(4));
    }

    #[test]
    fn retryable_status_codes() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(403));
        assert!(!is_retryable_status(404));
    }
}
