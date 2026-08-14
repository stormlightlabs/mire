use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use mire_core::SchemaVersion;
use mire_tui::{LiveAction, LiveControl, LiveRequest, LiveResponse, PresentationState};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const LIVE_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 0 };
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 4096;
const MAX_RANGE_LINES: u64 = 10_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum LiveSessionError {
    #[error("live-session transport is unavailable: {0}")]
    Io(#[from] io::Error),
    #[error("cannot encode live-session request: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("live session {0:?} was not found")]
    NotFound(String),
    #[error("live-session response was invalid: {0}")]
    InvalidResponse(String),
}

/// Owns the local discovery entry, socket listener, and terminal control receiver.
pub struct LiveSession {
    endpoint: PathBuf,
    descriptor: PathBuf,
    receiver: Option<Receiver<LiveRequest>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Deserialize, Serialize)]
struct Request {
    schema_version: SchemaVersion,
    token: String,
    #[serde(flatten)]
    action: LiveAction,
}

#[derive(Deserialize, Serialize)]
struct Response {
    schema_version: SchemaVersion,
    status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<PresentationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorResponse>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ResponseStatus {
    Ok,
    Error,
}

#[derive(Deserialize, Serialize)]
struct ErrorResponse {
    code: String,
}

#[derive(Deserialize, Serialize)]
struct Descriptor {
    schema_version: SchemaVersion,
    id: String,
    endpoint: PathBuf,
    token: String,
    reload_available: bool,
}

#[derive(Serialize)]
struct ListedSession<'a> {
    schema_version: SchemaVersion,
    id: &'a str,
    reload_available: bool,
}

impl LiveSession {
    /// Starts a local listener and registers a discoverable session.
    pub fn start(reload_available: bool) -> Result<Self, LiveSessionError> {
        let directory = discovery_directory()?;
        let id = random_hex(16)?;
        let endpoint = socket_path()?;
        let descriptor = directory.join(format!("{id}.json"));
        let token = random_hex(32)?;
        let listener = UnixListener::bind(&endpoint)?;
        if let Err(error) = fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600)) {
            let _ = fs::remove_file(&endpoint);
            return Err(LiveSessionError::Io(error));
        }
        if let Err(error) = listener.set_nonblocking(true) {
            let _ = fs::remove_file(&endpoint);
            return Err(LiveSessionError::Io(error));
        }
        let entry = Descriptor {
            schema_version: LIVE_SCHEMA_VERSION,
            id,
            endpoint: endpoint.clone(),
            token: token.clone(),
            reload_available,
        };
        if let Err(error) = write_descriptor(&descriptor, &entry) {
            let _ = fs::remove_file(&endpoint);
            return Err(error);
        }
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || serve(listener, token, sender, worker_stop));
        Ok(Self { endpoint, descriptor, receiver: Some(receiver), stop, worker: Some(worker) })
    }

    /// Transfers the terminal request receiver to Mire's event loop.
    pub fn take_control(&mut self) -> LiveControl {
        LiveControl::new(
            self.receiver
                .take()
                .expect("live control is taken once per terminal session"),
        )
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.descriptor);
        let _ = fs::remove_file(&self.endpoint);
    }
}

/// Lists active sessions without exposing their capabilities.
pub fn list_sessions() -> Result<Vec<serde_json::Value>, LiveSessionError> {
    let directory = discovery_directory()?;
    let mut sessions = Vec::new();
    for descriptor_path in descriptor_paths(&directory)? {
        let descriptor = match read_descriptor(&descriptor_path) {
            Ok(descriptor) => descriptor,
            Err(_) => continue,
        };
        if UnixStream::connect(&descriptor.endpoint).is_err() {
            let _ = fs::remove_file(&descriptor_path);
            let _ = fs::remove_file(&descriptor.endpoint);
            continue;
        }
        sessions.push(serde_json::to_value(ListedSession {
            schema_version: LIVE_SCHEMA_VERSION,
            id: &descriptor.id,
            reload_available: descriptor.reload_available,
        })?);
    }
    Ok(sessions)
}

/// Sends one authenticated action to a discovered local session.
pub fn request_session(id: &str, action: LiveAction) -> Result<serde_json::Value, LiveSessionError> {
    if id.is_empty() || id.len() > MAX_IDENTIFIER_BYTES {
        return Err(LiveSessionError::NotFound(id.to_owned()));
    }
    let descriptor = read_descriptor(&discovery_directory()?.join(format!("{id}.json")))
        .map_err(|_| LiveSessionError::NotFound(id.to_owned()))?;
    if descriptor.id != id || descriptor.schema_version.major != LIVE_SCHEMA_VERSION.major {
        return Err(LiveSessionError::NotFound(id.to_owned()));
    }
    validate_action(&action).map_err(|code| LiveSessionError::InvalidResponse(code.to_owned()))?;
    let request = Request { schema_version: LIVE_SCHEMA_VERSION, token: descriptor.token, action };
    let bytes = serde_json::to_vec(&request)?;
    let mut stream =
        UnixStream::connect(&descriptor.endpoint).map_err(|_| LiveSessionError::NotFound(id.to_owned()))?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    stream.write_all(&bytes)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let response = read_message(&mut stream)?;
    let response: Response =
        serde_json::from_slice(&response).map_err(|error| LiveSessionError::InvalidResponse(error.to_string()))?;
    if response.schema_version.major != LIVE_SCHEMA_VERSION.major {
        return Err(LiveSessionError::InvalidResponse(
            "unsupported response version".to_owned(),
        ));
    }
    serde_json::to_value(response).map_err(LiveSessionError::Encode)
}

