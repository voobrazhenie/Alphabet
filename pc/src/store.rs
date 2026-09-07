//! Saved settings, named templates and the six viewpoints — on this computer only.
//!
//! The page keeps a browser copy and a Firestore document; the native build keeps
//! neither. Two JSON files under the user's config directory
//! (`%APPDATA%\CorticalFlythrough\` on Windows) are the whole store, and nothing
//! here touches the network. Every failure is reported as text the console can show
//! and is never fatal.

use crate::state::State;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const SETTINGS_FILE: &str = "settings.json";
pub const TEMPLATES_FILE: &str = "templates.json";
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Saved {
    pub version: u32,
    pub saved_at: u64,
    pub data: State,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Template {
    pub name: String,
    pub saved_at: u64,
    pub data: State,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `%APPDATA%\CorticalFlythrough` on Windows, `$XDG_CONFIG_HOME/cortical-flythrough`
/// (or `~/.config/...`) elsewhere. Hand-rolled so the app carries no extra crate for
/// two environment variables.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("CorticalFlythrough"))
    }
    #[cfg(not(windows))]
    {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(x).join("cortical-flythrough"));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/cortical-flythrough"))
    }
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(SETTINGS_FILE))
}

pub fn templates_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(TEMPLATES_FILE))
}

/// Write through a temporary file and rename, so a crash mid-write cannot leave a
/// half-written settings file behind.
fn write_atomic(path: &PathBuf, text: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    // Windows will not rename onto an existing file
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load_settings() -> Option<State> {
    let path = settings_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let saved: Saved = serde_json::from_str(&text).ok()?;
    Some(saved.data)
}

pub fn save_settings(st: &State) -> Result<PathBuf, String> {
    let path = settings_path().ok_or("no config directory")?;
    let saved = Saved { version: VERSION, saved_at: now_ms(), data: st.clone() };
    let text = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    write_atomic(&path, &text)?;
    Ok(path)
}

pub fn clear_settings() -> Result<(), String> {
    let path = settings_path().ok_or("no config directory")?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn load_templates() -> Vec<Template> {
    let Some(path) = templates_path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_templates(list: &[Template]) -> Result<(), String> {
    let path = templates_path().ok_or("no config directory")?;
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    write_atomic(&path, &text)
}

/// Save the report next to the settings, so a run can be kept without the clipboard.
pub fn save_report(text: &str) -> Result<PathBuf, String> {
    let dir = config_dir().ok_or("no config directory")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("benchmark-{}.txt", now_ms()));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}
