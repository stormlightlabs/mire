use sha2::{Digest, Sha256};
use unidiff::PatchSet;

use crate::{
    BytePath, ByteString, Changeset, ChangesetSource, DiffLine, FileContent, FileDiff, FileMode, FileSide, FileStatus,
    Fingerprint, Hunk, LineKind, LineNumber, MissingNewline,
};

use super::{PatchError, Result};

const NULL_PATH: &str = "/dev/null";
const NO_NEWLINE_MARKER: &str = "\\ No newline at end of file";

#[derive(Clone, Copy)]
struct RawLine<'a> {
    start: usize,
    raw: &'a str,
}

#[derive(Clone, Copy)]
struct FileBlock<'a> {
    lines: &'a [RawLine<'a>],
    start: usize,
    end: usize,
}

#[derive(Default)]
struct FileMetadata {
    binary: bool,
    copy_from: Option<Vec<u8>>,
    copy_to: Option<Vec<u8>>,
    deleted_mode: Option<FileMode>,
    diff_new: Option<Vec<u8>>,
    diff_old: Option<Vec<u8>>,
    index_mode: Option<FileMode>,
    new_file_mode: Option<FileMode>,
    new_mode: Option<FileMode>,
    new_path: Option<Vec<u8>>,
    old_mode: Option<FileMode>,
    old_path: Option<Vec<u8>>,
    rename_from: Option<Vec<u8>>,
    rename_to: Option<Vec<u8>>,
    similarity: Option<u8>,
}

struct ResolvedFile {
    status: FileStatus,
    old_path: Option<Vec<u8>>,
    new_path: Option<Vec<u8>>,
}

impl FileMetadata {
    fn parse_line(&mut self, raw_line: RawLine<'_>) -> Result<()> {
        let line = grammar_line(raw_line);
        let offset = raw_line.start;

        if let Some(value) = line.strip_prefix("diff --git ") {
            let (old, new) = parse_diff_paths(value, offset)?;
            self.diff_old = Some(strip_side_prefix(old, b"a/"));
            self.diff_new = Some(strip_side_prefix(new, b"b/"));
        } else if let Some(value) = line.strip_prefix("new file mode ") {
            self.new_file_mode = Some(parse_mode(value, offset)?);
        } else if let Some(value) = line.strip_prefix("deleted file mode ") {
            self.deleted_mode = Some(parse_mode(value, offset)?);
        } else if let Some(value) = line.strip_prefix("old mode ") {
            self.old_mode = Some(parse_mode(value, offset)?);
        } else if let Some(value) = line.strip_prefix("new mode ") {
            self.new_mode = Some(parse_mode(value, offset)?);
        } else if let Some(value) = line.strip_prefix("similarity index ") {
            self.similarity = Some(parse_similarity(value, offset)?);
        } else if let Some(value) = line.strip_prefix("rename from ") {
            self.rename_from = Some(parse_git_path(value, offset)?);
        } else if let Some(value) = line.strip_prefix("rename to ") {
            self.rename_to = Some(parse_git_path(value, offset)?);
        } else if let Some(value) = line.strip_prefix("copy from ") {
            self.copy_from = Some(parse_git_path(value, offset)?);
        } else if let Some(value) = line.strip_prefix("copy to ") {
            self.copy_to = Some(parse_git_path(value, offset)?);
        } else if let Some(value) = line.strip_prefix("index ") {
            self.index_mode = parse_index_mode(value, offset)?;
        } else if let Some(value) = line.strip_prefix("--- ") {
            self.old_path = parse_file_header_path(value, b"a/", offset)?;
        } else if let Some(value) = line.strip_prefix("+++ ") {
            self.new_path = parse_file_header_path(value, b"b/", offset)?;
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.binary = true;
        }
        Ok(())
    }

