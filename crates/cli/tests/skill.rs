use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const BUNDLED_SKILL: &str = include_str!("../skills/mire/SKILL.md");

#[test]
fn skill_path_installs_and_points_to_the_shipped_skill() {
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!("mire-skill-home-{}-{id}", std::process::id()));

    let output = Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(["skill", "path"])
        .env("XDG_DATA_HOME", directory.join("xdg-data"))
        .env("HOME", &directory)
        .output()
        .expect("mire runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(path, directory.join(".agents/skills/mire/SKILL.md"));
    assert_eq!(fs::read_to_string(&path).unwrap(), BUNDLED_SKILL);

    fs::remove_dir_all(directory).unwrap();
}
