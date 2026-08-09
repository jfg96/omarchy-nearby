use super::events::ServerEvent;
use super::state::{ServerState, write_body_to_file_with_progress};
use crate::protocol::{DeviceInfo, FileId, PrepareUploadRequest, PrepareUploadResponse, SessionId};
use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub(crate) async fn handle_info(State(state): State<Arc<RwLock<ServerState>>>) -> Response {
    let state = state.read().await;
    Json(state.device.clone()).into_response()
}

pub(crate) async fn handle_register(
    State(state): State<Arc<RwLock<ServerState>>>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Json(mut remote_device): Json<DeviceInfo>,
) -> Response {
    tracing::debug!("Register request from {:?}", remote_device.alias);
    let state = state.read().await;
    remote_device.ip = Some(peer.ip().to_string());
    let _ = state
        .events_tx
        .send(ServerEvent::PeerRegistered(remote_device));
    Json(state.device.clone()).into_response()
}

#[derive(Deserialize)]
pub(crate) struct PrepareUploadParams {
    #[serde(rename = "pin")]
    pin: Option<String>,
}

pub(crate) async fn handle_prepare_upload(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Query(params): Query<PrepareUploadParams>,
    Json(request): Json<PrepareUploadRequest>,
) -> Response {
    // PIN gate runs first, before any session/event work -- a locked-out or
    // unauthenticated peer must never reach the accept flow (which would
    // otherwise answer with 403/409 instead of the correct 401/429).
    {
        let mut state = state_ref.write().await;
        match state.pin_gate.check(params.pin.as_deref(), peer.ip()) {
            crate::server::pin::PinVerdict::Ok => {}
            crate::server::pin::PinVerdict::Unauthorized => {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            crate::server::pin::PinVerdict::LockedOut => {
                return StatusCode::TOO_MANY_REQUESTS.into_response();
            }
        }
    }

    // R7: an empty files map means there is nothing to transfer -- answer
    // 204 immediately, before any session is reserved or accept-event is
    // emitted (a no-op request must not spuriously open a session).
    if request.files.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    // LocalSend represents a text message as exactly one small offered item
    // whose non-empty `preview` is the complete body. Mixed/multi-file offers
    // remain ordinary file transfers even if one item happens to have preview
    // metadata.
    let message_text = if request.files.len() == 1 {
        request.files.values().next().and_then(|file| {
            file.preview
                .as_ref()
                .filter(|text| !text.is_empty() && file.size < 1024 * 1024)
                .cloned()
        })
    } else {
        None
    };

    // Short lock: reject a conflicting session, reserve this one with a
    // placeholder session over the *offered* files (replaced below with the
    // real session, built from the accepted files only, once the accept
    // decision is in), and pull out the config needed to make that decision.
    // Never hold this guard across the `timeout(...).await` below -- that
    // would deadlock every other concurrent request (including the upload
    // that follows acceptance).
    let (events_tx, auto_accept, accept_timeout) = {
        let mut state = state_ref.write().await;

        // Check for existing session timeout (e.g. 5 minutes or session finished)
        if let Some(session) = &state.current_session {
            if session.is_timed_out(300) {
                state.current_session = None;
            } else {
                tracing::warn!("Session already exists, rejecting new session");
                return StatusCode::CONFLICT.into_response();
            }
        }

        state.current_session = Some(crate::core::Session::new(
            request.info.alias.clone(),
            request.files.clone(),
        ));

        (
            state.events_tx.clone(),
            state.auto_accept.load(std::sync::atomic::Ordering::Relaxed),
            state.accept_timeout,
        )
    };

    // Decide: auto-accept, or ask the event consumer.
    let decision = if auto_accept {
        crate::server::events::TransferDecision::Accept
    } else {
        let (pending_request, decision_rx) =
            crate::server::events::PendingRequest::new(request.info.clone(), request.files.clone());
        let request_id = pending_request.request_id().to_string();
        if events_tx
            .send(crate::server::events::ServerEvent::TransferRequest(
                pending_request,
            ))
            .is_err()
        {
            // No consumer listening -> decline.
            crate::server::events::TransferDecision::Decline
        } else {
            match tokio::time::timeout(accept_timeout, decision_rx).await {
                Ok(Ok(d)) => d,
                _ => {
                    let _ = events_tx.send(ServerEvent::TransferRequestExpired { request_id });
                    crate::server::events::TransferDecision::Decline
                }
            }
        }
    };

    let accepted_ids: Vec<FileId> = match decision {
        crate::server::events::TransferDecision::Accept => request.files.keys().cloned().collect(),
        crate::server::events::TransferDecision::AcceptFiles(ids) => ids
            .into_iter()
            .filter(|id| request.files.contains_key(id))
            .collect(),
        crate::server::events::TransferDecision::Decline => Vec::new(),
    };

    if accepted_ids.is_empty() {
        let mut state = state_ref.write().await;
        state.current_session = None;
        tracing::info!("Transfer declined (or timed out)");
        return StatusCode::FORBIDDEN.into_response();
    }

    // Build the real session from the accepted files only -- this replaces
    // the placeholder reservation above and generates fresh, random,
    // per-file tokens (R6: no longer derivable from session/file ids).
    let accepted_files: HashMap<FileId, crate::protocol::FileMetadata> = request
        .files
        .iter()
        .filter(|(id, _)| accepted_ids.contains(id))
        .map(|(id, meta)| (id.clone(), meta.clone()))
        .collect();
    let session = crate::core::Session::new(request.info.alias.clone(), accepted_files);
    let session_id = session.id.clone();
    let files_map = session.tokens.clone();

    {
        let mut state = state_ref.write().await;
        state.current_session = Some(session);
    }

    // If it's a message, return 204 No Content
    if let Some(text) = message_text {
        let mut state = state_ref.write().await;
        let _ = state
            .events_tx
            .send(crate::server::events::ServerEvent::TextReceived {
                session_id: session_id.clone(),
                text,
                sender_alias: request.info.alias.clone(),
            });

        let _ = state
            .events_tx
            .send(crate::server::events::ServerEvent::SessionCompleted {
                session_id: session_id.clone(),
            });

        state.current_session = None;
        return StatusCode::NO_CONTENT.into_response();
    }

    Json(PrepareUploadResponse {
        session_id,
        files: files_map,
    })
    .into_response()
}

#[derive(Deserialize)]
pub(crate) struct UploadParams {
    #[serde(rename = "sessionId")]
    session_id: SessionId,
    #[serde(rename = "fileId")]
    file_id: FileId,
    #[serde(rename = "token")]
    token: crate::protocol::Token,
}

#[derive(Clone)]
struct ReceiveProgressContext {
    session_id: SessionId,
    file_id: FileId,
    file_name: String,
    sender_alias: String,
    total_bytes: u64,
    file_count: usize,
    events_tx: tokio::sync::mpsc::UnboundedSender<crate::server::events::ServerEvent>,
    received_bytes: Arc<AtomicU64>,
    last_emitted: Arc<std::sync::Mutex<Instant>>,
}

impl ReceiveProgressContext {
    fn add(&self, amount: u64) {
        let previous = self
            .received_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_add(amount).min(self.total_bytes))
            })
            .unwrap_or_else(|current| current);
        self.emit(previous.saturating_add(amount).min(self.total_bytes));
    }

    fn rollback(&self, amount: u64) {
        let previous = self
            .received_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(amount))
            })
            .unwrap_or_else(|current| current);
        let _ = self
            .events_tx
            .send(crate::server::events::ServerEvent::FileReceiveProgress {
                session_id: self.session_id.clone(),
                file_id: self.file_id.clone(),
                file_name: self.file_name.clone(),
                sender_alias: self.sender_alias.clone(),
                bytes_received: previous.saturating_sub(amount),
                total_bytes: self.total_bytes,
                file_count: self.file_count,
            });
    }

    fn emit(&self, bytes_received: u64) {
        let mut last = self.last_emitted.lock().expect("progress clock poisoned");
        if bytes_received < self.total_bytes && last.elapsed() < Duration::from_millis(75) {
            return;
        }
        *last = Instant::now();
        let _ = self
            .events_tx
            .send(crate::server::events::ServerEvent::FileReceiveProgress {
                session_id: self.session_id.clone(),
                file_id: self.file_id.clone(),
                file_name: self.file_name.clone(),
                sender_alias: self.sender_alias.clone(),
                bytes_received,
                total_bytes: self.total_bytes,
                file_count: self.file_count,
            });
    }
}

