use std::collections::BTreeSet;

const CORPUS: &str = include_str!("../../core/tests/fixtures/corpus.txt");

#[test]
fn core_fixture_corpus_is_available_to_binary_tests() {
    let cases = CORPUS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('|').expect("corpus row has a delimiter").0)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        cases,
        BTreeSet::from([
            "crlf_bytes",
            "empty",
            "git_metadata",
            "invalid_utf8",
            "large_changeset",
            "long_line",
            "malformed",
            "mixed_languages",
            "repository_states",
            "text_edges",
        ])
    );
}
