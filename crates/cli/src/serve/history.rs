//! History routes: the revisions that changed a file, a file's hunks at one
//! revision, and the code a finding was written on.

use std::collections::BTreeSet;

use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{Response, StatusCode, Uri};
use axum::response::{IntoResponse, Json};
use mire_core::{BytePath, Checkpoint, CheckpointRef, CheckpointRow, ObjectId, Review, ReviewRevision};
use serde::Serialize;
use utoipa::ToSchema;

use super::{AppState, PathText, Problem, SemanticFileContent, load_current_review, problem, viewed};
use crate::history::{self, CheckpointContent, CheckpointFailure};

/// The body of `GET /api/v1/history`: the file's history without `revision`,
/// or its hunks at that revision with it.
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub enum HistoryResult {
    /// The revisions that changed the file.
    History(HistoryResponse),
    /// The file's hunks at the requested revision.
    Checkpoint(CheckpointResponse),
}

/// One revision that changed a file.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Review revision of the checkpoint.
    revision: u64,
    /// The file's fingerprint there, or `null` when the file left the
    /// changeset at that revision.
    fingerprint: Option<String>,
    /// Whether Mire can show the file's hunks at this revision.
    available: bool,
    /// Whether Mire kept a patch copy of the hunks.
    copy: bool,
    /// Whether this is the baseline of a review file written before
    /// checkpoints: history before it is unknown.
    baseline: bool,
    /// Whether the file changed somewhere between the previous checkpoint and
    /// this one, at revisions Mire did not record.
    gap: bool,
}

/// The checkpoint row a finding's anchor was written on.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointLink {
    /// Revision of the checkpoint that holds the row.
    revision: u64,
    path: PathText,
}

/// The revisions that changed one file, newest first.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    /// Current review revision.
    revision: u64,
    path: PathText,
    history: Vec<HistoryEntry>,
}

/// One file's hunks at a checkpoint.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointResponse {
    /// Current review revision.
    revision: u64,
    path: PathText,
    /// Revision of the checkpoint row the hunks come from.
    checkpoint_revision: u64,
    /// The file's fingerprint at that row.
    fingerprint: String,
    /// Where the hunks came from: `capture`, `git` or `copy`.
    source: String,
    content: SemanticFileContent,
}

impl HistoryEntry {
    fn new(review: &Review, checkpoint: &Checkpoint, row: &CheckpointRow, held: &BTreeSet<ObjectId>) -> Self {
        Self {
            revision: checkpoint.revision().get(),
            fingerprint: row.fingerprint().map(|fingerprint| fingerprint.to_string()),
            available: history::is_available(review, row, held),
            copy: row.copy().hunks().is_some(),
            baseline: checkpoint.is_baseline(),
            gap: checkpoint.is_gap(),
        }
    }
}

impl CheckpointLink {
    /// Describes a finding anchor's checkpoint reference.
    pub fn from_reference(reference: &CheckpointRef) -> Self {
        Self { revision: reference.revision().get(), path: PathText::from_bytes(reference.path().as_bytes()) }
    }
}

/// Returns a file's history entries, newest first, with carried rows left
/// out. `held` names the blobs the bound repository holds.
pub fn file_history(review: &Review, path: &BytePath, held: &BTreeSet<ObjectId>) -> Vec<HistoryEntry> {
    review
        .file_history(path)
        .into_iter()
        .map(|(checkpoint, row)| HistoryEntry::new(review, checkpoint, row, held))
        .collect()
}