fn serve(listener: UnixListener, token: String, sender: mpsc::Sender<LiveRequest>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => serve_connection(&mut stream, &token, &sender),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(20)),
            Err(_) => break,
        }
    }
}

fn serve_connection(stream: &mut UnixStream, token: &str, sender: &mpsc::Sender<LiveRequest>) {
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let response = match read_message(stream)
        .and_then(|bytes| serde_json::from_slice::<Request>(&bytes).map_err(io::Error::other))
    {
        Ok(request) if request.schema_version.major != LIVE_SCHEMA_VERSION.major => {
            error_response("unsupported_version")
        }
        Ok(request) if request.token != token => error_response("authentication_failed"),
        Ok(request) => match validate_action(&request.action) {
            Err(code) => error_response(code),
            Ok(()) => {
                let (response_sender, response_receiver) = mpsc::channel();
                if sender
                    .send(LiveRequest { action: request.action, response: response_sender })
                    .is_err()
                {
                    error_response("session_not_found")
                } else {
                    match response_receiver.recv_timeout(REQUEST_TIMEOUT) {
                        Ok(LiveResponse::State(state)) => ok_response(state),
                        Ok(LiveResponse::ReloadRequested) => ok_response_without_state(),
                        Ok(LiveResponse::Error { code }) => error_response(code),
                        Err(_) => error_response("session_not_found"),
                    }
                }
            }
        },
        Err(error) if error.kind() == io::ErrorKind::InvalidData => error_response("payload_too_large"),
        Err(_) => error_response("invalid_request"),
    };
    let _ = serde_json::to_writer(&mut *stream, &response);
}

fn ok_response(state: PresentationState) -> Response {
    Response { schema_version: LIVE_SCHEMA_VERSION, status: ResponseStatus::Ok, result: Some(state), error: None }
}

fn ok_response_without_state() -> Response {
    Response { schema_version: LIVE_SCHEMA_VERSION, status: ResponseStatus::Ok, result: None, error: None }
}

fn error_response(code: impl Into<String>) -> Response {
    Response {
        schema_version: LIVE_SCHEMA_VERSION,
        status: ResponseStatus::Error,
        result: None,
        error: Some(ErrorResponse { code: code.into() }),
    }
}

fn validate_action(action: &LiveAction) -> Result<(), &'static str> {
    match action {
        LiveAction::Inspect
        | LiveAction::Next
        | LiveAction::Previous
        | LiveAction::Reload
        | LiveAction::Walkthrough { .. } => Ok(()),
        LiveAction::FocusNote { note_id } if note_id.is_empty() || note_id.len() > MAX_IDENTIFIER_BYTES => {
            Err("invalid_request")
        }
        LiveAction::FocusNote { .. } => Ok(()),
        LiveAction::FocusLocation { path, start_line, end_line, .. }
            if path.is_empty()
                || path.len() > MAX_PATH_BYTES
                || *start_line == 0
                || *end_line < *start_line
                || end_line.saturating_sub(*start_line).saturating_add(1) > MAX_RANGE_LINES =>
        {
            Err("invalid_request")
        }
        LiveAction::FocusLocation { .. } => Ok(()),
    }
}

fn socket_path() -> Result<PathBuf, LiveSessionError> {
    let name = format!("mire-{}.sock", random_hex(8)?);
    let temporary = std::env::temp_dir().join(&name);
    if temporary.as_os_str().as_encoded_bytes().len() < 100 {
        return Ok(temporary);
    }
    Ok(PathBuf::from("/tmp").join(name))
}

fn discovery_directory() -> Result<PathBuf, LiveSessionError> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = base.join("mire");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let metadata = fs::metadata(&directory)?;
    if metadata.mode() & 0o077 != 0 {
        return Err(LiveSessionError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "live-session discovery directory is not private",
        )));
    }
    Ok(directory)
}

fn write_descriptor(path: &Path, descriptor: &Descriptor) -> Result<(), LiveSessionError> {
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    serde_json::to_writer(&mut file, descriptor)?;
    file.flush()?;
    Ok(())
}

fn read_descriptor(path: &Path) -> Result<Descriptor, LiveSessionError> {
    let metadata = fs::metadata(path)?;
    if metadata.mode() & 0o077 != 0 {
        return Err(LiveSessionError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "live-session descriptor is not private",
        )));
    }
    let file = File::open(path)?;
    serde_json::from_reader(file).map_err(LiveSessionError::Encode)
}

fn descriptor_paths(directory: &Path) -> Result<Vec<PathBuf>, LiveSessionError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "json") {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn read_message(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.take((MAX_MESSAGE_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "live-session message is too large",
        ));
    }
    Ok(bytes)
}

fn random_hex(bytes: usize) -> io::Result<String> {
    let mut random = vec![0_u8; bytes];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    Ok(random.iter().map(|value| format!("{value:02x}")).collect())
}
