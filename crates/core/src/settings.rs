// SPDX-License-Identifier: GPL-3.0-or-later

//! What the user has decided, kept between runs.
//!
//! Deliberately small: only the things that would be irritating to set again
//! every time. Anything the application can work out for itself is worked out
//! rather than stored.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::encode::Format;
use crate::naming::template;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Where rips are written. Empty means the system music folder, which is
    /// resolved by the layer that knows about the system.
    #[serde(default)]
    pub output_root: Option<PathBuf>,

    /// Pattern for folders and file name together, `/` separating them.
    #[serde(default = "default_pattern")]
    pub pattern: String,

    /// What a rip is written as. Several at once is normal: the disc is read
    /// once and every format comes out of that one read.
    #[serde(default = "default_formats")]
    pub formats: Vec<Format>,

    /// Bit rate in kbps for the formats where that means anything. Absent
    /// entries fall back to the format's own default.
    #[serde(default)]
    pub bitrates: std::collections::HashMap<Format, u32>,

    /// Read offset of the drive in samples, EAC convention, per drive so that
    /// swapping drives does not silently ruin a rip.
    #[serde(default)]
    pub drive_offsets: std::collections::HashMap<String, i32>,
}

fn default_pattern() -> String {
    template::DEFAULT.to_owned()
}

fn default_formats() -> Vec<Format> {
    vec![Format::default()]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_root: None,
            pattern: default_pattern(),
            formats: default_formats(),
            bitrates: std::collections::HashMap::new(),
            drive_offsets: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    /// Reads what was saved, falling back to the defaults for anything missing
    /// or unreadable. Broken settings should never stop the application.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|body| serde_json::from_str(&body).ok())
            .unwrap_or_default()
    }

    /// Writes through a temporary file, so an interrupted save leaves the
    /// previous settings rather than half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let body = serde_json::to_string_pretty(self)?;
        let temporary = path.with_extension("json.new");

        fs::write(&temporary, body)?;
        fs::rename(&temporary, path)
    }

    /// Offset for a drive, which is a property of the model rather than of the
    /// machine, and zero until somebody calibrates it.
    pub fn drive_offset(&self, drive: &str) -> i32 {
        self.drive_offsets.get(drive).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "toccata-settings-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[test]
    fn starts_from_a_pattern_that_already_works() {
        let settings = Settings::default();
        assert_eq!(settings.pattern, template::DEFAULT);
        assert_eq!(settings.output_root, None);
        assert_eq!(settings.drive_offset(r"\\.\E:"), 0);
    }

    #[test]
    fn what_was_saved_comes_back() {
        let path = temporary();
        let mut settings = Settings {
            pattern: "{album}/{title}".to_owned(),
            output_root: Some(PathBuf::from("D:/Music")),
            ..Default::default()
        };
        settings.drive_offsets.insert(r"\\.\E:".to_owned(), 6);

        settings.save(&path).expect("settings can be written");
        let loaded = Settings::load(&path);

        assert_eq!(loaded.pattern, "{album}/{title}");
        assert_eq!(loaded.output_root, Some(PathBuf::from("D:/Music")));
        assert_eq!(loaded.drive_offset(r"\\.\E:"), 6);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn nothing_saved_yet_is_not_an_error() {
        assert_eq!(Settings::load(&temporary()).pattern, template::DEFAULT);
    }

    // Settings written by a future version, or corrupted, must not stop a rip.
    #[test]
    fn unreadable_settings_fall_back_to_the_defaults() {
        let path = temporary();
        fs::write(&path, "{ not json at all").expect("the file can be written");

        assert_eq!(Settings::load(&path).pattern, template::DEFAULT);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_pattern_saved_without_the_newer_fields_still_loads() {
        let path = temporary();
        fs::write(&path, r#"{"pattern":"{album}/{title}"}"#).expect("the file can be written");

        let loaded = Settings::load(&path);
        assert_eq!(loaded.pattern, "{album}/{title}");
        assert_eq!(loaded.output_root, None);

        let _ = fs::remove_file(path);
    }
}
