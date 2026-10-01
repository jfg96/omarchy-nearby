use crate::protocol::DeviceInfo;
use axum::body::Body;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::io::AsyncWriteExt;

pub struct ServerState {
    pub device: DeviceInfo,
    pub current_session: Option<crate::core::Session>,
    pub save_dir: PathBuf,
    pub events_tx: tokio::sync::mpsc::UnboundedSender<crate::server::events::ServerEvent>,
    /// Shared with [`crate::server::LocalSendServer`] so a live
    /// `set_auto_accept` toggle is observed by the request handler.
    pub auto_accept: Arc<AtomicBool>,
    pub accept_timeout: std::time::Duration,
    pub receive_rate_limit_bytes_per_second: Option<u64>,
    pub pin_gate: crate::server::pin::PinGate,
    pub web_share: Option<crate::server::web_share::WebShareState>,
}

/// `keep_writing` is checked before each chunk; once it returns false the
/// upload stops without writing that chunk or consuming the rest of the body.
pub(crate) async fn write_body_to_file_with_progress<F, K>(
    body: Body,
    path: &Path,
    expected_size: u64,
    rate_limit_bytes_per_second: Option<u64>,
    keep_writing: K,
    mut progress: F,
) -> std::io::Result<u64>
where
    F: FnMut(u64),
    K: Fn() -> bool,
{
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    let mut bytes_written = 0u64;
    let mut stream = body.into_data_stream();
    let started_at = tokio::time::Instant::now();
    let rate_limit_bytes_per_second = rate_limit_bytes_per_second.filter(|rate| *rate > 0);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| std::io::Error::other(e.to_string()))?;
        if !keep_writing() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Upload session ended",
            ));
        }
        let next_size = checked_upload_size(bytes_written, chunk.len(), expected_size)?;
        file.write_all(&chunk).await?;
        bytes_written = next_size;
        if let Some(rate) = rate_limit_bytes_per_second {
            let target = std::time::Duration::from_secs_f64(bytes_written as f64 / rate as f64);
            let delay = target.saturating_sub(started_at.elapsed());
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
        }
        progress(bytes_written);
    }

    file.flush().await?;
    Ok(bytes_written)
}

/// Reject an entire chunk before writing if it would exceed the accepted size.
fn checked_upload_size(written: u64, chunk_len: usize, expected_size: u64) -> std::io::Result<u64> {
    u64::try_from(chunk_len)
        .ok()
        .and_then(|chunk_len| written.checked_add(chunk_len))
        .filter(|next_size| *next_size <= expected_size)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Upload exceeds the accepted file size",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{checked_upload_size, write_body_to_file_with_progress};
    use axum::body::{Body, Bytes};
    use futures_util::stream;
    use std::convert::Infallible;

    #[tokio::test]
    async fn write_body_to_file_writes_stream_and_returns_size() {
        let path = std::env::temp_dir().join(format!(
            "localsend-stream-upload-{}.bin",
            uuid::Uuid::new_v4()
        ));
        let body = Body::from("streamed upload content");

        let bytes_written =
            write_body_to_file_with_progress(body, &path, 23, None, || true, |_| {})
                .await
                .expect("body should stream to file");

        assert_eq!(bytes_written, 23);
        assert_eq!(
            tokio::fs::read(&path).await.expect("file should exist"),
            b"streamed upload content"
        );

        let _ = tokio::fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn write_body_to_file_reports_cumulative_bytes_for_each_chunk() {
        let path = std::env::temp_dir().join(format!(
            "localsend-progress-upload-{}.bin",
            uuid::Uuid::new_v4()
        ));
        let chunks = stream::iter([
            Ok::<_, Infallible>(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"de")),
            Ok(Bytes::from_static(b"fghi")),
        ]);
        let body = Body::from_stream(chunks);
        let mut samples = Vec::new();

        let bytes_written = write_body_to_file_with_progress(
            body,
            &path,
            9,
            None,
            || true,
            |cumulative| {
                samples.push(cumulative);
            },
        )
        .await
        .expect("body should stream with progress");

        assert_eq!(samples, vec![3, 5, 9]);
        assert_eq!(bytes_written, 9);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abcdefghi");

        let _ = tokio::fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn write_body_to_file_can_throttle_real_stream_consumption() {
        let path = std::env::temp_dir().join(format!(
            "localsend-throttled-upload-{}.bin",
            uuid::Uuid::new_v4()
        ));
        let body = Body::from(vec![0_u8; 4_096]);
        let started_at = tokio::time::Instant::now();

        let bytes_written =
            write_body_to_file_with_progress(body, &path, 4_096, Some(8_192), || true, |_| {})
                .await
                .expect("throttled body should stream to file");

        assert_eq!(bytes_written, 4_096);
        assert!(started_at.elapsed() >= std::time::Duration::from_millis(450));
        assert_eq!(tokio::fs::metadata(&path).await.unwrap().len(), 4_096);

        let _ = tokio::fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn oversized_stream_stops_before_eof_without_writing_the_excess_chunk() {
        use futures_util::StreamExt;
        use tokio::time::{Duration, timeout};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upload.part");
        // A sender need not close the body after sending excess bytes. Neither
        // disk writes nor the rejection may wait for that EOF.
        let chunks = stream::iter([
            Ok::<_, Infallible>(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"de")),
        ])
        .chain(stream::pending());
        let mut samples = Vec::new();
        let result = timeout(
            Duration::from_secs(1),
            write_body_to_file_with_progress(
                Body::from_stream(chunks),
                &path,
                4,
                None,
                || true,
                |bytes| {
                    samples.push(bytes);
                },
            ),
        )
        .await
        .expect("must reject excess bytes without waiting for EOF");
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abc");
        assert_eq!(samples, vec![3], "rejected bytes must not advance progress");
    }

    #[tokio::test]
    async fn oversized_first_chunk_never_writes_or_reports_progress() {
        for expected_size in [0, 2] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("upload.part");
            let mut samples = Vec::new();
            let error = write_body_to_file_with_progress(
                Body::from("abc"),
                &path,
                expected_size,
                None,
                || true,
                |bytes| samples.push(bytes),
            )
            .await
            .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            assert_eq!(tokio::fs::metadata(&path).await.unwrap().len(), 0);
            assert!(samples.is_empty());
        }
    }

    #[tokio::test]
    async fn empty_upload_matches_zero_accepted_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upload.part");
        let written = write_body_to_file_with_progress(
            Body::empty(),
            &path,
            0,
            None,
            || true,
            |_| {
                panic!("an empty body must not report written bytes");
            },
        )
        .await
        .unwrap();
        assert_eq!(written, 0);
        assert_eq!(tokio::fs::metadata(&path).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn ended_session_stops_before_writing_the_next_chunk() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upload.part");
        let alive = AtomicBool::new(true);
        let chunks = stream::iter([
            Ok::<_, Infallible>(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"de")),
        ]);
        let error = write_body_to_file_with_progress(
            Body::from_stream(chunks),
            &path,
            5,
            None,
            || alive.load(Ordering::Relaxed),
            |_| alive.store(false, Ordering::Relaxed),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abc");
    }

    #[test]
    fn upload_size_check_rejects_overflow_and_accepts_the_exact_boundary() {
        assert_eq!(
            checked_upload_size(u64::MAX - 1, 1, u64::MAX).unwrap(),
            u64::MAX
        );
        assert_eq!(
            checked_upload_size(u64::MAX, 1, u64::MAX)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData,
        );
        assert!(checked_upload_size(3, 2, 4).is_err());
    }
}
