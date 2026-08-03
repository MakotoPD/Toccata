// SPDX-License-Identifier: GPL-3.0-or-later

/// Version of the ripping core, reported to the UI.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
