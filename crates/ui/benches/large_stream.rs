use divan::{Bencher, black_box};
use mire_core::{ChangesetSource, PatchLimits, parse_patch};
use mire_tui::{App, Theme, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const FILE_COUNT: usize = 200;
const LINES_PER_FILE: usize = 50;

fn main() {
    divan::main();
}

#[divan::bench(sample_count = 50, max_time = 2)]
fn parse_large_patch(bencher: Bencher) {
    let patch = large_patch();
    bencher.bench_local(|| {
        let changeset = parse_patch(
            black_box(patch.as_bytes()),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .expect("generated benchmark patch is valid");
        black_box(changeset.files().len());
    });
}

#[divan::bench(sample_count = 50, max_time = 2)]
fn build_review_stream(bencher: Bencher) {
    let patch = large_patch();
    let changeset = parse_patch(
        patch.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("generated benchmark patch is valid");
    bencher.bench_local(|| {
        let mut app = App::ready(black_box(&changeset));
        app.resize(120, 40);
        black_box(app.scroll());
    });
}

#[divan::bench(sample_count = 100, max_time = 2)]
fn render_frame(bencher: Bencher) {
    let patch = large_patch();
    let changeset = parse_patch(
        patch.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("generated benchmark patch is valid");
    let mut app = App::ready(&changeset);
    app.resize(120, 40);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal is available");
    bencher.bench_local(|| {
        terminal
            .draw(|frame| render(frame, black_box(&app), &Theme::dark()))
            .expect("benchmark frame renders");
    });
}

fn large_patch() -> String {
    let mut patch = String::new();
    for file in 0..FILE_COUNT {
        patch.push_str(&format!(
            "diff --git a/src/file_{file}.rs b/src/file_{file}.rs\n--- a/src/file_{file}.rs\n+++ b/src/file_{file}.rs\n@@ -1,{LINES_PER_FILE} +1,{LINES_PER_FILE} @@\n"
        ));
        for line in 0..LINES_PER_FILE / 2 {
            patch.push_str(&format!(" context_{line}\n-old_{file}_{line}\n+new_{file}_{line}\n"));
        }
    }
    patch
}
