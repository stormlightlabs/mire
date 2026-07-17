use std::collections::BTreeSet;

use mire_core::{PatchError, PatchInput, PatchLimits};

const CORPUS: &str = include_str!("fixtures/corpus.txt");
const CRLF_HEX: &str = include_str!("fixtures/patches/crlf.patch.hex");
const INVALID_UTF8_HEX: &str = include_str!("fixtures/patches/invalid_utf8.patch.hex");
const REPOSITORY_CASES: &str = include_str!("fixtures/repositories/cases.txt");

#[test]
fn corpus_covers_every_required_vcs_and_text_case() {
    let actual = corpus_coverage();
    let required = [
        "staged",
        "unstaged",
        "untracked",
        "two_dot",
        "three_dot",
        "commit_diff",
        "add",
        "delete",
        "rename",
        "copy",
        "mode_change",
        "submodule",
        "binary",
        "missing_final_newline",
        "crlf",
        "unicode",
        "invalid_utf8",
        "very_long_line",
        "large_changeset",
        "mixed_language",
        "unknown_extension",
        "shebang",
        "malformed_source",
        "highlighting_fallback",
    ];

    for required_case in required {
        assert!(
            actual.contains(required_case),
            "missing fixture coverage: {required_case}"
        );
    }
}

#[test]
fn byte_sensitive_fixture_contains_invalid_utf8() {
    let bytes = decode_hex_fixture(INVALID_UTF8_HEX);
    assert!(std::str::from_utf8(&bytes).is_err());
    assert!(bytes.windows(b"+\xffnew\n".len()).any(|window| window == b"+\xffnew\n"));
    let invalid_offset = bytes
        .iter()
        .position(|byte| *byte == 0xff)
        .expect("fixture contains its invalid byte");
    let input = PatchInput::new(&bytes, PatchLimits::default()).expect("fixture is below the default size limit");
    assert_eq!(
        input.as_utf8(),
        Err(PatchError::Encoding { encoding: "UTF-8", offset: invalid_offset })
    );
}

#[test]
fn byte_sensitive_fixture_contains_crlf_content() {
    let bytes = decode_hex_fixture(CRLF_HEX);
    assert!(bytes.windows(b"-old\r\n".len()).any(|window| window == b"-old\r\n"));
    assert!(bytes.windows(b"+new\r\n".len()).any(|window| window == b"+new\r\n"));
}

#[test]
fn every_corpus_input_is_checked_in() {
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for line in data_lines(CORPUS) {
        let relative_path = line.split('|').nth(1).expect("corpus row has an input path");
        let path = fixture_root.join(relative_path);
        assert!(path.is_file(), "missing corpus input: {}", path.display());
    }
}

#[test]
fn repository_fixture_declares_each_git_source_case() {
    let cases = data_lines(REPOSITORY_CASES)
        .map(|line| line.split_once('|').expect("repository case has a delimiter").0)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        cases,
        BTreeSet::from(["commit_diff", "staged", "three_dot", "two_dot", "unstaged", "untracked"])
    );
}

#[test]
fn invalid_and_oversized_cases_have_stable_expected_errors() {
    let expectations = data_lines(CORPUS)
        .map(|line| {
            let columns = line.split('|').collect::<Vec<_>>();
            (columns[0], columns[2])
        })
        .collect::<BTreeSet<_>>();
    assert!(expectations.contains(&("malformed", "malformed_error")));
    assert!(expectations.contains(&("invalid_utf8", "encoding_error")));
    assert!(expectations.contains(&("large_changeset", "input_too_large")));

    let error = PatchInput::new(b"too large", PatchLimits { max_bytes: 1 })
        .expect_err("oversized fixture is rejected before parsing");
    assert_eq!(error, PatchError::InputTooLarge { actual: 9, limit: 1 });

    let encoding_error = PatchError::Encoding { encoding: "UTF-8", offset: 77 };
    assert_eq!(encoding_error.to_string(), "patch is not valid UTF-8 near byte 77");
}

fn corpus_coverage() -> BTreeSet<&'static str> {
    data_lines(CORPUS)
        .flat_map(|line| line.split('|').nth(3).expect("corpus row has coverage").split(','))
        .collect()
}

fn data_lines(input: &str) -> impl Iterator<Item = &str> {
    input.lines().filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn decode_hex_fixture(input: &str) -> Vec<u8> {
    let encoded = data_lines(input).collect::<String>();
    assert_eq!(encoded.len() % 2, 0, "hex fixture has complete byte pairs");
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("hex uses ASCII");
            u8::from_str_radix(pair, 16).expect("fixture contains valid hex")
        })
        .collect()
}
