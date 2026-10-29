//! How many colors the terminal shows, so a palette can pick colors that
//! survive the terminal's rounding.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Longest wait for tmux to report its client's features. A stopped or busy
/// tmux server would otherwise hang startup.
const TMUX_TIMEOUT: Duration = Duration::from_millis(200);

/// Pause between checks on whether the tmux query has exited.
const TMUX_POLL: Duration = Duration::from_millis(5);

/// How many colors the terminal shows for an RGB color.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorDepth {
    /// The terminal shows every RGB color as sent.
    Truecolor,
    /// The terminal rounds each RGB color to its nearest xterm 256-colour index.
    Xterm256,
}

/// What the environment says about the color depth, strongest first.
#[derive(Clone, Copy, Debug, Default)]
pub struct DepthSignals<'a> {
    /// `MIRE_COLOR_DEPTH`: `truecolor`, `24bit` or `256`. Other values are ignored.
    pub forced: Option<&'a OsStr>,
    /// The attached tmux client's `client_termfeatures`, when tmux reported them.
    pub tmux_features: Option<&'a str>,
    /// Whether the process runs inside GNU screen (`STY` is set).
    pub screen: bool,
    /// `COLORTERM`.
    pub colorterm: Option<&'a OsStr>,
}

impl ColorDepth {
    /// Detects the color depth from the environment.
    ///
    /// `MIRE_COLOR_DEPTH=truecolor` or `256` forces the depth. Inside tmux,
    /// tmux decides: it rounds to 256 colors unless the attached client has
    /// the `RGB` feature, so its `client_termfeatures` wins when tmux reports
    /// them within 200 ms. GNU screen 4.x rounds to 256 colors while passing
    /// `COLORTERM` through, so `STY` means 256 colors. Otherwise
    /// `COLORTERM=truecolor` or `24bit` means truecolor, and anything else
    /// means 256 colors.
    pub fn detect() -> Self {
        let forced = std::env::var_os("MIRE_COLOR_DEPTH");
        if let Some(depth) = forced.as_deref().and_then(Self::parse_forced) {
            return depth;
        }
        let tmux = if std::env::var_os("TMUX").is_some() {
            let mut command = Command::new("tmux");
            command.args(["display-message", "-p", "#{client_termfeatures}"]);
            query_features(command, TMUX_TIMEOUT)
        } else {
            None
        };
        let colorterm = std::env::var_os("COLORTERM");
        Self::from_signals(DepthSignals {
            forced: None,
            tmux_features: tmux.as_deref(),
            screen: std::env::var_os("STY").is_some(),
            colorterm: colorterm.as_deref(),
        })
    }

    /// Picks the depth from the signals, in order: a forced depth, tmux's
    /// client features, GNU screen, then `COLORTERM`.
    pub fn from_signals(signals: DepthSignals<'_>) -> Self {
        if let Some(depth) = signals.forced.and_then(Self::parse_forced) {
            return depth;
        }
        if let Some(features) = signals.tmux_features {
            return if features.split(',').any(|feature| feature.trim() == "RGB") {
                Self::Truecolor
            } else {
                Self::Xterm256
            };
        }
        if signals.screen {
            return Self::Xterm256;
        }
        match signals.colorterm {
            Some(value) if value == "truecolor" || value == "24bit" => Self::Truecolor,
            _ => Self::Xterm256,
        }
    }

    /// Reads a `MIRE_COLOR_DEPTH` value, or `None` for one it does not know.
    fn parse_forced(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "truecolor" | "24bit" => Some(Self::Truecolor),
            "256" => Some(Self::Xterm256),
            _ => None,
        }
    }
}

/// Runs a terminal-features query and returns its trimmed output. Returns
/// `None` when the query fails, prints nothing, as tmux does with no client
/// attached, or runs past `timeout`, in which case it is killed.
fn query_features(mut command: Command, timeout: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(TMUX_POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut features = String::new();
    child.stdout.take()?.read_to_string(&mut features).ok()?;
    let features = features.trim().to_owned();
    (!features.is_empty()).then_some(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depth(forced: Option<&str>, tmux_features: Option<&str>, screen: bool, colorterm: Option<&str>) -> ColorDepth {
        ColorDepth::from_signals(DepthSignals {
            forced: forced.map(OsStr::new),
            tmux_features,
            screen,
            colorterm: colorterm.map(OsStr::new),
        })
    }

    #[test]
    fn colorterm_selects_truecolor_outside_tmux() {
        for value in ["truecolor", "24bit"] {
            assert_eq!(depth(None, None, false, Some(value)), ColorDepth::Truecolor);
        }
        for value in [Some(""), Some("yes"), Some("256color"), None] {
            assert_eq!(
                depth(None, None, false, value),
                ColorDepth::Xterm256,
                "COLORTERM={value:?}"
            );
        }
    }

    #[test]
    fn tmux_client_features_override_colorterm() {
        assert_eq!(
            depth(None, Some("256,bpaste,title"), false, Some("truecolor")),
            ColorDepth::Xterm256
        );
        assert_eq!(depth(None, Some("256,RGB,title"), false, None), ColorDepth::Truecolor);
    }

    #[test]
    fn screen_rounds_despite_colorterm() {
        assert_eq!(depth(None, None, true, Some("truecolor")), ColorDepth::Xterm256);
    }

    #[test]
    fn tmux_client_features_override_screen() {
        assert_eq!(
            depth(None, Some("256,RGB"), true, Some("truecolor")),
            ColorDepth::Truecolor
        );
    }

    #[test]
    fn forced_depth_overrides_every_other_signal() {
        assert_eq!(depth(Some("truecolor"), Some("256"), true, None), ColorDepth::Truecolor);
        assert_eq!(depth(Some("24bit"), Some("256"), true, None), ColorDepth::Truecolor);
        assert_eq!(
            depth(Some("256"), Some("256,RGB"), false, Some("truecolor")),
            ColorDepth::Xterm256
        );
    }

    #[test]
    fn unknown_forced_depth_is_ignored() {
        for value in ["", "auto", "TrueColor", "16"] {
            assert_eq!(
                depth(Some(value), None, false, Some("truecolor")),
                ColorDepth::Truecolor,
                "{value:?}"
            );
            assert_eq!(
                depth(Some(value), None, true, Some("truecolor")),
                ColorDepth::Xterm256,
                "{value:?}"
            );
        }
    }

    #[cfg(unix)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[cfg(unix)]
    #[test]
    fn query_returns_trimmed_features() {
        let features = query_features(shell("echo ' 256,RGB '"), Duration::from_secs(5));
        assert_eq!(features.as_deref(), Some("256,RGB"));
    }

    #[cfg(unix)]
    #[test]
    fn query_ignores_failures_and_empty_output() {
        assert_eq!(query_features(shell("echo RGB; exit 1"), Duration::from_secs(5)), None);
        assert_eq!(query_features(shell("true"), Duration::from_secs(5)), None);
        assert_eq!(
            query_features(Command::new("/nonexistent/tmux"), Duration::from_secs(5)),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn query_gives_up_on_a_hung_tmux() {
        let started = Instant::now();
        let features = query_features(shell("exec sleep 30"), Duration::from_millis(50));
        assert_eq!(features, None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }
}
