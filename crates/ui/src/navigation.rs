use crossterm::event::{KeyCode, KeyEvent};

/// Which pane receives keyboard movement commands.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Focus {
    /// The continuous diff stream.
    #[default]
    Review,
    /// The file sidebar.
    Sidebar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Quit,
    ToggleHelp,
    ToggleFocus,
    MoveDown,
    MoveUp,
    PageDown,
    PageUp,
    FirstRow,
    LastRow,
    NextFile,
    PreviousFile,
    NextHunk,
    PreviousHunk,
    StartSearch,
    NextMatch,
    PreviousMatch,
    MoreContext,
    LessContext,
    ToggleWrap,
    UnifiedLayout,
    SplitLayout,
    AutomaticLayout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyPattern {
    Char(char),
    Down,
    End,
    Esc,
    Home,
    PageDown,
    PageUp,
    Tab,
    Up,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Binding {
    action: Action,
    description: &'static str,
    keys: &'static [KeyPattern],
    labels: &'static str,
}

const BINDINGS: &[Binding] = &[
    Binding {
        action: Action::Quit,
        description: "quit",
        keys: &[KeyPattern::Char('q'), KeyPattern::Esc],
        labels: "q / Esc",
    },
    Binding { action: Action::ToggleHelp, description: "toggle help", keys: &[KeyPattern::Char('?')], labels: "?" },
    Binding {
        action: Action::ToggleFocus,
        description: "switch sidebar/review focus",
        keys: &[KeyPattern::Tab],
        labels: "Tab",
    },
    Binding {
        action: Action::MoveDown,
        description: "scroll or select next file",
        keys: &[KeyPattern::Char('j'), KeyPattern::Down],
        labels: "j / Down",
    },
    Binding {
        action: Action::MoveUp,
        description: "scroll or select previous file",
        keys: &[KeyPattern::Char('k'), KeyPattern::Up],
        labels: "k / Up",
    },
    Binding { action: Action::PageDown, description: "page down", keys: &[KeyPattern::PageDown], labels: "PgDn" },
    Binding { action: Action::PageUp, description: "page up", keys: &[KeyPattern::PageUp], labels: "PgUp" },
    Binding {
        action: Action::FirstRow,
        description: "first row",
        keys: &[KeyPattern::Char('g'), KeyPattern::Home],
        labels: "g / Home",
    },
    Binding {
        action: Action::LastRow,
        description: "last row",
        keys: &[KeyPattern::Char('G'), KeyPattern::End],
        labels: "G / End",
    },
    Binding { action: Action::NextFile, description: "next file", keys: &[KeyPattern::Char(']')], labels: "]" },
    Binding {
        action: Action::PreviousFile,
        description: "previous file",
        keys: &[KeyPattern::Char('[')],
        labels: "[",
    },
    Binding { action: Action::NextHunk, description: "next hunk", keys: &[KeyPattern::Char('}')], labels: "}" },
    Binding {
        action: Action::PreviousHunk,
        description: "previous hunk",
        keys: &[KeyPattern::Char('{')],
        labels: "{",
    },
    Binding { action: Action::StartSearch, description: "search", keys: &[KeyPattern::Char('/')], labels: "/" },
    Binding {
        action: Action::NextMatch,
        description: "next search match",
        keys: &[KeyPattern::Char('n')],
        labels: "n",
    },
    Binding {
        action: Action::PreviousMatch,
        description: "previous search match",
        keys: &[KeyPattern::Char('N')],
        labels: "N",
    },
    Binding {
        action: Action::MoreContext,
        description: "show more context",
        keys: &[KeyPattern::Char('+')],
        labels: "+",
    },
    Binding {
        action: Action::LessContext,
        description: "show less context",
        keys: &[KeyPattern::Char('-')],
        labels: "-",
    },
    Binding {
        action: Action::ToggleWrap,
        description: "toggle line wrapping",
        keys: &[KeyPattern::Char('w')],
        labels: "w",
    },
    Binding {
        action: Action::UnifiedLayout,
        description: "unified layout",
        keys: &[KeyPattern::Char('1')],
        labels: "1",
    },
    Binding {
        action: Action::SplitLayout,
        description: "split layout",
        keys: &[KeyPattern::Char('2')],
        labels: "2",
    },
    Binding {
        action: Action::AutomaticLayout,
        description: "automatic layout",
        keys: &[KeyPattern::Char('3')],
        labels: "3",
    },
];

pub fn action_for(key: KeyEvent) -> Option<Action> {
    let pattern = KeyPattern::from_code(key.code)?;
    BINDINGS
        .iter()
        .find(|binding| binding.keys.contains(&pattern))
        .map(|binding| binding.action)
}

pub fn help_entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    BINDINGS
        .iter()
        .map(|binding| (binding.labels, binding.description))
        .chain([
            ("wheel", "scroll review or sidebar"),
            ("left click", "select a sidebar or review file"),
        ])
}

impl Focus {
    pub const fn toggle(self) -> Self {
        match self {
            Self::Review => Self::Sidebar,
            Self::Sidebar => Self::Review,
        }
    }
}

impl KeyPattern {
    fn from_code(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::Char(character) => Some(Self::Char(character)),
            KeyCode::Down => Some(Self::Down),
            KeyCode::End => Some(Self::End),
            KeyCode::Esc => Some(Self::Esc),
            KeyCode::Home => Some(Self::Home),
            KeyCode::PageDown => Some(Self::PageDown),
            KeyCode::PageUp => Some(Self::PageUp),
            KeyCode::Tab => Some(Self::Tab),
            KeyCode::Up => Some(Self::Up),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_help_has_no_conflicting_key_bindings() {
        let mut seen = Vec::new();
        for binding in BINDINGS {
            for key in binding.keys {
                assert!(!seen.contains(key), "binding conflict for {key:?}");
                seen.push(*key);
            }
        }
    }

    #[test]
    fn navigation_help_lists_every_keyboard_binding() {
        let help = help_entries().collect::<Vec<_>>();
        for binding in BINDINGS {
            assert!(help.contains(&(binding.labels, binding.description)));
        }
    }
}
