use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use notify::{Config, Event, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};

const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(150);
const RECOVERY_INTERVAL: Duration = Duration::from_secs(2);

enum WatchBackend {
    Native(RecommendedWatcher),
    Polling(PollWatcher),
}

/// A failure to start both native and polling filesystem observation.
#[derive(Debug, thiserror::Error)]
#[error("cannot watch {path:?}: native observation failed: {native}; polling fallback failed: {polling}")]
pub struct WatchError {
    path: std::path::PathBuf,
    native: Box<notify::Error>,
    polling: Box<notify::Error>,
}

/// Debounced filesystem observation with a periodic recovery reload.
pub struct WatchSet {
    backend: WatchBackend,
    events: Receiver<notify::Result<Event>>,
    path: std::path::PathBuf,
    schedule: ReloadSchedule,
    error: Option<notify::Error>,
}

impl WatchSet {
    /// Watches one file parent or repository tree, falling back to `PollWatcher`.
    pub fn new(path: &Path, recursive: bool) -> Result<Self, WatchError> {
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        let (sender, events) = mpsc::channel();
        let native_result = RecommendedWatcher::new(sender.clone(), Config::default()).and_then(|mut watcher| {
            watcher.watch(path, mode)?;
            Ok(watcher)
        });
        let backend = match native_result {
            Ok(watcher) => WatchBackend::Native(watcher),
            Err(native) => {
                let polling = PollWatcher::new(sender, Config::default().with_poll_interval(RECOVERY_INTERVAL))
                    .and_then(|mut watcher| {
                        watcher.watch(path, mode)?;
                        Ok(watcher)
                    });
                match polling {
                    Ok(watcher) => WatchBackend::Polling(watcher),
                    Err(polling) => {
                        return Err(WatchError {
                            path: path.to_owned(),
                            native: Box::new(native),
                            polling: Box::new(polling),
                        });
                    }
                }
            }
        };
        Ok(Self { backend, events, path: path.to_owned(), schedule: ReloadSchedule::new(Instant::now()), error: None })
    }

    /// Reports when a debounced or recovery reload should run.
    pub fn reload_due(&mut self) -> bool {
        let now = Instant::now();
        while let Ok(event) = self.events.try_recv() {
            match event {
                Ok(_) => self.schedule.observe(now),
                Err(error) => self.error = Some(error),
            }
        }
        self.schedule.reload_due(now)
    }

    /// Returns and clears the latest backend error observed while watching.
    pub fn take_error(&mut self) -> Option<notify::Error> {
        self.error.take()
    }
}

impl Drop for WatchSet {
    fn drop(&mut self) {
        match &mut self.backend {
            WatchBackend::Native(watcher) => _ = watcher.unwatch(&self.path),
            WatchBackend::Polling(watcher) => _ = watcher.unwatch(&self.path),
        }
    }
}

#[derive(Debug)]
struct ReloadSchedule {
    debounce_deadline: Option<Instant>,
    recovery_deadline: Instant,
}

impl ReloadSchedule {
    fn new(now: Instant) -> Self {
        Self { debounce_deadline: None, recovery_deadline: now + RECOVERY_INTERVAL }
    }

    fn observe(&mut self, now: Instant) {
        self.debounce_deadline = Some(now + DEBOUNCE_INTERVAL);
    }

    fn reload_due(&mut self, now: Instant) -> bool {
        let event_due = self.debounce_deadline.is_some_and(|deadline| now >= deadline);
        let recovery_due = now >= self.recovery_deadline;
        if !event_due && !recovery_due {
            return false;
        }
        self.debounce_deadline = None;
        self.recovery_deadline = now + RECOVERY_INTERVAL;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_events_wait_for_a_quiet_debounce_window() {
        let start = Instant::now();
        let mut schedule = ReloadSchedule::new(start);
        schedule.observe(start);
        schedule.observe(start + Duration::from_millis(100));

        assert!(!schedule.reload_due(start + Duration::from_millis(200)));
        assert!(schedule.reload_due(start + Duration::from_millis(250)));
        assert!(!schedule.reload_due(start + Duration::from_millis(251)));
    }

    #[test]
    fn periodic_recovery_reload_does_not_require_an_event() {
        let start = Instant::now();
        let mut schedule = ReloadSchedule::new(start);

        assert!(!schedule.reload_due(start + Duration::from_millis(999)));
        assert!(schedule.reload_due(start + RECOVERY_INTERVAL));
    }
}
