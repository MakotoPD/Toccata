// SPDX-License-Identifier: GPL-3.0-or-later

//! Access to optical drives.
//!
//! This is the only module in the project allowed to branch on the target
//! operating system. Everything above it works against [`Drive`] and never
//! learns which platform it is running on.

use serde::Serialize;

use crate::toc::{Toc, TocError};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

/// A drive the user can pick from. `id` is what gets passed back to
/// [`open`], and it stays stable for as long as the device is attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub id: String,
    /// Vendor and model where the system reports them, the device path
    /// otherwise. Never translated: it names a piece of hardware.
    pub name: String,
}

/// Errors carry a code and the values needed to describe what went wrong.
/// They are never prose: the frontend owns every sentence the user reads.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DriveError {
    #[error("no drive with id {device}")]
    NotFound { device: String },

    /// On Linux this almost always means the user is missing from the group
    /// that owns the device node, so the group name travels with the error.
    #[error("permission denied for {device}")]
    PermissionDenied {
        device: String,
        group: Option<String>,
    },

    #[error("no disc in {device}")]
    NoDisc { device: String },

    #[error("the disc in {device} has no audio tracks")]
    NotAnAudioDisc { device: String },

    #[error("{operation} on {device} failed with status {status}")]
    Io {
        device: String,
        operation: &'static str,
        status: i32,
    },

    #[error("the disc in {device} reported an unusable table of contents")]
    UnreadableToc { device: String, reason: TocError },
}

pub trait Drive {
    fn info(&self) -> &DriveInfo;

    /// Reads the table of contents of the disc currently loaded.
    fn read_toc(&mut self) -> Result<Toc, DriveError>;

    fn eject(&mut self) -> Result<(), DriveError>;
}

/// Every optical drive the system currently reports.
pub fn list() -> Vec<DriveInfo> {
    platform::list()
}

pub fn open(id: &str) -> Result<Box<dyn Drive>, DriveError> {
    platform::open(id)
}