    fn validate(&self, offset: usize) -> Result<()> {
        let renamed = paired_metadata(self.rename_from.as_ref(), self.rename_to.as_ref(), offset, "rename")?;
        let copied = paired_metadata(self.copy_from.as_ref(), self.copy_to.as_ref(), offset, "copy")?;
        let operation_count = usize::from(self.new_file_mode.is_some())
            + usize::from(self.deleted_mode.is_some())
            + usize::from(renamed)
            + usize::from(copied);
        if operation_count > 1 {
            return Err(malformed(
                offset,
                "file has conflicting add, delete, rename, or copy metadata",
            ));
        }
        if self.similarity.is_some() && !renamed && !copied {
            return Err(malformed(offset, "similarity index requires rename or copy metadata"));
        }
        Ok(())
    }

    fn validate_content(&self, has_hunks: bool, offset: usize) -> Result<()> {
        if self.binary && has_hunks {
            return Err(malformed(offset, "binary file metadata cannot contain text hunks"));
        }
        Ok(())
    }
}

pub fn parse(text: &str, source: ChangesetSource) -> Result<Changeset> {
    if text.is_empty() {
        return Ok(Changeset::new(source, Vec::new(), fingerprint(text.as_bytes())));
    }

    let lines = raw_lines(text);
    validate_top_level(&lines)?;
    let blocks = file_blocks(&lines, text.len());
    let mut files = Vec::with_capacity(blocks.len());
    for block in blocks {
        files.push(parse_file(text, block)?);
    }

    Ok(Changeset::new(source, files, fingerprint(text.as_bytes())))
}

fn parse_file(text: &str, block: FileBlock<'_>) -> Result<FileDiff> {
    validate_with_unidiff(text, block)?;
    let metadata = parse_metadata(block.lines)?;
    let hunks = parse_hunks(block.lines)?;
    metadata.validate_content(!hunks.is_empty(), block.start)?;
    let ResolvedFile { status, old_path, new_path } = resolve_paths(&metadata, block.start)?;
    let old_mode = metadata.deleted_mode.or(metadata.old_mode).or(metadata.index_mode);
    let new_mode = metadata.new_file_mode.or(metadata.new_mode).or(metadata.index_mode);
    let old = make_side(old_path, old_mode, block.start)?;
    let new = make_side(new_path, new_mode, block.start)?;
    let content = if metadata.binary { FileContent::Binary } else { FileContent::Text { hunks } };

    FileDiff::new(
        status,
        old,
        new,
        metadata.similarity,
        content,
        fingerprint(&text.as_bytes()[block.start..block.end]),
    )
    .map_err(|error| PatchError::InvalidModel { offset: block.start, reason: error.to_string() })
}

