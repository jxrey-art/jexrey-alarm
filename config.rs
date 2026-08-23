//! Local persistence. Alarms are saved as a small JSON document
//! (`alarms.json`) inside the user's per-app config directory
//! (`%APPDATA%\JexreyAlarmSystem\alarms.json` on Windows), so the app keeps
//! working (and writing) even when it is installed somewhere read-only like
//! `Program Files`.

use crate::alarm::Alarm;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

const CONFIG_FILE_NAME: &str = "alarms.json";
const APP_DIR_NAME: &str = "JexreyAlarmSystem";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    /// Schema version, bumped only if we ever need a breaking migration.
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub alarms: Vec<Alarm>,
}

fn default_version() -> u32 {
    1
}

pub fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    base.join(APP_DIR_NAME).join(CONFIG_FILE_NAME)
}

/// Load the config file. Missing file -> empty config (first run).
/// Corrupted file -> the corrupt file is backed up next to itself with a
/// `.corrupt` suffix and an empty config is returned, rather than crashing
/// the whole application on a bad write or manual edit.
pub fn load() -> ConfigFile {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<ConfigFile>(&text) {
            Ok(cfg) => cfg,
            Err(_) => {
                let backup = path.with_extension("json.corrupt");
                let _ = fs::copy(&path, &backup);
                ConfigFile::default()
            }
        },
        Err(_) => ConfigFile::default(),
    }
}

pub fn save(cfg: &ConfigFile) -> io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg).unwrap_or_default();
    // Write to a temp file then rename, so a crash mid-write can never leave
    // a half-written, corrupt alarms.json behind.
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}
