//! Splits mbox patch series, as `git format-patch --stdout` and
//! `gh pr diff --patch` print them, into one diff per commit.

/// One commit's diff and its byte offset in the series.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitDiff<'a> {
    /// Byte offset of the diff within the whole input.
    pub offset: usize,
    /// The diff text, from its first `diff --git` line to its last hunk line.
    pub text: &'a str,
}

/// Returns whether the input starts with an mbox commit header.
pub fn is_mbox(text: &str) -> bool {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(is_commit_start)
}

/// Returns each commit's diff in series order, skipping commits without one.
///
/// A commit's headers and message end at its first `---` line. Its diff starts
/// at the next `diff --git` line and ends before the `-- ` signature that
/// `git format-patch` appends, or at the next commit.
pub fn commit_diffs(text: &str) -> Vec<CommitDiff<'_>> {
    let lines = lines_with_offsets(text);
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, (_, line))| is_commit_start(trim_newline(line)))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    let mut diffs = Vec::new();
    for (position, &start) in starts.iter().enumerate() {
        let end = starts.get(position + 1).copied().unwrap_or(lines.len());
        if let Some(diff) = commit_diff(text, &lines[start..end]) {
            diffs.push(diff);
        }
    }
    diffs
}

fn commit_diff<'a>(text: &'a str, lines: &[(usize, &'a str)]) -> Option<CommitDiff<'a>> {
    let message_end = lines.iter().position(|(_, line)| trim_newline(line) == "---")?;
    let diff_start = message_end
        + lines[message_end..]
            .iter()
            .position(|(_, line)| line.starts_with("diff --git "))?;

    let mut old_remaining = 0_u64;
    let mut new_remaining = 0_u64;
    let mut diff_end = lines.len();
    for (index, (_, raw)) in lines.iter().enumerate().skip(diff_start) {
        let line = trim_newline(raw);
        if old_remaining > 0 || new_remaining > 0 {
            match line.as_bytes().first() {
                Some(b' ') => {
                    old_remaining = old_remaining.saturating_sub(1);
                    new_remaining = new_remaining.saturating_sub(1);
                }
                Some(b'-') => old_remaining = old_remaining.saturating_sub(1),
                Some(b'+') => new_remaining = new_remaining.saturating_sub(1),
                _ => {}
            }
            continue;
        }
        if line == "-- " || line == "--" {
            diff_end = index;
            break;
        }
        if let Some((old_count, new_count)) = hunk_counts(line) {
            old_remaining = old_count;
            new_remaining = new_count;
        }
    }

    let start = lines[diff_start].0;
    let end = match lines.get(diff_end) {
        Some((offset, _)) => *offset,
        None => lines.last().map_or(start, |(offset, line)| offset + line.len()),
    };
    Some(CommitDiff { offset: start, text: &text[start..end] })
}

fn hunk_counts(line: &str) -> Option<(u64, u64)> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    Some((range_count(old)?, range_count(new)?))
}

fn range_count(range: &str) -> Option<u64> {
    match range.split_once(',') {
        Some((_, count)) => count.parse().ok(),
        None => range.parse::<u64>().ok().map(|_| 1),
    }
}

fn is_commit_start(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("From ") else {
        return false;
    };
    let bytes = rest.as_bytes();
    bytes.len() > 41 && bytes[..40].iter().all(u8::is_ascii_hexdigit) && bytes[40] == b' '
}

fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let start = offset;
            offset += line.len();
            (start, line)
        })
        .collect()
}

fn trim_newline(line: &str) -> &str {
    line.strip_suffix('\n')
        .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIES: &str = "From 1111111111111111111111111111111111111111 Mon Sep 17 00:00:00 2001
From: A <a@example.invalid>
Subject: [PATCH 1/2] First

Body that mentions diff --git a/x b/x before the separator.
---
 a.txt | 2 +-
 1 file changed

diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
--\x20
+new
--\x20
2.50.0

From 2222222222222222222222222222222222222222 Mon Sep 17 00:00:00 2001
From: A <a@example.invalid>
Subject: [PATCH 2/2] Empty

---
 0 files changed

From 3333333333333333333333333333333333333333 Mon Sep 17 00:00:00 2001
Subject: [PATCH 3/3] Last
---
diff --git a/b.txt b/b.txt
--- a/b.txt
+++ b/b.txt
@@ -1 +1 @@
-old
+new

From 4444444444444444444444444444444444444444 Mon Sep 17 00:00:00 2001
Subject: [PATCH 4/4] After an unsigned commit
---
diff --git a/c.txt b/c.txt
--- a/c.txt
+++ b/c.txt
@@ -1 +1 @@
-old
+new
";

    #[test]
    fn detects_mbox_by_its_first_commit_header() {
        assert!(is_mbox(SERIES));
        assert!(is_mbox(&format!("\n{SERIES}")));
        assert!(!is_mbox("diff --git a/a b/a\n"));
        assert!(!is_mbox("From someone else\n"));
    }

    #[test]
    fn splits_diffs_and_drops_messages_signatures_and_empty_commits() {
        let diffs = commit_diffs(SERIES);
        assert_eq!(diffs.len(), 3);
        assert!(diffs[0].text.starts_with("diff --git a/a.txt b/a.txt\n"));
        assert!(
            diffs[0].text.ends_with("@@ -1 +1 @@\n-- \n+new\n"),
            "a deleted line that reads `- ` stays in the hunk: {:?}",
            diffs[0].text
        );
        assert_eq!(&SERIES[diffs[0].offset..diffs[0].offset + 10], "diff --git");
        assert!(diffs[1].text.starts_with("diff --git a/b.txt b/b.txt\n"));
        assert!(
            diffs[1].text.ends_with("+new\n\n") && !diffs[1].text.contains("From 4444"),
            "a commit without a signature ends where the next commit starts: {:?}",
            diffs[1].text
        );
        assert!(diffs[2].text.starts_with("diff --git a/c.txt b/c.txt\n"));
    }
}