fn validate_top_level(lines: &[RawLine<'_>]) -> Result<()> {
    let first = lines.iter().find(|line| !grammar_line(**line).is_empty()).copied();
    match first {
        Some(line) if grammar_line(line).starts_with("diff --git ") || grammar_line(line).starts_with("--- ") => Ok(()),
        Some(line) => Err(malformed(
            line.start,
            "expected a Git diff header or unified file header",
        )),
        None => Ok(()),
    }
}

fn validate_with_unidiff(text: &str, block: FileBlock<'_>) -> Result<()> {
    let has_text_header = block.lines.iter().any(|line| grammar_line(*line).starts_with("--- "));
    let has_hunk_like_line = block.lines.iter().any(|line| grammar_line(*line).starts_with("@@"));

    for raw_line in block.lines {
        let line = grammar_line(*raw_line);
        if line.starts_with("@@") && parse_hunk_header(line).is_err() {
            return Err(malformed(raw_line.start, "invalid unified hunk header"));
        }
    }
    if has_hunk_like_line && !has_text_header {
        return Err(malformed(block.start, "hunk is missing its --- and +++ file headers"));
    }
    if !has_text_header {
        return Ok(());
    }

    let mut patch = PatchSet::new();
    patch.parse(&text[block.start..block.end]).map_err(|error| {
        let reason = match error {
            unidiff::Error::TargetWithoutSource(_) => "+++ header has no preceding --- header",
            unidiff::Error::UnexpectedHunk(_) => "hunk has no file header",
            unidiff::Error::ExpectLine(_) => "invalid line in unified hunk",
        };
        malformed(block.start, reason)
    })?;
    if patch.is_empty() {
        return Err(malformed(block.start, "patch contains no parseable file header"));
    }
    Ok(())
}

fn parse_metadata(block: &[RawLine<'_>]) -> Result<FileMetadata> {
    let mut metadata = FileMetadata::default();
    for &raw_line in block.iter().take_while(|line| !grammar_line(**line).starts_with("@@")) {
        metadata.parse_line(raw_line)?;
    }
    metadata.validate(block.first().map_or(0, |line| line.start))?;
    Ok(metadata)
}

fn paired_metadata<T>(first: Option<&T>, second: Option<&T>, offset: usize, operation: &str) -> Result<bool> {
    match (first.is_some(), second.is_some()) {
        (false, false) => Ok(false),
        (true, true) => Ok(true),
        _ => Err(malformed(
            offset,
            format!("{operation} metadata requires both source and destination paths"),
        )),
    }
}

fn parse_hunks(block: &[RawLine<'_>]) -> Result<Vec<Hunk>> {
    let mut hunks = Vec::new();
    let mut index = 0;
    while index < block.len() {
        let header = grammar_line(block[index]);
        if !header.starts_with("@@") {
            index += 1;
            continue;
        }
        let (old_start, old_count, new_start, new_count, section) =
            parse_hunk_header(header).map_err(|reason| malformed(block[index].start, reason))?;
        let hunk_start = block[index].start;
        index += 1;
        let mut old_line = old_start;
        let mut new_line = new_start;
        let mut old_seen = 0_u64;
        let mut new_seen = 0_u64;
        let mut lines = Vec::new();

        while index < block.len() && (old_seen < old_count || new_seen < new_count) {
            let raw = block[index];
            let grammar = grammar_line(raw);
            if grammar == NO_NEWLINE_MARKER {
                mark_missing_newline(&mut lines, raw.start)?;
                index += 1;
                continue;
            }
            let (kind, content) = parse_body_line(raw)?;
            let old_number = match kind {
                LineKind::Addition => None,
                LineKind::Context | LineKind::Deletion => LineNumber::new(old_line),
            };
            let new_number = match kind {
                LineKind::Deletion => None,
                LineKind::Context | LineKind::Addition => LineNumber::new(new_line),
            };
            if !matches!(kind, LineKind::Addition) {
                old_seen += 1;
                old_line = old_line.saturating_add(1);
            }
            if !matches!(kind, LineKind::Deletion) {
                new_seen += 1;
                new_line = new_line.saturating_add(1);
            }
            if old_seen > old_count || new_seen > new_count {
                return Err(malformed(raw.start, "hunk contains more lines than its header"));
            }
            lines.push(
                DiffLine::new(
                    kind,
                    old_number,
                    new_number,
                    ByteString::new(content),
                    MissingNewline::None,
                )
                .map_err(|error| PatchError::InvalidModel { offset: raw.start, reason: error.to_string() })?,
            );
            index += 1;
        }
        while index < block.len() && grammar_line(block[index]) == NO_NEWLINE_MARKER {
            mark_missing_newline(&mut lines, block[index].start)?;
            index += 1;
        }
        if old_seen != old_count || new_seen != new_count {
            return Err(malformed(hunk_start, "hunk ended before its declared line counts"));
        }
        let hunk_end = block.get(index).map_or_else(
            || block.last().map_or(hunk_start, |line| line.start + line.raw.len()),
            |line| line.start,
        );
        hunks.push(
            Hunk::new(
                old_start,
                old_count,
                new_start,
                new_count,
                ByteString::new(section.as_bytes()),
                lines,
                fingerprint_range(block, hunk_start, hunk_end),
            )
            .map_err(|error| PatchError::InvalidModel { offset: hunk_start, reason: error.to_string() })?,
        );
    }
    Ok(hunks)
}

fn resolve_paths(metadata: &FileMetadata, offset: usize) -> Result<ResolvedFile> {
    let mut old = metadata
        .rename_from
        .as_ref()
        .or(metadata.copy_from.as_ref())
        .or(metadata.old_path.as_ref())
        .or(metadata.diff_old.as_ref())
        .cloned();
    let mut new = metadata
        .rename_to
        .as_ref()
        .or(metadata.copy_to.as_ref())
        .or(metadata.new_path.as_ref())
        .or(metadata.diff_new.as_ref())
        .cloned();

    let status = if metadata.new_file_mode.is_some() || old.is_none() {
        old = None;
        FileStatus::Added
    } else if metadata.deleted_mode.is_some() || new.is_none() {
        new = None;
        FileStatus::Deleted
    } else if metadata.rename_from.is_some() || metadata.rename_to.is_some() {
        FileStatus::Renamed
    } else if metadata.copy_from.is_some() || metadata.copy_to.is_some() {
        FileStatus::Copied
    } else if old == new {
        FileStatus::Modified
    } else {
        FileStatus::Renamed
    };
    if old.is_none() && new.is_none() {
        return Err(malformed(offset, "file diff contains no usable path"));
    }
    Ok(ResolvedFile { status, old_path: old, new_path: new })
}

fn parse_hunk_header(line: &str) -> std::result::Result<(u64, u64, u64, u64, &str), &'static str> {
    let rest = line.strip_prefix("@@ -").ok_or("invalid unified hunk header")?;
    let (old, rest) = rest.split_once(" +").ok_or("invalid unified hunk header")?;
    let (new, section) = rest.split_once(" @@").ok_or("invalid unified hunk header")?;
    let (old_start, old_count) = parse_range(old)?;
    let (new_start, new_count) = parse_range(new)?;
    old_start.checked_add(old_count).ok_or("old hunk range overflows")?;
    new_start.checked_add(new_count).ok_or("new hunk range overflows")?;
    usize::try_from(old_start).map_err(|_| "old hunk start exceeds platform limits")?;
    usize::try_from(old_count).map_err(|_| "old hunk count exceeds platform limits")?;
    usize::try_from(new_start).map_err(|_| "new hunk start exceeds platform limits")?;
    usize::try_from(new_count).map_err(|_| "new hunk count exceeds platform limits")?;
    let section = section.strip_prefix(' ').unwrap_or(section);
    Ok((old_start, old_count, new_start, new_count, section))
}

fn parse_range(range: &str) -> std::result::Result<(u64, u64), &'static str> {
    let (start, count) = range.split_once(',').map_or((range, "1"), |parts| parts);
    let start = start.parse().map_err(|_| "invalid hunk line number")?;
    let count = count.parse().map_err(|_| "invalid hunk line count")?;
    Ok((start, count))
}

fn parse_body_line(raw: RawLine<'_>) -> Result<(LineKind, Vec<u8>)> {
    let bytes = raw.raw.as_bytes();
    let (&prefix, content) = bytes
        .split_first()
        .ok_or_else(|| malformed(raw.start, "empty line inside hunk has no context prefix"))?;
    let kind = match prefix {
        b' ' => LineKind::Context,
        b'+' => LineKind::Addition,
        b'-' => LineKind::Deletion,
        _ => return Err(malformed(raw.start, "invalid line prefix inside hunk")),
    };
    Ok((kind, content.to_vec()))
}

fn mark_missing_newline(lines: &mut [DiffLine], offset: usize) -> Result<()> {
    let line = lines
        .last_mut()
        .ok_or_else(|| malformed(offset, "missing-newline marker has no preceding line"))?;
    let marker = match line.kind() {
        LineKind::Context => MissingNewline::Both,
        LineKind::Addition => MissingNewline::New,
        LineKind::Deletion => MissingNewline::Old,
    };
    let replacement = DiffLine::new(
        line.kind(),
        line.old_line(),
        line.new_line(),
        ByteString::new(line.content()),
        marker,
    )
    .map_err(|error| PatchError::InvalidModel { offset, reason: error.to_string() })?;
    *line = replacement;
    Ok(())
}

fn parse_diff_paths(value: &str, offset: usize) -> Result<(Vec<u8>, Vec<u8>)> {
    if !value.starts_with('"') {
        let mut fallback = None;
        for (separator, _) in value.match_indices(" b/") {
            let old = &value.as_bytes()[..separator];
            let new = &value.as_bytes()[separator + 1..];
            if fallback.is_none() {
                fallback = Some((old.to_vec(), new.to_vec()));
            }
            if old.strip_prefix(b"a/") == new.strip_prefix(b"b/") {
                return Ok((old.to_vec(), new.to_vec()));
            }
        }
        return fallback.ok_or_else(|| malformed(offset, "diff --git header must contain two paths"));
    }
    let (old, rest) = parse_path_token(value, offset)?;
    let (new, trailing) = parse_path_token(rest.trim_start(), offset)?;
    if !trailing.trim().is_empty() {
        return Err(malformed(offset, "diff --git header contains extra path data"));
    }
    Ok((old, new))
}

fn parse_file_header_path(value: &str, prefix: &[u8], offset: usize) -> Result<Option<Vec<u8>>> {
    let value = value.split_once('\t').map_or(value, |(path, _)| path);
    if value == NULL_PATH {
        return Ok(None);
    }
    Ok(Some(strip_side_prefix(parse_git_path(value, offset)?, prefix)))
}

fn parse_git_path(value: &str, offset: usize) -> Result<Vec<u8>> {
    if !value.starts_with('"') {
        if value.is_empty() {
            return Err(malformed(offset, "path is empty"));
        }
        return Ok(value.as_bytes().to_vec());
    }
    let (path, trailing) = parse_path_token(value.trim(), offset)?;
    if !trailing.trim().is_empty() {
        return Err(malformed(offset, "quoted path contains trailing data"));
    }
    Ok(path)
}

fn parse_path_token(value: &str, offset: usize) -> Result<(Vec<u8>, &str)> {
    if let Some(value) = value.strip_prefix('"') {
        parse_quoted_path(value, offset)
    } else {
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        if end == 0 {
            return Err(malformed(offset, "path is empty"));
        }
        Ok((value.as_bytes()[..end].to_vec(), &value[end..]))
    }
}

fn parse_quoted_path(mut value: &str, offset: usize) -> Result<(Vec<u8>, &str)> {
    let mut output = Vec::new();
    while !value.is_empty() {
        if let Some(rest) = value.strip_prefix('"') {
            return Ok((output, rest));
        }
        if let Some(rest) = value.strip_prefix('\\') {
            let bytes = rest.as_bytes();
            let Some(&escaped) = bytes.first() else {
                return Err(malformed(offset, "quoted path ends after an escape"));
            };
            match escaped {
                b'"' | b'\\' => {
                    output.push(escaped);
                    value = &rest[1..];
                }
                b'a' | b'b' | b'f' | b'n' | b'r' | b't' | b'v' => {
                    output.push(match escaped {
                        b'a' => 0x07,
                        b'b' => 0x08,
                        b'f' => 0x0c,
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'v' => 0x0b,
                        _ => escaped,
                    });
                    value = &rest[1..];
                }
                b'0'..=b'7' => {
                    let count = bytes
                        .iter()
                        .take(3)
                        .take_while(|byte| matches!(byte, b'0'..=b'7'))
                        .count();
                    let octal = std::str::from_utf8(&bytes[..count])
                        .map_err(|_| malformed(offset, "path escape is not ASCII"))?;
                    let byte = u8::from_str_radix(octal, 8)
                        .map_err(|_| malformed(offset, "path octal escape is out of range"))?;
                    output.push(byte);
                    value = &rest[count..];
                }
                _ => return Err(malformed(offset, "quoted path contains an unsupported escape")),
            }
        } else {
            let character = value
                .chars()
                .next()
                .ok_or_else(|| malformed(offset, "quoted path is incomplete"))?;
            let length = character.len_utf8();
            output.extend_from_slice(&value.as_bytes()[..length]);
            value = &value[length..];
        }
    }
    Err(malformed(offset, "quoted path is missing its closing quote"))
}

fn parse_mode(value: &str, offset: usize) -> Result<FileMode> {
    if value.len() != 6 || !value.bytes().all(|byte| matches!(byte, b'0'..=b'7')) {
        return Err(malformed(offset, "file mode must contain six octal digits"));
    }
    let mode = u32::from_str_radix(value, 8).map_err(|_| malformed(offset, "file mode is not valid octal"))?;
    FileMode::new(mode).map_err(|error| PatchError::InvalidModel { offset, reason: error.to_string() })
}

fn parse_index_mode(value: &str, offset: usize) -> Result<Option<FileMode>> {
    let Some((_, mode)) = value.rsplit_once(' ') else {
        return Ok(None);
    };
    parse_mode(mode, offset).map(Some)
}

fn parse_similarity(value: &str, offset: usize) -> Result<u8> {
    let percentage = value
        .strip_suffix('%')
        .ok_or_else(|| malformed(offset, "similarity index must end with %"))?;
    let similarity = percentage
        .parse::<u8>()
        .map_err(|_| malformed(offset, "similarity index must be between 0 and 100"))?;
    if similarity > 100 {
        return Err(malformed(offset, "similarity index must be between 0 and 100"));
    }
    Ok(similarity)
}

fn make_side(path: Option<Vec<u8>>, mode: Option<FileMode>, offset: usize) -> Result<Option<FileSide>> {
    path.map(|path| {
        BytePath::new(path)
            .map(|path| FileSide { path, mode, fingerprint: None })
            .map_err(|error| malformed(offset, error.to_string()))
    })
    .transpose()
}

fn file_blocks<'a>(lines: &'a [RawLine<'a>], text_len: usize) -> Vec<FileBlock<'a>> {
    let mut starts = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| grammar_line(*line).starts_with("diff --git ").then_some(index))
        .collect::<Vec<_>>();
    if starts.is_empty() {
        starts = plain_file_starts(lines);
        if starts.is_empty() {
            starts.push(0);
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(position, start_index)| {
            let end_index = starts.get(position + 1).copied().unwrap_or(lines.len());
            let block_lines = &lines[*start_index..end_index];
            let start = block_lines.first().map_or(0, |line| line.start);
            let end = starts.get(position + 1).map_or(text_len, |next| lines[*next].start);
            FileBlock { lines: block_lines, start, end }
        })
        .collect()
}

fn plain_file_starts(lines: &[RawLine<'_>]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut old_remaining = 0_u64;
    let mut new_remaining = 0_u64;
    let mut index = 0;
    while index < lines.len() {
        let line = grammar_line(lines[index]);
        if old_remaining > 0 || new_remaining > 0 {
            if line != NO_NEWLINE_MARKER {
                match line.as_bytes().first() {
                    Some(b' ') => {
                        old_remaining = old_remaining.saturating_sub(1);
                        new_remaining = new_remaining.saturating_sub(1);
                    }
                    Some(b'-') => old_remaining = old_remaining.saturating_sub(1),
                    Some(b'+') => new_remaining = new_remaining.saturating_sub(1),
                    _ => {}
                }
            }
        } else if let Ok((_, old_count, _, new_count, _)) = parse_hunk_header(line) {
            old_remaining = old_count;
            new_remaining = new_count;
        } else if line.starts_with("--- ")
            && lines
                .get(index + 1)
                .is_some_and(|next| grammar_line(*next).starts_with("+++ "))
        {
            starts.push(index);
        }
        index += 1;
    }
    starts
}

fn raw_lines(text: &str) -> Vec<RawLine<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for segment in text.split_inclusive('\n') {
        let raw = segment.strip_suffix('\n').unwrap_or(segment);
        lines.push(RawLine { start, raw });
        start += segment.len();
    }
    if lines.is_empty() {
        lines.push(RawLine { start: 0, raw: "" });
    }
    lines
}

fn grammar_line(line: RawLine<'_>) -> &str {
    line.raw.strip_suffix('\r').unwrap_or(line.raw)
}

fn strip_side_prefix(path: Vec<u8>, prefix: &[u8]) -> Vec<u8> {
    path.strip_prefix(prefix).unwrap_or(&path).to_vec()
}

fn fingerprint(bytes: &[u8]) -> Fingerprint {
    Fingerprint::new(Sha256::digest(bytes).into())
}

fn fingerprint_range(block: &[RawLine<'_>], start: usize, end: usize) -> Fingerprint {
    let bytes = block
        .iter()
        .filter(|line| line.start >= start && line.start < end)
        .flat_map(|line| line.raw.as_bytes().iter().copied().chain([b'\n']))
        .collect::<Vec<_>>();
    fingerprint(&bytes)
}

fn malformed(offset: usize, reason: impl Into<String>) -> PatchError {
    PatchError::Malformed { offset, reason: reason.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ChangesetSource {
        ChangesetSource::Patch { label: None }
    }

    #[test]
    fn parses_text_and_missing_newline_marker() {
        let patch = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n";
        let changeset = parse(std::str::from_utf8(patch).unwrap(), source()).unwrap();
        let FileContent::Text { hunks } = changeset.files()[0].content() else {
            panic!("text patch remains text");
        };
        assert_eq!(hunks[0].lines()[1].missing_newline(), MissingNewline::New);
    }

    #[test]
    fn rejects_a_malformed_hunk_that_unidiff_would_ignore() {
        let patch = "--- a/file.txt\n+++ b/file.txt\n@@ invalid @@\n-old\n+new\n";
        assert!(matches!(parse(patch, source()), Err(PatchError::Malformed { .. })));
    }

    #[test]
    fn rejects_overflowing_hunk_ranges_before_calling_unidiff() {
        let patch = "--- a/file.txt\n+++ b/file.txt\n@@ -18446744073709551615,1 +1 @@\n-old\n+new\n";
        assert!(matches!(parse(patch, source()), Err(PatchError::Malformed { .. })));
    }

    #[test]
    fn rejects_incomplete_or_conflicting_operation_metadata() {
        let incomplete_rename = concat!(
            "diff --git a/old.txt b/new.txt\n",
            "similarity index 90%\n",
            "rename from old.txt\n",
        );
        assert_malformed_reason(
            incomplete_rename,
            "rename metadata requires both source and destination paths",
        );

        let conflicting_operations = concat!(
            "diff --git a/old.txt b/new.txt\n",
            "similarity index 90%\n",
            "rename from old.txt\n",
            "rename to new.txt\n",
            "copy from old.txt\n",
            "copy to new.txt\n",
        );
        assert_malformed_reason(
            conflicting_operations,
            "file has conflicting add, delete, rename, or copy metadata",
        );
    }

    #[test]
    fn rejects_binary_metadata_combined_with_text_hunks() {
        let patch = concat!(
            "diff --git a/file.bin b/file.bin\n",
            "Binary files a/file.bin and b/file.bin differ\n",
            "--- a/file.bin\n",
            "+++ b/file.bin\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
        );
        assert_malformed_reason(patch, "binary file metadata cannot contain text hunks");
    }

    #[test]
    fn decodes_git_octal_paths() {
        let (path, trailing) = parse_path_token("\"a/\\303\\274.txt\" rest", 0).unwrap();
        assert_eq!(path, "a/ü.txt".as_bytes());
        assert_eq!(trailing, " rest");
    }

    #[test]
    fn splits_unquoted_git_paths_that_contain_the_separator_text() {
        let (old, new) = parse_diff_paths("a/dir b/file.txt b/dir b/file.txt", 0).unwrap();
        assert_eq!(old, b"a/dir b/file.txt");
        assert_eq!(new, b"b/dir b/file.txt");
    }

    #[test]
    fn parses_multiple_plain_files_without_mistaking_content_for_headers() {
        let patch = concat!(
            "--- a/one.txt\n",
            "+++ b/one.txt\n",
            "@@ -1 +1 @@\n",
            "--- looks like a header\n",
            "+++ still hunk content\n",
            "--- a/two.txt\n",
            "+++ b/two.txt\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
        );
        let changeset = parse(patch, source()).unwrap();
        assert_eq!(changeset.files().len(), 2);
    }

    fn assert_malformed_reason(patch: &str, expected: &str) {
        let error = parse(patch, source()).expect_err("conflicting metadata is rejected");
        assert_eq!(error, PatchError::Malformed { offset: 0, reason: expected.to_owned() });
    }
}
