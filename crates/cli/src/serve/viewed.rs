//! Viewed routes: the server's reader marks and clears viewed files by path.

use std::path::Path;

use axum::body::Body;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{Response, StatusCode, Uri};
use axum::response::{IntoResponse, Json};
use mire_core::{BytePath, Fingerprint, Review, ReviewError, ReviewRevision, ViewedMark};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{AppState, PathText, Problem, announce_viewed, local_author, problem};
use crate::review_file::{ReviewFileError, update_review_viewed};

enum ViewedChange {
    Mark {
        fingerprint: Fingerprint,
        revision: ReviewRevision,
    },
    Clear,
}

/// A viewed mark as the browser reads it.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ViewedState {
    /// File fingerprint the reader marked viewed. The file is viewed while
    /// this equals its current fingerprint, its `id`.
    fingerprint: String,
    /// Review revision the reader read the file at.
    revision: u64,
}

/// Body of `PUT /api/v1/viewed`: the file the reader saw.
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkViewedRequest {
    /// File fingerprint the reader saw.
    fingerprint: String,
    /// Review revision the reader read the file at.
    revision: u64,
}

/// The reader's mark for one path after a change.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ViewedResponse {
    /// Current review revision. Marks never advance it.
    revision: u64,
    path: PathText,
    /// The reader's mark, or `null` once cleared.
    viewed: Option<ViewedState>,
}

impl ViewedState {
    /// Describes one stored mark.
    pub fn from_mark(mark: &ViewedMark) -> Self {
        Self { fingerprint: mark.fingerprint().to_string(), revision: mark.revision().get() }
    }
}

/// Marks the path in the `path` query parameter viewed for the local reader.
#[utoipa::path(
    put,
    path = "/api/v1/viewed",
    params(("path" = String, Query, description = "The file's path bytes as lowercase hex, from a path's `raw` field")),
    request_body = MarkViewedRequest,
    responses(
        (status = 200, description = "The reader's mark; the revision does not change", body = ViewedResponse),
        (status = 400, description = "The path is not hex or not a valid path (`invalid_path`), or the body is not JSON with a 64-hex-digit fingerprint and a positive revision (`invalid_viewed_mark`)", body = Problem),
        (status = 409, description = "The file did not have this fingerprint at this revision", body = Problem),
        (status = 422, description = "The reader or the review is at its limit of viewed marks", body = Problem),
        (status = 423, description = "Another process holds the review lock", body = Problem)
    )
)]
pub async fn put_viewed(
    State(state): State<AppState>, uri: Uri, body: Result<axum::Json<MarkViewedRequest>, JsonRejection>,
) -> Response<Body> {
    let Some(path) = query_path(&uri) else {
        return invalid_path();
    };
    let Ok(axum::Json(request)) = body else {
        return invalid_mark();
    };
    let fingerprint = match serde_json::from_value::<Fingerprint>(serde_json::Value::String(request.fingerprint)) {
        Ok(fingerprint) => fingerprint,
        Err(_) => return invalid_mark(),
    };
    let Ok(revision) = ReviewRevision::new(request.revision) else {
        return invalid_mark();
    };
    change_viewed(state, path, ViewedChange::Mark { fingerprint, revision }).await
}

/// Clears the local reader's mark for the path in the `path` query parameter,
/// whether or not the path is still in the changeset.
#[utoipa::path(
    delete,
    path = "/api/v1/viewed",
    params(("path" = String, Query, description = "The file's path bytes as lowercase hex, from a path's `raw` field")),
    responses(
        (status = 200, description = "The path with no mark; the revision does not change", body = ViewedResponse),
        (status = 400, description = "The path is not hex or not a valid path", body = Problem),
        (status = 423, description = "Another process holds the review lock", body = Problem)
    )
)]
pub async fn delete_viewed(State(state): State<AppState>, uri: Uri) -> Response<Body> {
    let Some(path) = query_path(&uri) else {
        return invalid_path();
    };
    change_viewed(state, path, ViewedChange::Clear).await
}

/// Encodes path bytes as the lowercase hex the `path` query parameter takes.
pub fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

/// Reads the one `path` query parameter as hex path bytes.
pub fn query_path(uri: &Uri) -> Option<BytePath> {
    let mut values = uri.query()?.split('&').filter_map(|pair| pair.strip_prefix("path="));
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    BytePath::new(decode_hex(value)?).ok()
}

/// The `invalid_path` problem for a `path` query parameter.
pub fn invalid_path() -> Response<Body> {
    problem(
        StatusCode::BAD_REQUEST,
        "invalid_path",
        "The path query parameter must be a file path's bytes as hex.",
    )
}

async fn change_viewed(state: AppState, path: BytePath, change: ViewedChange) -> Response<Body> {
    let review_path = state.review_path.clone();
    let reader = local_author().id().to_owned();
    let (changed_reader, changed_path) = (reader.clone(), path.clone());
    let result =
        tokio::task::spawn_blocking(move || apply_change(&review_path, &changed_reader, &changed_path, change)).await;
    match result {
        Ok(Ok(review)) => {
            announce_viewed(&state, &review);
            Json(ViewedResponse {
                revision: review.revision().get(),
                path: PathText::from_bytes(path.as_bytes()),
                viewed: review.viewed_mark(&reader, &path).map(ViewedState::from_mark),
            })
            .into_response()
        }
        Ok(Err(ReviewFileError::MarkRejected(ReviewError::ViewedMismatch))) => viewed_mismatch(),
        Ok(Err(ReviewFileError::MarkRejected(error))) => problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            error.error_code(),
            "The review cannot hold this viewed mark.",
        ),
        Ok(Err(ReviewFileError::Locked { .. })) => problem(
            StatusCode::LOCKED,
            "review_locked",
            "The review is being updated by another process. Try again shortly.",
        ),
        Ok(Err(_)) | Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "review_unavailable",
            "The review file could not be read or updated.",
        ),
    }
}

fn apply_change(
    review_path: &Path, reader: &str, path: &BytePath, change: ViewedChange,
) -> Result<Review, ReviewFileError> {
    update_review_viewed(review_path, |review| match change {
        ViewedChange::Mark { fingerprint, revision } => review.mark_viewed(reader, path.clone(), fingerprint, revision),
        ViewedChange::Clear => review.unmark_viewed(reader, path),
    })
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = value.as_bytes().as_chunks::<2>();
    if pairs.is_empty() || !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|[high, low]| Some(hex_value(*high)? << 4 | hex_value(*low)?))
        .collect()
}

/// Reads one lowercase hex digit. Uppercase is rejected so each path has one
/// spelling.
fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

fn invalid_mark() -> Response<Body> {
    problem(
        StatusCode::BAD_REQUEST,
        "invalid_viewed_mark",
        "The body must be JSON with a 64-hex-digit fingerprint and a positive revision.",
    )
}

fn viewed_mismatch() -> Response<Body> {
    problem(
        StatusCode::CONFLICT,
        "viewed_mismatch",
        "The file did not have this fingerprint at this review revision. Reload and try again.",
    )
}
