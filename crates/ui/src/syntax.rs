use std::collections::VecDeque;

use inkjet::tree_sitter_highlight::HighlightEvent;
use inkjet::{Highlighter, Language};
use mire_core::{FileContent, FileDiff};

const MAX_CACHE_ENTRIES: usize = 256;
const MAX_HIGHLIGHT_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LanguageKind {
    Inkjet(Language),
    Markdown,
    Plain,
}

/// A syntax scope covering one byte range of source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntaxRange {
    /// First byte covered by the scope.
    pub start: usize,
    /// First byte after the scope.
    pub end: usize,
    /// Inkjet's stable semantic scope index.
    pub scope: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CacheEntry {
    file: usize,
    hunk: usize,
    line: usize,
    ranges: Vec<SyntaxRange>,
}

/// Bounded, presentation-only syntax highlighting cache.
pub struct SyntaxCache {
    entries: VecDeque<CacheEntry>,
    highlighter: Highlighter,
    override_language: Option<LanguageKind>,
}

impl SyntaxCache {
    /// Creates a cache with an optional language name or alias override.
    pub fn new(language_override: Option<&str>) -> Self {
        Self {
            entries: VecDeque::new(),
            highlighter: Highlighter::new(),
            override_language: language_override.map(language_from_token),
        }
    }

    /// Highlights one line, returning no ranges for plain text or on failure.
    pub fn ranges(
        &mut self, file_index: usize, hunk_index: usize, line_index: usize, file: &FileDiff, source: &str,
    ) -> &[SyntaxRange] {
        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.file == file_index && entry.hunk == hunk_index && entry.line == line_index)
        {
            self.entries.rotate_left(position);
            return self.entries.front().map_or(&[], |entry| entry.ranges.as_slice());
        }

        let ranges = if source.len() > MAX_HIGHLIGHT_BYTES {
            Vec::new()
        } else {
            match self
                .override_language
                .or_else(|| detect_language(file))
                .unwrap_or(LanguageKind::Plain)
            {
                LanguageKind::Inkjet(language) => {
                    highlight(&mut self.highlighter, language, source).unwrap_or_default()
                }
                LanguageKind::Markdown => highlight_markdown(source),
                LanguageKind::Plain => Vec::new(),
            }
        };
        self.entries
            .push_front(CacheEntry { file: file_index, hunk: hunk_index, line: line_index, ranges });
        self.entries.truncate(MAX_CACHE_ENTRIES);
        self.entries.front().map_or(&[], |entry| entry.ranges.as_slice())
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

fn detect_language(file: &FileDiff) -> Option<LanguageKind> {
    let path = file.new_side().or(file.old_side())?.path.as_bytes();
    let path = String::from_utf8_lossy(path);
    let name = path.rsplit('/').next().unwrap_or(&path);
    let token = match name {
        "Dockerfile" => "dockerfile",
        "Makefile" => "makefile",
        _ => name.rsplit_once('.').map(|(_, extension)| extension).unwrap_or(""),
    };
    if matches!(token, "md" | "markdown") {
        Some(LanguageKind::Markdown)
    } else {
        Language::from_token(token)
            .map(LanguageKind::Inkjet)
            .or_else(|| detect_shebang(file))
    }
}

fn detect_shebang(file: &FileDiff) -> Option<LanguageKind> {
    let FileContent::Text { hunks } = file.content() else {
        return None;
    };
    let first = hunks.iter().flat_map(|hunk| hunk.lines()).find(|line| {
        line.old_line().is_some_and(|number| number.get() == 1)
            || line.new_line().is_some_and(|number| number.get() == 1)
    })?;
    let source = std::str::from_utf8(first.content()).ok()?;
    if !source.starts_with("#!") {
        return None;
    }
    ["python", "bash", "sh", "node", "ruby"]
        .into_iter()
        .find(|token| source.contains(token))
        .and_then(|token| Language::from_token(if token == "node" { "javascript" } else { token }))
        .map(LanguageKind::Inkjet)
}

fn language_from_token(token: &str) -> LanguageKind {
    match token {
        "md" | "markdown" => LanguageKind::Markdown,
        "plain" | "plaintext" | "text" => LanguageKind::Plain,
        _ => Language::from_token(token).map_or(LanguageKind::Plain, LanguageKind::Inkjet),
    }
}

fn highlight_markdown(source: &str) -> Vec<SyntaxRange> {
    let scope_name = if source.trim_start().starts_with('#') {
        "markup.heading"
    } else if source.trim_start().starts_with(['-', '*', '+']) {
        "markup.list"
    } else if source.contains('`') {
        "markup.raw.inline"
    } else {
        return Vec::new();
    };
    let scope = inkjet::constants::HIGHLIGHT_NAMES
        .iter()
        .position(|name| *name == scope_name)
        .unwrap_or_default();
    vec![SyntaxRange { start: 0, end: source.len(), scope }]
}

fn highlight(highlighter: &mut Highlighter, language: Language, source: &str) -> inkjet::Result<Vec<SyntaxRange>> {
    let events = highlighter.highlight_raw(language, &source)?;
    let mut active = Vec::new();
    let mut ranges = Vec::new();
    for event in events {
        match event? {
            HighlightEvent::HighlightStart(scope) => active.push(scope.0),
            HighlightEvent::Source { start, end } => {
                if let Some(scope) = active.last() {
                    ranges.push(SyntaxRange { start, end, scope: *scope });
                }
            }
            HighlightEvent::HighlightEnd => {
                active.pop();
            }
        }
    }
    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

    const PATCH: &[u8] = include_bytes!("../../core/tests/fixtures/patches/mixed_languages.patch");

    #[test]
    fn detection_uses_extensions_shebangs_overrides_and_plaintext_fallback() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let detected = changeset.files().iter().map(detect_language).collect::<Vec<_>>();
        assert!(detected.contains(&Some(LanguageKind::Inkjet(Language::Rust))));
        assert!(detected.contains(&Some(LanguageKind::Inkjet(Language::Python))));
        assert!(detected.contains(&None));
        let cache = SyntaxCache::new(Some("typescript"));
        assert_eq!(
            cache.override_language,
            Some(LanguageKind::Inkjet(Language::Typescript))
        );
    }

    #[test]
    fn highlighting_cache_is_bounded() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let file = &changeset.files()[0];
        let mut cache = SyntaxCache::new(None);
        for line in 0..MAX_CACHE_ENTRIES + 20 {
            cache.ranges(0, 0, line, file, "let value = 1;");
        }
        assert_eq!(cache.len(), MAX_CACHE_ENTRIES);

        let long_source = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        assert!(
            cache
                .ranges(0, 0, MAX_CACHE_ENTRIES + 21, file, &long_source)
                .is_empty()
        );
    }

    #[test]
    fn selected_languages_and_markdown_produce_semantic_ranges() {
        let cases = [
            (LanguageKind::Inkjet(Language::Rust), "pub fn main() {}"),
            (LanguageKind::Inkjet(Language::Typescript), "const value: number = 1;"),
            (LanguageKind::Inkjet(Language::Python), "def main(): return 1"),
        ];
        let mut highlighter = Highlighter::new();
        for (language, source) in cases {
            let LanguageKind::Inkjet(language) = language else {
                unreachable!("test cases use Inkjet languages");
            };
            assert!(!highlight(&mut highlighter, language, source).unwrap().is_empty());
        }
        assert!(!highlight_markdown("# Review heading").is_empty());
    }
}
