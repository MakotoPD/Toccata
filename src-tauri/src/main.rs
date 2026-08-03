// SPDX-License-Identifier: GPL-3.0-or-later

// Without this the release build spawns a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};
use toccata_core::drive::{self, DriveError, DriveInfo};
use toccata_core::metadata::{Cascade, LookupReport};
use toccata_core::toc::Toc;

/// The disc currently on screen, plus the metadata sources. Keeping the TOC
/// here rather than passing it back from the frontend means a lookup can never
/// be run against a table of contents the drive did not actually report.
struct AppState {
    disc: Mutex<Option<Toc>>,
    metadata: Cascade,
}

/// What the UI needs to describe the disc currently in a drive. The
/// identifiers travel alongside the TOC so the frontend never recomputes them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Disc {
    drive: DriveInfo,
    toc: Toc,
    musicbrainz_disc_id: String,
    freedb_id: String,
}

#[tauri::command]
fn core_version() -> &'static str {
    toccata_core::version()
}

#[tauri::command]
fn list_drives() -> Vec<DriveInfo> {
    drive::list()
}

#[tauri::command]
fn read_disc(drive_id: String, state: State<'_, AppState>) -> Result<Disc, DriveError> {
    let mut handle = drive::open(&drive_id)?;
    let toc = handle.read_toc()?;

    *state
        .disc
        .lock()
        .expect("state lock is never held across a panic") = Some(toc.clone());

    Ok(Disc {
        drive: handle.info().clone(),
        musicbrainz_disc_id: toc.musicbrainz_disc_id(),
        freedb_id: toc.freedb_id(),
        toc,
    })
}

#[tauri::command]
async fn lookup_metadata(state: State<'_, AppState>) -> Result<LookupReport, DriveError> {
    let toc = state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    let Some(toc) = toc else {
        return Ok(LookupReport {
            candidates: Vec::new(),
            failures: Vec::new(),
        });
    };

    Ok(state.metadata.lookup(&toc).await)
}

#[tauri::command]
fn eject(drive_id: String, state: State<'_, AppState>) -> Result<(), DriveError> {
    drive::open(&drive_id)?.eject()?;
    *state
        .disc
        .lock()
        .expect("state lock is never held across a panic") = None;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState {
                disc: Mutex::new(None),
                metadata: Cascade::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            list_drives,
            read_disc,
            lookup_metadata,
            eject
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Tauri application");
}
