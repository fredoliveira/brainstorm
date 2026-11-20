//! The settings file: `<config_dir>/settings.toml`. Written whole on every change, readable
//! by its owner only since it holds the 17Lands token.

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cards::Group;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The 17Lands client token. Without one nothing is uploaded.
    pub token: Option<String>,
    /// Keep every draft and game in the archive on this Mac.
    pub save: bool,
    /// Show the pack's ratings while drafting.
    pub draft_overlay: bool,
    /// Show what is left in the library while playing.
    pub deck_overlay: bool,
    /// Draw the overlays at 80%.
    pub compact: bool,
    /// Whose games the win rates come from.
    pub group: Group,
}

impl Default for Settings {
    fn default() -> Self {
        Self { token: None, save: true, draft_overlay: true, deck_overlay: true, compact: false, group: Group::All }
    }
}

fn file() -> PathBuf {
    paths::config_dir().join("settings.toml")
}

impl Settings {
    /// The saved settings, and whether there were none yet (a first run). A file that won't
    /// parse counts as none, and is left alone until something is saved.
    pub fn load() -> (Self, bool) {
        match std::fs::read_to_string(file()) {
            Ok(text) => match toml::from_str(&text) {
                Ok(settings) => (settings, false),
                Err(err) => {
                    log::warn!(target: "brainstorm", "Ignoring unreadable settings: {err}");
                    (Self::default(), false)
                }
            },
            Err(_) => (Self::default(), true),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let path = file();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        // Written beside the file and moved over it, so a crash never leaves half a file.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(tmp, path)
    }

    /// The last four characters of the token: enough to tell which one is saved.
    pub fn token_hint(&self) -> Option<String> {
        let token = self.token.as_ref()?;
        Some(token[token.len().saturating_sub(4)..].to_string())
    }
}
