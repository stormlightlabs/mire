use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

const SKILL_CONTENT: &str = include_str!("../skills/mire/SKILL.md");

/// Failures while locating or installing Mire's bundled agent skill.
#[derive(Debug, Error)]
pub enum SkillError {
    /// The user's home directory is unavailable.
    #[error("cannot locate the user skill directory: HOME is not set")]
    HomeUnavailable,
    /// The bundled skill could not be created or refreshed.
    #[error("cannot install bundled skill at {path:?}: {source}")]
    Install { path: PathBuf, source: io::Error },
}

/// Installs the bundled skill in the user data directory and returns its path.
pub fn installed_path() -> Result<PathBuf, SkillError> {
    let path = default_path()?;
    if fs::read(&path).ok().as_deref() == Some(SKILL_CONTENT.as_bytes()) {
        return Ok(path);
    }

    let parent = path.parent().expect("the skill path always has a parent");
    fs::create_dir_all(parent).map_err(|source| SkillError::Install { path: path.clone(), source })?;
    fs::write(&path, SKILL_CONTENT).map_err(|source| SkillError::Install { path: path.clone(), source })?;
    Ok(path)
}

fn default_path() -> Result<PathBuf, SkillError> {
    nonempty_env("HOME")
        .map(|home| home.join(".agents/skills/mire/SKILL.md"))
        .ok_or(SkillError::HomeUnavailable)
}

fn nonempty_env(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}
