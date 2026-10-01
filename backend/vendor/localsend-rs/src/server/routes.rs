use super::handlers::{
    handle_cancel, handle_info, handle_prepare_upload, handle_register, handle_upload,
};
use super::limits::{BodyTimeout, ServerLimits, limit_request_body};
use super::state::ServerState;
use super::web_share::{
    handle_download, handle_prepare_download, handle_web_i18n, handle_web_index, handle_web_js,
};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request},
    middleware,
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Register bodies carry one device description; they never need the 2 MB
/// default that prepare-upload's file lists may use.
const REGISTER_BODY_LIMIT: usize = 64 * 1024;

pub(crate) fn create_router(state: Arc<RwLock<ServerState>>, limits: ServerLimits) -> Router {
    let whole_body = BodyTimeout::Total(limits.request_body_timeout);
    let upload_idle = BodyTimeout::Idle(limits.upload_idle_timeout);

    // Every route except upload must receive its whole body within the body
    // timeout. Handler work after the body, such as waiting for an accept
    // decision, is not limited here.
    let requests = Router::new()
        .route("/api/localsend/v2/info", get(handle_info))
        .route(
            "/api/localsend/v2/register",
            post(handle_register).layer(DefaultBodyLimit::max(REGISTER_BODY_LIMIT)),
        )
        .route(
            "/api/localsend/v2/prepare-upload",
            post(handle_prepare_upload),
        )
        .route("/api/localsend/v2/cancel", post(handle_cancel))
        .route(
            "/api/localsend/v2/prepare-download",
            post(handle_prepare_download),
        )
        .route("/api/localsend/v2/download", get(handle_download))
        .route("/", get(handle_web_index))
        .route("/main.js", get(handle_web_js))
        .route("/i18n.json", get(handle_web_i18n))
        .route_layer(middleware::map_request(move |request: Request| {
            limit_request_body(whole_body, request)
        }));

    // Uploads may legitimately last long; only a stalled sender is cut off.
    let uploads = Router::new()
        .route("/api/localsend/v2/upload", post(handle_upload))
        .route_layer(middleware::map_request(move |request: Request| {
            limit_request_body(upload_idle, request)
        }));

    requests.merge(uploads).with_state(state)
}
