//! Size-capped reads of HTTP response bodies.
//!
//! reqwest's `.text()`, `.bytes()` and `.json()` buffer the whole body with no
//! limit, so a misbehaving or hostile endpoint can make the shell allocate
//! without bound. These helpers read the body incrementally and refuse, with a
//! `BudgetExceeded` error, once it grows past the caller's limit. The same
//! refusal applies up front when the server declares a `Content-Length` over
//! the limit, before any of the body is read.
//!
//! Model downloads are deliberately not routed through here: they are large by
//! design and stream to disk.

use crate::safety::budget_exceeded;
use anyhow::Result;
use serde::de::DeserializeOwned;

/// Largest provider or plugin response body the shell will buffer (32 MiB).
pub const AI_BODY_MAX: usize = 32 * 1024 * 1024;

fn too_large(max: usize) -> anyhow::Error {
    budget_exceeded(
        "http_body",
        format!("the response body is more than {max} bytes"),
        "the endpoint sent a larger response than the shell accepts; check the endpoint or the request",
    )
}

/// Refuse before reading when the server declares a body larger than `max`.
fn check_declared_length(declared: Option<u64>, max: usize) -> Result<()> {
    match declared {
        Some(n) if n > max as u64 => Err(too_large(max)),
        _ => Ok(()),
    }
}

/// Append one chunk, refusing once the accumulated length would exceed `max`.
fn push_chunk(buf: &mut Vec<u8>, chunk: &[u8], max: usize) -> Result<()> {
    if buf.len().saturating_add(chunk.len()) > max {
        return Err(too_large(max));
    }
    buf.extend_from_slice(chunk);
    Ok(())
}

#[cfg(test)]
/// The accumulation loop over any chunk source. The async reader performs the
/// same steps inline, since `chunk().await` cannot be driven from an iterator.
fn accumulate_capped<C, I>(chunks: I, max: usize) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = Result<C>>,
    C: AsRef<[u8]>,
{
    let mut buf = Vec::new();
    for chunk in chunks {
        let chunk = chunk?;
        push_chunk(&mut buf, chunk.as_ref(), max)?;
    }
    Ok(buf)
}

/// Read an async response body in full, refusing if it exceeds `max` bytes.
pub async fn body_bytes_capped(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    check_declared_length(resp.content_length(), max)?;
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        push_chunk(&mut buf, &chunk, max)?;
    }
    Ok(buf)
}

/// Async `.text()` with a size cap. Decodes as UTF-8, lossily.
pub async fn body_text_capped(resp: reqwest::Response, max: usize) -> Result<String> {
    let bytes = body_bytes_capped(resp, max).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Async `.json()` with a size cap.
pub async fn body_json_capped<T: DeserializeOwned>(
    resp: reqwest::Response,
    max: usize,
) -> Result<T> {
    let bytes = body_bytes_capped(resp, max).await?;
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("error decoding response body: {e}"))
}

/// Read a blocking response body in full, refusing if it exceeds `max` bytes.
/// Reads at most `max + 1` bytes, as `builtins::http_get` does.
pub fn body_bytes_capped_blocking(
    mut resp: reqwest::blocking::Response,
    max: usize,
) -> Result<Vec<u8>> {
    use std::io::Read;
    check_declared_length(resp.content_length(), max)?;
    let mut buf = Vec::new();
    Read::take(&mut resp, (max as u64).saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| anyhow::anyhow!("error reading response body: {e}"))?;
    if buf.len() > max {
        return Err(too_large(max));
    }
    Ok(buf)
}

/// Blocking `.text()` with a size cap.
pub fn body_text_capped_blocking(resp: reqwest::blocking::Response, max: usize) -> Result<String> {
    let bytes = body_bytes_capped_blocking(resp, max)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Blocking `.json()` with a size cap.
pub fn body_json_capped_blocking<T: DeserializeOwned>(
    resp: reqwest::blocking::Response,
    max: usize,
) -> Result<T> {
    let bytes = body_bytes_capped_blocking(resp, max)?;
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("error decoding response body: {e}"))
}

/// Method form of the blocking readers at `AI_BODY_MAX`, for call chains such
/// as `.send()?.error_for_status()?.json_capped()?`.
pub trait BlockingBodyExt {
    /// Blocking `.text()` capped at `AI_BODY_MAX`.
    fn text_capped(self) -> Result<String>;
    /// Blocking `.json()` capped at `AI_BODY_MAX`.
    fn json_capped<T: DeserializeOwned>(self) -> Result<T>;
}

impl BlockingBodyExt for reqwest::blocking::Response {
    fn text_capped(self) -> Result<String> {
        body_text_capped_blocking(self, AI_BODY_MAX)
    }

    fn json_capped<T: DeserializeOwned>(self) -> Result<T> {
        body_json_capped_blocking(self, AI_BODY_MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::{ErrorCode, SafetyError};

    fn code_of(err: &anyhow::Error) -> Option<ErrorCode> {
        err.downcast_ref::<SafetyError>().map(|s| s.code)
    }

    #[test]
    fn under_limit_is_read_whole() {
        let chunks = vec![Ok::<_, anyhow::Error>(b"ab".to_vec()), Ok(b"cd".to_vec())];
        assert_eq!(accumulate_capped(chunks, 4).unwrap(), b"abcd");
    }

    #[test]
    fn exactly_at_limit_is_accepted() {
        let chunks = vec![Ok::<_, anyhow::Error>(vec![7u8; 4])];
        assert_eq!(accumulate_capped(chunks, 4).unwrap().len(), 4);
    }

    #[test]
    fn over_limit_is_budget_exceeded() {
        let chunks = vec![Ok::<_, anyhow::Error>(vec![0u8; 3]), Ok(vec![0u8; 2])];
        let err = accumulate_capped(chunks, 4).unwrap_err();
        assert_eq!(code_of(&err), Some(ErrorCode::BudgetExceeded));
    }

    #[test]
    fn declared_length_over_limit_is_refused_before_reading() {
        let err = check_declared_length(Some(5), 4).unwrap_err();
        assert_eq!(code_of(&err), Some(ErrorCode::BudgetExceeded));
        assert!(check_declared_length(Some(4), 4).is_ok());
        assert!(check_declared_length(None, 4).is_ok());
    }

    #[test]
    fn read_error_is_not_reported_as_budget() {
        let chunks = vec![Err::<Vec<u8>, _>(anyhow::anyhow!("connection reset"))];
        let err = accumulate_capped(chunks, 4).unwrap_err();
        assert_eq!(code_of(&err), None);
    }
}
