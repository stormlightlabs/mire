//! How many colors the terminal shows, so a palette can pick colors that
//! survive the terminal's rounding.

use std::ffi::OsStr;
use std::process::Command;

/// How many colors the terminal shows for an RGB color.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorDepth {
    /// The terminal shows every RGB color as sent.
    Truecolor,
    /// The terminal rounds each RGB color to its nearest xterm 256-colour index.
    Xterm256,
}

impl ColorDepth {
    /// Detects the color depth from the environment.
    ///
    /// Inside tmux, tmux decides: it rounds to 256 colors unless the attached
    /// client has the `RGB` feature, so its `client_termfeatures` wins when
    /// tmux reports them. Otherwise `COLORTERM=truecolor` or `24bit` means
    /// truecolor, and anything else means 256 colors.
    pub fn detect() -> Self {
        let tmux = if std::env::var_os("TMUX").is_some() { tmux_features() } else { None };
        Self::from_signals(std::env::var_os("COLORTERM").as_deref(), tmux.as_deref())
    }

    /// Picks the depth from `COLORTERM` and tmux's client features, when known.
    pub fn from_signals(colorterm: Option<&OsStr>, tmux_features: Option<&str>) -> Self {
        if let Some(features) = tmux_features {
            return if features.split(',').any(|feature| feature.trim() == "RGB") {
                Self::Truecolor
            } else {
                Self::Xterm256
            };
        }
        match colorterm {
            Some(value) if value == "truecolor" || value == "24bit" => Self::Truecolor,
            _ => Self::Xterm256,
        }
    }
}

/// Asks tmux for the attached client's terminal features. Returns `None`
/// when tmux fails or reports none, as it does with no client attached.
fn tmux_features() -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{client_termfeatures}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let features = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!features.is_empty()).then_some(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorterm_selects_truecolor_outside_tmux() {
        for value in ["truecolor", "24bit"] {
            assert_eq!(
                ColorDepth::from_signals(Some(OsStr::new(value)), None),
                ColorDepth::Truecolor
            );
        }
        for value in [Some(""), Some("yes"), Some("256color"), None] {
            assert_eq!(
                ColorDepth::from_signals(value.map(OsStr::new), None),
                ColorDepth::Xterm256,
                "COLORTERM={value:?}"
            );
        }
    }

    #[test]
    fn tmux_client_features_override_colorterm() {
        let truecolor = Some(OsStr::new("truecolor"));
        assert_eq!(
            ColorDepth::from_signals(truecolor, Some("256,bpaste,title")),
            ColorDepth::Xterm256
        );
        assert_eq!(
            ColorDepth::from_signals(None, Some("256,RGB,title")),
            ColorDepth::Truecolor
        );
    }
}
