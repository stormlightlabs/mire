use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
fn terminal_review_enters_renders_and_restores_the_alternate_screen() {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .expect("a pseudo-terminal can be allocated");
    let mut command = CommandBuilder::new(binary());
    command.arg("patch");
    command.arg(fixture_path("text_edges.patch"));
    command.env("NO_COLOR", "1");

    let mut reader = pair.master.try_clone_reader().expect("PTY output can be read");
    let reader_thread = thread::spawn(move || {
        let mut output = Vec::new();
        reader.read_to_end(&mut output).expect("PTY output can be captured");
        output
    });
    let mut writer = pair.master.take_writer().expect("PTY input can be written");
    let mut child = pair.slave.spawn_command(command).expect("Mire can start in the PTY");
    drop(pair.slave);

    thread::sleep(Duration::from_millis(100));
    writer.write_all(b"2").expect("split-layout key can be sent");
    writer.flush().expect("split-layout key can be flushed");
    thread::sleep(Duration::from_millis(50));
    writer.write_all(b"q").expect("quit key can be sent");
    writer.flush().expect("quit key can be flushed");
    let status = child.wait().expect("terminal review exits");
    drop(writer);
    drop(pair.master);
    let output = reader_thread.join().expect("PTY reader does not panic");

    assert!(status.success(), "PTY review failed with {status:?}");
    assert!(contains(&output, b"\x1b[?1049h"), "alternate screen was not entered");
    assert!(contains(&output, b"\x1b[?1049l"), "alternate screen was not restored");
    assert!(contains(&output, b"Mire review"), "review frame was not rendered");
    assert!(
        contains(&output, b"split"),
        "split layout was not rendered after its key binding"
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/patches")
        .join(name)
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}