/// Lists the revisions that changed the file in the `path` query parameter,
/// or, with `revision`, returns the file's hunks at that revision.
#[utoipa::path(
    get,
    path = "/api/v1/history",
    params(
        ("path" = String, Query, description = "The file's path bytes as lowercase hex, from a path's `raw` field"),
        ("revision" = Option<u64>, Query, description = "Review revision to read the file at")
    ),
    responses(
        (status = 200, description = "Without `revision`, a `HistoryResponse`; with it, a `CheckpointResponse`", body = HistoryResult),
        (status = 400, description = "The path is not hex or not a valid path (`invalid_path`), or the revision is not a positive number (`invalid_revision`)", body = Problem),
        (status = 404, description = "The file has no checkpoint at this revision (`checkpoint_not_found`), or Mire cannot rebuild its hunks (`checkpoint_unavailable`)", body = Problem)
    )
)]
pub async fn get_history(State(state): State<AppState>, uri: Uri) -> Response<Body> {
    let Some(path) = viewed::query_path(&uri) else {
        return viewed::invalid_path();
    };
    let revision = match query_revision(&uri) {
        Ok(revision) => revision,
        Err(()) => {
            return problem(
                StatusCode::BAD_REQUEST,
                "invalid_revision",
                "The revision query parameter must be a positive number.",
            );
        }
    };
    let review = match load_current_review(&state).await {
        Ok(review) => review,
        Err(response) => return *response,
    };
    let result = tokio::task::spawn_blocking(move || match revision {
        None => {
            let held = history::held_blobs(&review);
            Ok(Json(HistoryResult::History(HistoryResponse {
                revision: review.revision().get(),
                path: PathText::from_bytes(path.as_bytes()),
                history: file_history(&review, &path, &held),
            }))
            .into_response())
        }
        Some(revision) => history::file_at_revision(&review, &path, revision).map(|(checkpoint, row, content)| {
            let Json(response) = checkpoint_response(&review, &path, checkpoint, row, content);
            Json(HistoryResult::Checkpoint(response)).into_response()
        }),
    })
    .await;
    finish(result)
}

/// Returns the code a finding was written on: its file's hunks at the
/// checkpoint row its anchor names.
#[utoipa::path(
    get,
    path = "/api/v1/findings/{note_id}/checkpoint",
    params(("note_id" = String, Path, description = "Stable finding identifier")),
    responses(
        (status = 200, description = "The file's hunks at the finding's checkpoint", body = CheckpointResponse),
        (status = 404, description = "No finding has this identity (`finding_not_found`), the finding has no checkpoint (`checkpoint_not_found`), or Mire cannot rebuild its hunks (`checkpoint_unavailable`)", body = Problem)
    )
)]
pub async fn get_finding_checkpoint(
    State(state): State<AppState>, AxumPath(note_id): AxumPath<String>,
) -> Response<Body> {
    let review = match load_current_review(&state).await {
        Ok(review) => review,
        Err(response) => return *response,
    };
    let Some(note) = review.notes().iter().find(|note| note.id().as_str() == note_id) else {
        return problem(
            StatusCode::NOT_FOUND,
            "finding_not_found",
            "This finding is not part of the review.",
        );
    };
    let Some(reference) = note.anchor().checkpoint().cloned() else {
        return failure(CheckpointFailure::NotFound);
    };
    let result = tokio::task::spawn_blocking(move || {
        let row = review.checkpoint_row(&reference).ok_or(CheckpointFailure::NotFound)?;
        let content = history::rebuild(&review, row)?;
        Ok(checkpoint_response(&review, reference.path(), reference.revision(), row, content).into_response())
    })
    .await;
    finish(result)
}

fn checkpoint_response(
    review: &Review, path: &BytePath, checkpoint: ReviewRevision, row: &CheckpointRow, content: CheckpointContent,
) -> Json<CheckpointResponse> {
    Json(CheckpointResponse {
        revision: review.revision().get(),
        path: PathText::from_bytes(path.as_bytes()),
        checkpoint_revision: checkpoint.get(),
        fingerprint: row
            .fingerprint()
            .map(|fingerprint| fingerprint.to_string())
            .unwrap_or_default(),
        source: content.source.as_str().to_owned(),
        content: SemanticFileContent::from_content(&content.content),
    })
}

fn finish(result: Result<Result<Response<Body>, CheckpointFailure>, tokio::task::JoinError>) -> Response<Body> {
    match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => failure(error),
        Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "review_unavailable",
            "The review file could not be read.",
        ),
    }
}

fn failure(error: CheckpointFailure) -> Response<Body> {
    problem(StatusCode::NOT_FOUND, error.code(), error.message())
}

/// Reads the optional `revision` query parameter. A repeated, zero or
/// non-numeric value is an error.
fn query_revision(uri: &Uri) -> Result<Option<ReviewRevision>, ()> {
    let mut values = uri
        .query()
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter_map(|pair| pair.strip_prefix("revision="));
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let number = value.parse::<u64>().map_err(|_| ())?;
    ReviewRevision::new(number).map(Some).map_err(|_| ())
}
