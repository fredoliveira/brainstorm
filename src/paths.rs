//! Where Brainstorm keeps its files. `BRAINSTORM_HOME` moves all of it under one directory,
//! for trying things without touching the real settings and data.

use std::path::PathBuf;

const APP: &str = "brainstorm";

fn dir(home: &str, platform: Option<PathBuf>) -> PathBuf {
    match std::env::var_os("BRAINSTORM_HOME") {
        Some(root) => PathBuf::from(root).join(home),
        None => platform.unwrap_or_else(std::env::temp_dir).join(APP),
    }
}

/// `settings.toml` lives here (`~/Library/Application Support/brainstorm` on macOS).
pub fn config_dir() -> PathBuf {
    dir("config", dirs::config_dir())
}

/// The archive and the checkpoints: what must survive.
pub fn data_dir() -> PathBuf {
    dir("data", dirs::data_dir())
}

/// Card data and images from 17Lands: all of it can be fetched again.
pub fn cache_dir() -> PathBuf {
    dir("cache", dirs::cache_dir())
}

/// Where drafts and games are saved, one JSONL file per day (see `recall-core`'s archive).
pub fn archive_dir() -> PathBuf {
    data_dir().join("archive")
}
