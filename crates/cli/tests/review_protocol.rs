use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::{read_review, write_review_atomic};
use mire_core::{
    Anchor, AnchorSide, Author, BytePath, ChangesetSource, LineNumber, LineRange, NoteId, NoteSeverity, NoteStatus,
    PatchLimits, Provenance, Review, ReviewNote, ReviewRevision, parse_patch,
};
use serde_json::{Value, json};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] = b"--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old value\n+new value\n context\n";

#[test]
fn context_defaults_to_a_compact_manifest_and_bounds_expanded_requests() {
    let fixture = Fixture::new();

    let manifest = mire(&["context", fixture.review_str(), "--format", "json"]);
    assert_success(&manifest);
    let manifest: Value = serde_json::from_slice(&manifest.stdout).expect("manifest is JSON");
    assert_eq!(manifest["schema_version"]["major"], 1);
    assert_eq!(manifest["payload"]["kind"], "manifest");
    assert!(manifest["payload"]["files"][0]["hunks"][0]["fingerprint"].is_string());
    assert!(manifest["payload"]["files"][0]["hunks"][0].get("lines").is_none());

    let missing_bound = mire(&["context", fixture.review_str(), "--patch"]);
    assert_eq!(missing_bound.status.code(), Some(2));
    assert!(stderr(&missing_bound).contains("--max-bytes"));

    let too_small = mire(&["context", fixture.review_str(), "--patch", "--max-bytes", "16"]);
    assert_eq!(too_small.status.code(), Some(8));
    assert!(stderr(&too_small).contains("requested limit is 16 bytes"));

    let patch = mire(&["context", fixture.review_str(), "--patch", "--max-bytes", "65536"]);
    assert_success(&patch);
    let patch: Value = serde_json::from_slice(&patch.stdout).expect("patch context is JSON");
    assert_eq!(patch["payload"]["kind"], "patch");
    assert_eq!(patch["payload"]["changeset"]["files"].as_array().unwrap().len(), 1);

    let expanded = mire(&[
        "context",
        fixture.review_str(),
        "--file",
        "src/lib.rs",
        "--max-bytes",
        "65536",
    ]);
    assert_success(&expanded);
    let expanded: Value = serde_json::from_slice(&expanded.stdout).expect("expanded context is JSON");
    assert_eq!(expanded["payload"]["kind"], "file");
    assert_eq!(
        expanded["payload"]["file"]["content"]["hunks"][0]["lines"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn mixed_provenance_round_trips_through_json_and_markdown() {
    let fixture = Fixture::new();
    let batch = fixture.batch(&[
        note(&fixture.review, "human", Provenance::Human),
        note(
            &fixture.review,
            "agent",
            Provenance::Agent { producer: "codex".to_owned() },
        ),
        note(
            &fixture.review,
            "analyzer",
            Provenance::Analyzer { producer: "clippy".to_owned() },
        ),
        note(
            &fixture.review,
            "interchange",
            Provenance::Interchange { format: "external-json".to_owned(), producer: Some("scanner".to_owned()) },
        ),
    ]);
    fs::write(&fixture.batch_path, batch).expect("batch fixture can be written");

    let imported = mire(&["notes", "import", fixture.review_str(), fixture.batch_str()]);
    assert_success(&imported);
    let result: Value = serde_json::from_slice(&imported.stdout).expect("import result is JSON");
    assert_eq!(result["status"], "imported");
    assert_eq!(result["imported"], 4);
    assert_eq!(result["review_revision"], 2);

    let listed = mire(&["notes", "list", fixture.review_str(), "--format", "json"]);
    assert_success(&listed);
    let exported = mire(&["notes", "export", fixture.review_str(), "--format", "json"]);
    assert_success(&exported);
    assert_eq!(listed.stdout, exported.stdout, "JSON commands must be deterministic");
    let document: Value = serde_json::from_slice(&listed.stdout).expect("note listing is JSON");
    let kinds = document["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["provenance"]["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["agent", "analyzer", "human", "interchange"]);

    let round_trip = Fixture::new();
    let reimported = mire_with_stdin(&["notes", "import", round_trip.review_str(), "-"], &exported.stdout);
    assert_success(&reimported);
    assert_eq!(read_review(&round_trip.review_path).unwrap().notes().len(), 4);

    let markdown = mire(&["notes", "export", fixture.review_str(), "--format", "markdown"]);
    assert_success(&markdown);
    let markdown = String::from_utf8(markdown.stdout).expect("Markdown is UTF-8");
    assert!(markdown.contains("# Mire review notes"));
    assert!(markdown.contains("agent (codex)"));
    assert!(markdown.contains("analyzer (clippy)"));
    assert!(markdown.contains("external-json interchange (scanner)"));
    assert!(markdown.contains("`src/lib.rs` (new side, lines 1–1)"));
}

#[test]
fn rejected_batches_report_every_invalid_anchor_without_rewriting_the_review() {
    let fixture = Fixture::new();
    let mut first = serde_json::to_value(note(&fixture.review, "bad-a", Provenance::Human)).unwrap();
    let mut second = serde_json::to_value(note(&fixture.review, "bad-b", Provenance::Human)).unwrap();
    first["anchor"]["content_fingerprint"] = Value::String("00".repeat(32));
    second["anchor"]["content_fingerprint"] = Value::String("11".repeat(32));
    fs::write(
        &fixture.batch_path,
        serde_json::to_vec(&json!({
            "schema_version": { "major": 1, "minor": 0 },
            "notes": [first, second],
        }))
        .unwrap(),
    )
    .expect("invalid batch fixture can be written");
    let before = fs::read(&fixture.review_path).expect("review can be read before import");

    let output = mire(&["notes", "import", fixture.review_str(), fixture.batch_str()]);
    assert_eq!(output.status.code(), Some(8));
    assert!(output.stdout.is_empty());
    let report: Value = serde_json::from_slice(&output.stderr).expect("rejection is structured JSON");
    assert_eq!(report["status"], "rejected");
    assert_eq!(report["failures"].as_array().unwrap().len(), 2);
    assert_eq!(report["failures"][0]["note_id"], "bad-a");
    assert_eq!(report["failures"][0]["code"], "content_fingerprint_mismatch");
    assert_eq!(report["failures"][1]["note_id"], "bad-b");
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before);
    assert_eq!(read_review(&fixture.review_path).unwrap().revision().get(), 1);
}

struct Fixture {
    directory: PathBuf,
    review_path: PathBuf,
    batch_path: PathBuf,
    review: Review,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-review-protocol-{}-{id}", std::process::id()));
        fs::create_dir(&directory).expect("fixture directory can be created");
        let review_path = directory.join("review.json");
        let batch_path = directory.join("batch.json");
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default())
            .expect("fixture patch parses");
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
        write_review_atomic(&review_path, &review).expect("fixture review can be written");
        Self { directory, review_path, batch_path, review }
    }

    fn batch(&self, notes: &[ReviewNote]) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": { "major": 1, "minor": 0 },
            "notes": notes,
        }))
        .expect("batch serializes")
    }

    fn review_str(&self) -> &str {
        self.review_path.to_str().expect("fixture path is UTF-8")
    }

    fn batch_str(&self) -> &str {
        self.batch_path.to_str().expect("fixture path is UTF-8")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).expect("fixture directory can be removed");
    }
}

fn note(review: &Review, id: &str, provenance: Provenance) -> ReviewNote {
    let changeset = review.changeset();
    let hunk = match changeset.files()[0].content() {
        mire_core::FileContent::Text { hunks } => &hunks[0],
        mire_core::FileContent::Binary => panic!("fixture is text"),
    };
    let line = LineNumber::new(1).unwrap();
    let anchor = Anchor::new(
        changeset,
        BytePath::new(b"src/lib.rs".to_vec()).unwrap(),
        AnchorSide::New,
        LineRange::new(line, line).unwrap(),
        hunk.fingerprint(),
    )
    .unwrap();
    ReviewNote::new(
        NoteId::new(id).unwrap(),
        anchor,
        Author::new(format!("author-{id}"), Some(format!("Author {id}"))).unwrap(),
        NoteSeverity::Medium,
        NoteStatus::Open,
        format!("Finding from {id}."),
        provenance,
    )
    .unwrap()
}

fn mire(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(args)
        .output()
        .expect("mire runs")
}

fn mire_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mire starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input)
        .expect("stdin can be written");
    child.wait_with_output().expect("mire exits")
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "mire failed: {}", stderr(output));
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
