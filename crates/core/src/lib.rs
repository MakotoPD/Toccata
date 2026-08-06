// SPDX-License-Identifier: GPL-3.0-or-later

mod base64;

pub mod discid;
pub mod drive;
pub mod encode;
pub mod metadata;
pub mod naming;
pub mod rip;
pub mod settings;
pub mod tag;
pub mod toc;
pub mod verify;

/// Version of the ripping core, reported to the UI.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
