use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use mire_core::DEFAULT_MAX_PATCH_BYTES;
use serde_json::Value;

const CRLF_HEX: &str = include_str!("../../core/tests/fixtures/patches/crlf.patch.hex");
const INVALID_UTF8_HEX: &str = include_str!("../../core/tests/fixtures/patches/invalid_utf8.patch.hex");

static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn file_and_stdin_inputs_produce_identical_canonical_json() {
    for name in [
        "empty.patch",
        "git_metadata.patch",
        "text_edges.patch",
        "mixed_languages.patch",
    ] {
        let path = fixture_path(name);
        let bytes = fs::read(&path).expect("fixture can be read");
        let from_file = run_file(&path);
        let from_stdin = run_stdin(&bytes);
        assert!(
            from_file.status.success(),
            "file input failed for {name}: {}",
            stderr(&from_file)
        );
        assert!(
            from_stdin.status.success(),
            "stdin input failed for {name}: {}",
            stderr(&from_stdin)
        );
        assert_eq!(from_file.stdout, from_stdin.stdout, "canonical JSON differs for {name}");
    }
}

#[test]
fn patch_fixture_matrix_matches_golden_normalization_summaries() {
    let cases = [
        ("empty.patch", ""),
        (
            "git_metadata.patch",
            "added:added.txt,copied:copy.txt,deleted:deleted.txt,modified:image.bin,renamed:new-name.txt,modified:script.sh,modified:vendor/library",
        ),
        ("text_edges.patch", "modified:ünicode.txt"),
        (
            "mixed_languages.patch",
            "modified:data/example.unknown,modified:src/lib.rs,modified:tools/run,modified:web/app.ts",
        ),
    ];
    for (name, golden) in cases {
        let output = run_file(&fixture_path(name));
        assert!(output.status.success(), "{name}: {}", stderr(&output));
        assert_eq!(summary(&output.stdout), golden, "golden summary changed for {name}");
    }

    let crlf = run_stdin(&decode_hex(CRLF_HEX));
    assert!(crlf.status.success(), "crlf fixture: {}", stderr(&crlf));
    assert_eq!(summary(&crlf.stdout), "modified:crlf.txt");
    let json: Value = serde_json::from_slice(&crlf.stdout).expect("output is JSON");
    let content = &json["files"][0]["content"]["hunks"][0]["lines"][0]["content"];
    assert_eq!(
        content
            .as_array()
            .and_then(|bytes| bytes.last())
            .and_then(Value::as_u64),
        Some(13)
    );
}

#[test]
fn malformed_encoding_limit_and_io_failures_have_stable_exits() {
    let malformed = run_file(&fixture_path("malformed.patch"));
    assert_eq!(malformed.status.code(), Some(4));
    assert!(stderr(&malformed).contains("cannot parse patch: invalid patch"));

    let invalid_utf8 = run_stdin(&decode_hex(INVALID_UTF8_HEX));
    assert_eq!(invalid_utf8.status.code(), Some(4));
    assert!(stderr(&invalid_utf8).contains("patch is not valid UTF-8 near byte"));

    let missing = run_file(Path::new("this-patch-does-not-exist.patch"));
    assert_eq!(missing.status.code(), Some(3));
    assert!(stderr(&missing).contains("cannot read patch from"));

    let large_path = temp_path("oversized.patch");
    let large_file = File::create(&large_path).expect("temporary fixture can be created");
    large_file
        .set_len(DEFAULT_MAX_PATCH_BYTES as u64 + 1)
        .expect("temporary fixture can be made sparse");
    let oversized = run_file(&large_path);
    fs::remove_file(&large_path).expect("temporary fixture can be removed");
    assert_eq!(oversized.status.code(), Some(4));
    assert!(stderr(&oversized).contains("limit is 67108864 bytes"));
}

