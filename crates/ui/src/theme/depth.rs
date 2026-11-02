//! How many colors the terminal shows, so a palette can pick colors that
//! survive the terminal's rounding.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Longest wait for tmux to report its clients' features. A stopped or busy
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
    /// `MIRE_COLOR_DEPTH`: `truecolor`, `24bit` or `256`, ignoring case and
    /// surrounding whitespace. Other values are ignored.
    pub forced: Option<&'a OsStr>,
    /// `client_termfeatures` of each tmux client attached to this pane's
    /// session, one client per line, when tmux reported them.
    pub tmux_features: Option<&'a str>,
    /// Whether the process runs inside GNU screen (`STY` is set).
    pub screen: bool,
    /// `COLORTERM`, ignoring case and surrounding whitespace.
    pub colorterm: Option<&'a OsStr>,
}

impl ColorDepth {
    /// Detects the color depth from the environment.
    ///
    /// `MIRE_COLOR_DEPTH=truecolor` or `256` forces the depth. Inside tmux,
    /// tmux decides: it rounds to 256 colors for each attached client that
    /// lacks the `RGB` feature, so truecolor needs every client attached to
    /// this pane's session to have it. tmux's answer wins when it arrives
    /// within 200 ms; a slower tmux falls through to the checks below.
    /// GNU screen 4.x rounds to 256 colors while passing
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
            command.args(["list-clients", "-F", "#{client_termfeatures}"]);
            // `-t` takes this pane's id and lists the clients on its session.
            // Without it, tmux lists every client on the server, which can
            // only pick 256 colors more often.
            if let Some(pane) = std::env::var_os("TMUX_PANE") {
                command.arg("-t").arg(pane);
            }
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
        if let Some(rgb) = signals.tmux_features.and_then(every_client_has_rgb) {
            return if rgb { Self::Truecolor } else { Self::Xterm256 };
        }
        if signals.screen {
            return Self::Xterm256;
        }
        match signals.colorterm.and_then(normalize).as_deref() {
            Some("truecolor" | "24bit") => Self::Truecolor,
            _ => Self::Xterm256,
        }
    }

    /// Reads a `MIRE_COLOR_DEPTH` value, or `None` for one it does not know.
    fn parse_forced(value: &OsStr) -> Option<Self> {
        match normalize(value)?.as_str() {
            "truecolor" | "24bit" => Some(Self::Truecolor),
            "256" => Some(Self::Xterm256),
            _ => None,
        }
    }
}

/// Whether every tmux client in a `client_termfeatures` listing, one client
/// per line, has the `RGB` feature. Returns `None` for a listing with no
/// clients.
fn every_client_has_rgb(listing: &str) -> Option<bool> {
    let mut clients = listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .peekable();
    clients.peek()?;
    Some(clients.all(|features| features.split(',').any(|feature| feature.trim() == "RGB")))
}

/// Trims an environment value and lowercases it, or returns `None` when it is
/// not UTF-8.
fn normalize(value: &OsStr) -> Option<String> {
    value.to_str().map(|value| value.trim().to_ascii_lowercase())
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
    fn colorterm_ignores_case_and_whitespace() {
        for value in ["TrueColor", " truecolor\n", "24BIT", "\t24bit "] {
            assert_eq!(
                depth(None, None, false, Some(value)),
                ColorDepth::Truecolor,
                "{value:?}"
            );
        }
    }

    #[test]
    fn forced_depth_ignores_case_and_whitespace() {
        for value in ["TrueColor", " truecolor ", "24BIT\n"] {
            assert_eq!(depth(Some(value), None, true, None), ColorDepth::Truecolor, "{value:?}");
        }
        assert_eq!(
            depth(Some(" 256\n"), None, false, Some("truecolor")),
            ColorDepth::Xterm256
        );
    }

    #[test]
    fn tmux_truecolor_needs_every_client_to_have_rgb() {
        assert_eq!(
            depth(None, Some("bpaste,RGB,title\nRGB,focus\n"), false, None),
            ColorDepth::Truecolor
        );
        assert_eq!(
            depth(
                None,
                Some("bpaste,RGB,title\nbpaste,focus,title"),
                false,
                Some("truecolor")
            ),
            ColorDepth::Xterm256
        );
        assert_eq!(
            depth(
                None,
                Some("bpaste,focus,title\nbpaste,RGB,title"),
                false,
                Some("truecolor")
            ),
            ColorDepth::Xterm256
        );
    }

    #[test]
    fn empty_tmux_listing_falls_back() {
        for listing in ["", "\n", " \n \n"] {
            assert_eq!(
                depth(None, Some(listing), false, Some("truecolor")),
                ColorDepth::Truecolor
            );
            assert_eq!(depth(None, Some(listing), false, None), ColorDepth::Xterm256);
        }
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
        for value in ["", "auto", "truecolour", "16"] {
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
