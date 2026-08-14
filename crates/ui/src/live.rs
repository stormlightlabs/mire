use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use mire_core::AnchorSide;
use serde::{Deserialize, Serialize};

/// Versioned presentation state returned by a live-session inspection.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PresentationState {
    /// Current terminal application state.
    pub state: PresentationKind,
    /// Repository-relative bytes for the selected file, when one is selected.
    pub selected_path: Option<Vec<u8>>,
    /// Identifier for the selected finding, when one is selected.
    pub selected_note_id: Option<String>,
    /// First visible logical row in the review stream.
    pub scroll_row: usize,
    /// Requested review layout.
    pub layout: String,
    /// Active finding filters.
    pub filter: String,
    /// Durable review revision for editable review sessions.
    pub review_revision: Option<u64>,
    /// Whether a local client has started a walkthrough.
    pub walkthrough_active: bool,
}

/// The high-level state displayed by a Mire terminal session.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationKind {
    /// The changeset is loading.
    Loading,
    /// The changeset has no files.
    Empty,
    /// A changeset is displayed.
    Ready,
    /// The TUI is showing a recoverable load error.
    Error,
}

/// A transient presentation operation accepted from the local session protocol.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum LiveAction {
    /// Inspect the current presentation state.
    Inspect,
    /// Focus one visible finding by stable identifier.
    FocusNote { note_id: String },
    /// Focus the changed hunk containing a path-side-range location.
    FocusLocation {
        path: Vec<u8>,
        side: AnchorSide,
        start_line: u64,
        end_line: u64,
    },
    /// Move to the next visible finding.
    Next,
    /// Move to the previous visible finding.
    Previous,
    /// Request the CLI's normal source reload path.
    Reload,
    /// Start, stop, or move a coordinated walkthrough.
    Walkthrough { action: WalkthroughAction },
}

/// A coordinated walkthrough action.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkthroughAction {
    /// Mark the session as controlled by a walkthrough client.
    Start,
    /// Move to the next visible finding.
    Next,
    /// Move to the previous visible finding.
    Previous,
    /// End the walkthrough.
    Stop,
}

/// One request delivered to the terminal event loop.
pub struct LiveRequest {
    /// Requested presentation operation.
    pub action: LiveAction,
    /// One-shot response channel for the local transport thread.
    pub response: Sender<LiveResponse>,
}

/// One terminal-loop response for a local presentation operation.
pub enum LiveResponse {
    /// A successful state inspection or navigation response.
    State(PresentationState),
    /// The terminal loop accepted a reload request.
    ReloadRequested,
    /// The request was rejected with a stable machine-readable code.
    Error { code: &'static str },
}

/// Receiver owned by a terminal session for local control requests.
pub struct LiveControl {
    receiver: Receiver<LiveRequest>,
}

impl LiveControl {
    /// Builds a terminal control endpoint from the transport receiver.
    pub fn new(receiver: Receiver<LiveRequest>) -> Self {
        Self { receiver }
    }

    /// Returns the next queued request without waiting.
    pub fn try_recv(&self) -> Result<LiveRequest, TryRecvError> {
        self.receiver.try_recv()
    }
}