#[test]
fn long_lines_and_argument_like_content_remain_patch_data() {
    let long_content = "x".repeat(1024 * 1024);
    let long_patch = format!("--- /dev/null\n+++ b/long.txt\n@@ -0,0 +1 @@\n+{long_content}\n");
    let long_output = run_stdin(long_patch.as_bytes());
    assert!(long_output.status.success(), "long line: {}", stderr(&long_output));
    let long_json: Value = serde_json::from_slice(&long_output.stdout).expect("output is JSON");
    assert_eq!(
        long_json["files"][0]["content"]["hunks"][0]["lines"][0]["content"]
            .as_array()
            .map(Vec::len),
        Some(1024 * 1024)
    );

    let instructions = b"--- /dev/null\n+++ b/safe.txt\n@@ -0,0 +1 @@\n+/etc/passwd --format json; echo unsafe\n";
    let output = run_stdin(instructions);
    assert!(output.status.success(), "argument-like content: {}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).expect("output is JSON");
    let bytes = json["files"][0]["content"]["hunks"][0]["lines"][0]["content"]
        .as_array()
        .expect("line content is a byte array")
        .iter()
        .map(|byte| byte.as_u64().expect("content byte") as u8)
        .collect::<Vec<_>>();
    assert_eq!(bytes, b"/etc/passwd --format json; echo unsafe");
}

#[test]
fn invalid_formats_have_a_stable_exit_and_actionable_help() {
    let output = Command::new(binary())
        .args(["patch", "-", "--format", "yaml"])
        .output()
        .expect("mire runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("invalid value 'yaml'"));
    assert!(stderr(&output).contains("possible values: json"));
    assert!(stderr(&output).contains("try '--help'"));
}

#[test]
fn theme_selection_does_not_change_structured_output() {
    let path = fixture_path("mixed_languages.patch");
    let expected = run_file(&path);
    assert!(expected.status.success(), "baseline: {}", stderr(&expected));

    for theme in ["auto", "iceberg", "eldritch", "catppuccin"] {
        let before = Command::new(binary())
            .args(["--theme", theme, "patch"])
            .arg(&path)
            .args(["--format", "json"])
            .output()
            .expect("mire runs");
        assert!(before.status.success(), "{theme} before command: {}", stderr(&before));
        assert_eq!(before.stdout, expected.stdout, "{theme} changed JSON output");

        let after = Command::new(binary())
            .arg("patch")
            .arg(&path)
            .args(["--format", "json", "--theme", theme])
            .output()
            .expect("mire runs");
        assert!(after.status.success(), "{theme} after command: {}", stderr(&after));
        assert_eq!(after.stdout, expected.stdout, "{theme} changed JSON output");
    }
}

#[test]
fn invalid_themes_list_every_allowed_identifier() {
    let output = Command::new(binary())
        .args(["patch", "-", "--theme", "dracula"])
        .output()
        .expect("mire runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("invalid value 'dracula'"));
    assert!(stderr(&output).contains("possible values: auto, iceberg, eldritch, catppuccin"));
}

fn run_file(path: &Path) -> Output {
    Command::new(binary())
        .arg("patch")
        .arg(path)
        .args(["--format", "json"])
        .output()
        .expect("mire runs")
}

fn run_stdin(bytes: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .args(["patch", "-", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mire starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(bytes)
        .expect("fixture can be written to stdin");
    child.wait_with_output().expect("mire exits")
}

fn summary(output: &[u8]) -> String {
    let json: Value = serde_json::from_slice(output).expect("output is JSON");
    json["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|file| {
            let status = file["status"].as_str().expect("status is text");
            let path = file["new"]["path"]
                .as_array()
                .or_else(|| file["old"]["path"].as_array())
                .expect("one side has a path");
            let path = path
                .iter()
                .map(|byte| byte.as_u64().expect("path byte") as u8)
                .collect::<Vec<_>>();
            format!("{status}:{}", String::from_utf8_lossy(&path))
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/patches")
        .join(name)
}

fn temp_path(name: &str) -> PathBuf {
    let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("mire-{}-{id}-{name}", std::process::id()))
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn decode_hex(input: &str) -> Vec<u8> {
    let encoded = input.lines().filter(|line| !line.starts_with('#')).collect::<String>();
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("hex is ASCII"), 16).expect("valid hex"))
        .collect()
}
