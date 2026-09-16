const TRANSIENT_UPLOAD_RETRY_ATTEMPTS: u32 = 2;

// Fixed transfer policy (direct Telegram transfers only — no user-configurable
// throttle). These replace the old `TransferPolicy` struct + DI plumbing.
pub(crate) const RETRY_ATTEMPTS: u32 = 0;
pub(crate) const RETRY_BASE_BACKOFF_MS: u64 = 1_000;
pub(crate) const RETRY_MAX_BACKOFF_MS: u64 = 30_000;
pub(crate) const ARCHIVE_MAX_BYTES: u64 = 256 * 1024 * 1024;

// Downloads previously reused `retry_attempts() == 0` as the chunk retry budget,
// so a single transient chunk error killed the whole download and deleted the
// partial file. Give chunk errors a real budget independent of user retries.
pub(crate) const DOWNLOAD_CHUNK_RETRY_ATTEMPTS: u32 = 3;
pub(crate) const DOWNLOAD_STALL_TIMEOUT_SECS: u64 = 30;

pub(crate) fn upload_stream_retry_attempts(configured_attempts: u32) -> u32 {
    configured_attempts.max(TRANSIENT_UPLOAD_RETRY_ATTEMPTS)
}

pub(crate) fn backoff_ms(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    let capped = base_ms.saturating_mul(1u64 << attempt.min(10)).min(max_ms);
    capped + (capped as f64 * 0.25 * rand::random::<f64>()) as u64
}

pub(crate) fn flood_wait_retry_attempts(configured_attempts: u32) -> u32 {
    configured_attempts.max(10)
}

pub(crate) fn should_retry_upload_error(
    err: &str,
    attempt: u32,
    configured_attempts: u32,
) -> bool {
    if attempt < configured_attempts {
        return true;
    }

    (err.starts_with("FLOOD_WAIT_") || is_transient_upload_error(err))
        && attempt < upload_stream_retry_attempts(configured_attempts)
}

pub(crate) fn upload_error_kind(err: &str) -> &'static str {
    if err.starts_with("FLOOD_WAIT_") {
        "telegram flood wait"
    } else if is_transient_upload_error(err) {
        "transient network/Telegram read error"
    } else {
        "non-retryable upload error"
    }
}

fn is_transient_upload_error(err: &str) -> bool {
    let err = err.to_ascii_lowercase();
    [
        "read 0 bytes",
        "reached eof before",
        "eof before reaching",
        "unexpected eof",
        "early eof",
        "connection reset",
        "connection aborted",
        "connection closed",
        "forcibly closed",
        "os error 10054",
        "connection lost",
        "broken pipe",
        "timed out",
        "timeout",
        "temporarily unavailable",
    ]
    .iter()
    .any(|needle| err.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_read_errors_get_two_internal_retries_when_user_retry_is_disabled() {
        let err = "request error: read 0 bytes";

        assert!(should_retry_upload_error(err, 0, 0));
        assert!(should_retry_upload_error(err, 1, 0));
        assert!(!should_retry_upload_error(err, 2, 0));
    }

    #[test]
    fn permanent_upload_errors_do_not_get_internal_retries() {
        let err = "rpc error 400: FILE_PARTS_INVALID caused by upload.saveBigFilePart";

        assert!(!should_retry_upload_error(err, 0, 0));
    }

    #[test]
    fn windows_connection_reset_gets_internal_retry() {
        let err = "request error: An existing connection was forcibly closed by the remote host. (os error 10054)";

        assert_eq!(upload_error_kind(err), "transient network/Telegram read error");
        assert!(should_retry_upload_error(err, 1, 1));
        assert!(!should_retry_upload_error(err, 2, 1));
    }

    #[test]
    fn user_retry_budget_still_applies_before_transient_only_retry() {
        let err = "rpc error 400: FILE_PARTS_INVALID caused by upload.saveBigFilePart";

        assert!(should_retry_upload_error(err, 0, 1));
        assert!(!should_retry_upload_error(err, 1, 1));
    }

    #[test]
    fn flood_wait_gets_ten_retries_when_optional_retries_are_disabled() {
        assert_eq!(flood_wait_retry_attempts(0), 10);
        assert_eq!(flood_wait_retry_attempts(3), 10);
    }

    #[test]
    fn download_chunks_have_a_real_retry_budget() {
        // Guard against regressing to the old budget = 0 behaviour where a
        // single transient chunk error failed the entire download.
        assert!(DOWNLOAD_CHUNK_RETRY_ATTEMPTS > 0);
        assert!(DOWNLOAD_STALL_TIMEOUT_SECS > 0);
    }

    #[test]
    fn backoff_is_bounded_and_grows_with_attempt() {
        let first = backoff_ms(0, RETRY_BASE_BACKOFF_MS, RETRY_MAX_BACKOFF_MS);
        let second = backoff_ms(1, RETRY_BASE_BACKOFF_MS, RETRY_MAX_BACKOFF_MS);
        let capped = backoff_ms(20, RETRY_BASE_BACKOFF_MS, RETRY_MAX_BACKOFF_MS);

        assert!(first >= RETRY_BASE_BACKOFF_MS);
        assert!(second >= first);
        assert!(capped <= RETRY_MAX_BACKOFF_MS + (RETRY_MAX_BACKOFF_MS / 4));
    }
}