async fn fail_receive_session(
    state_ref: &Arc<RwLock<ServerState>>,
    session_id: &SessionId,
    message: &str,
) {
    let mut state = state_ref.write().await;
    if state
        .current_session
        .as_ref()
        .is_some_and(|s| s.id == *session_id)
    {
        state.current_session = None;
        let _ = state.events_tx.send(ServerEvent::SessionFailed {
            session_id: session_id.clone(),
            message: message.into(),
        });
    }
}

#[axum::debug_handler]
pub(crate) async fn handle_upload(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    Query(params): Query<UploadParams>,
    body: Body,
) -> Response {
    let state = state_ref.write().await;

    // Verify session
    let (
        file_name,
        session_id,
        sender_alias,
        declared_size,
        declared_sha,
        events_tx,
        received_bytes,
        total_bytes,
        file_count,
        receive_rate_limit_bytes_per_second,
        active_uploads,
    ) = if let Some(session) = &state.current_session {
        if session.id != params.session_id {
            tracing::warn!(
                "Upload rejected: Session ID mismatch. Expected {}, got {}",
                session.id,
                params.session_id
            );
            return StatusCode::FORBIDDEN.into_response();
        }

        // Verify token against the session's random per-file token (R6) --
        // never re-derive it, only compare against what was issued.
        if !session.verify_token(&params.file_id, &params.token) {
            tracing::warn!("Upload rejected: Token mismatch");
            return StatusCode::FORBIDDEN.into_response();
        }

        // Find file metadata
        if let Some(meta) = session.files.get(&params.file_id) {
            (
                meta.file_name.clone(),
                session.id.clone(),
                session.sender_alias.clone(),
                meta.size,
                meta.sha256.clone(),
                state.events_tx.clone(),
                session.received_bytes.clone(),
                session
                    .files
                    .values()
                    .map(|file| file.size)
                    .fold(0_u64, u64::saturating_add),
                session.files.len(),
                state.receive_rate_limit_bytes_per_second,
                session.active_uploads.clone(),
            )
        } else {
            tracing::warn!(
                "Upload rejected: File ID {} not found in session",
                params.file_id
            );
            return StatusCode::NOT_FOUND.into_response();
        }
    } else {
        tracing::warn!("Upload rejected: No active session");
        return StatusCode::FORBIDDEN.into_response();
    };

    let save_dir = state.save_dir.clone();
    let final_probe = match crate::path_safety::safe_join(&save_dir, &file_name) {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!("Upload rejected: {}", e);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    // Release the lock before async I/O operations
    drop(state);

    // Ensure parent directory exists (async)
    if let Some(parent) = final_probe.parent()
        && let Err(e) = tokio::fs::create_dir_all(parent).await
    {
        tracing::error!("Failed to create directory {:?}: {}", parent, e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let temp_path = save_dir.join(format!(".nearby-{}-{}.part", session_id, params.file_id));
    let _ = tokio::fs::remove_file(&temp_path).await;
    let progress = ReceiveProgressContext {
        session_id: session_id.clone(),
        file_id: params.file_id.clone(),
        file_name: file_name.clone(),
        sender_alias: sender_alias.clone(),
        total_bytes,
        file_count,
        events_tx,
        received_bytes,
        last_emitted: Arc::new(std::sync::Mutex::new(
            Instant::now() - Duration::from_secs(1),
        )),
    };
    let callback_progress = progress.clone();
    let file_reported = Arc::new(AtomicU64::new(0));
    let callback_file_reported = file_reported.clone();
    let mut previous_file_bytes = 0_u64;
    active_uploads.fetch_add(1, Ordering::Relaxed);
    let body_result = write_body_to_file_with_progress(
        body,
        &temp_path,
        receive_rate_limit_bytes_per_second,
        move |file_bytes| {
            let delta = file_bytes.saturating_sub(previous_file_bytes);
            previous_file_bytes = file_bytes;
            callback_file_reported.store(file_bytes, Ordering::Relaxed);
            callback_progress.add(delta);
        },
    )
    .await;
    active_uploads.fetch_sub(1, Ordering::Relaxed);
    let body_len = match body_result {
        Ok(bytes_written) => bytes_written,
        Err(e) => {
            progress.rollback(file_reported.load(Ordering::Relaxed));
            let _ = tokio::fs::remove_file(&temp_path).await;
            tracing::error!("Failed to save file to {:?}: {}", temp_path, e);
            fail_receive_session(&state_ref, &session_id, "Connection lost during transfer").await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Validate the received bytes against the metadata declared in
    // prepare-upload. A truncated body (network cut, or a misbehaving client
    // that illegally splits the upload into multiple POSTs) would otherwise
    // be saved as a partial file and the session wrongly marked complete.
    // On any mismatch: discard the partial, return 500 ("Unknown error by
    // receiver", per the LocalSend v2.1 spec's upload error table), and leave
    // the session untouched so it is neither recorded nor completed -- the
    // sender can retry the same file id against the still-open session.
    if body_len != declared_size {
        progress.rollback(body_len);
        tracing::warn!(
            "Upload size mismatch for {:?}: declared {} bytes, received {} bytes; discarding partial",
            temp_path,
            declared_size,
            body_len
        );
        let _ = tokio::fs::remove_file(&temp_path).await;
        fail_receive_session(
            &state_ref,
            &session_id,
            "Received size did not match the request",
        )
        .await;
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // When the sender advertised a sha256, verify the bytes on disk match it
    // (case-insensitive hex). Size can be right while the contents are
    // corrupt; reject those the same way.
    if let Some(expected_sha) = declared_sha {
        match crate::sha256_from_file(&temp_path).await {
            Ok(actual) if actual.eq_ignore_ascii_case(&expected_sha) => {}
            Ok(actual) => {
                progress.rollback(body_len);
                tracing::warn!(
                    "Upload sha256 mismatch for {:?}: declared {}, computed {}; discarding",
                    temp_path,
                    expected_sha,
                    actual
                );
                let _ = tokio::fs::remove_file(&temp_path).await;
                fail_receive_session(
                    &state_ref,
                    &session_id,
                    "Received file failed integrity validation",
                )
                .await;
                return StatusCode::UNPROCESSABLE_ENTITY.into_response();
            }
            Err(e) => {
                progress.rollback(body_len);
                tracing::error!("Failed to hash uploaded file {:?}: {}", temp_path, e);
                let _ = tokio::fs::remove_file(&temp_path).await;
                fail_receive_session(&state_ref, &session_id, "Could not validate received file")
                    .await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    // Reacquire lock for state updates
    let mut state = state_ref.write().await;

    // The write above was lock-free and may have taken a long time (large /
    // multi-GB files). The session validated before the write started may
    // have since been cancelled or timed out and replaced by a brand-new
    // session. Re-validate identity before touching session state: a stale
    // upload must never be recorded against a different session's
    // accounting, since a foreign file id could otherwise push an unrelated
    // session to "all done".
    let still_current = state
        .current_session
        .as_ref()
        .map(|session| session.id == session_id)
        .unwrap_or(false);

    // Record this file as received on the (still-current) session; a
    // multi-file transfer only closes once every accepted file has arrived,
    // not after the first one (R5).
    if !still_current {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return StatusCode::CONFLICT.into_response();
    }
    let save_path = match crate::core::commit_temp_file(&save_dir, &file_name, &temp_path).await {
        Ok(path) => path,
        Err(e) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            let _ = state.events_tx.send(ServerEvent::SessionFailed {
                session_id: session_id.clone(),
                message: e.to_string(),
            });
            state.current_session = None;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    tracing::info!("Received file: {:?} for session {}", save_path, session_id);
    let all_done = if still_current {
        state
            .current_session
            .as_mut()
            .map(|session| session.mark_received(&params.file_id))
            .unwrap_or(false)
    } else {
        tracing::warn!(
            "Upload for session {} completed after the session was replaced; \
             file saved to disk but not recorded against the new session",
            session_id
        );
        false
    };

    // The event queue is unbounded, so terminal/control events are reliable and
    // cannot block the upload path. Progress is throttled before enqueueing.
    // -- a slow or absent event consumer must not stall the transfer.
    // The bytes genuinely landed on disk, so FileReceived is still accurate
    // to emit even if the owning session has since changed. Report the
    // *final* on-disk name -- unique_save_path may have renamed the file on
    // collision, and a consumer needs to see where the bytes actually went,
    // not the name originally requested by the sender.
    let final_file_name = save_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(file_name);
    let _ = state
        .events_tx
        .send(crate::server::events::ServerEvent::FileReceived {
            session_id: session_id.clone(),
            file_id: params.file_id.clone(),
            file_name: final_file_name,
            path: save_path,
            size: body_len,
            sender_alias,
            // A real binary upload has no inline text body.
            message_text: None,
        });

    if still_current && all_done {
        let _ = state
            .events_tx
            .send(crate::server::events::ServerEvent::SessionCompleted { session_id });
        state.current_session = None;
    }

    StatusCode::OK.into_response()
}

#[derive(Deserialize)]
pub(crate) struct CancelParams {
    #[serde(rename = "sessionId")]
    session_id: SessionId,
}

pub(crate) async fn handle_cancel(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    Query(params): Query<CancelParams>,
) -> Response {
    let mut state = state_ref.write().await;

    if let Some(session) = &state.current_session
        && session.id.as_str() == params.session_id.as_str()
    {
        let _ = state
            .events_tx
            .send(crate::server::events::ServerEvent::SessionCancelled {
                session_id: params.session_id.clone(),
            });
        state.current_session = None;
        tracing::info!("Session {} cancelled", params.session_id);
    }

    StatusCode::OK.into_response()
}
