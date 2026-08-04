// SPDX-License-Identifier: GPL-3.0-or-later

// Without this the release build spawns a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};
use toccata_core::drive::{self, DriveError, DriveInfo};
use toccata_core::metadata::cover::Covers;
use toccata_core::metadata::manual::Manual;
use toccata_core::metadata::musicbrainz::MusicBrainz;
use toccata_core::metadata::{Cascade, LookupReport, MetadataError, ReleaseCandidate};
use toccata_core::toc::Toc;

/// The disc currently on screen, plus the metadata sources. Keeping the TOC
/// here rather than passing it back from the frontend means a lookup can never
/// be run against a table of contents the drive did not actually report.
struct AppState {
    disc: Mutex<Option<Toc>>,
    metadata: Cascade,
    covers: Covers,
    /// Manual search talks to MusicBrainz directly rather than through the
    /// cascade, which only knows how to answer a table of contents.
    search: MusicBrainz,
    /// Corrections the user has made, which the cascade also reads from.
    store: Manual,
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

/// Free text search, always available rather than only after the cascade has
/// failed. Hits carry no tracks; `fetch_release` fills one in once chosen.
#[tauri::command]
async fn search_releases(
    artist: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReleaseCandidate>, MetadataError> {
    state.search.search(&artist, &title).await
}

/// Accepts a release address or a bare identifier, so a disc can be pinned to
/// a pressing the user already found by other means.
#[tauri::command]
async fn fetch_release(
    reference: String,
    state: State<'_, AppState>,
) -> Result<Option<ReleaseCandidate>, MetadataError> {
    state.search.release(&reference).await
}

/// Keeps a release under the Disc ID of the disc on screen. The identifier is
/// taken from the table of contents the drive reported rather than from the
/// caller, so a correction can only ever be filed against the right disc.
#[tauri::command]
fn save_release(
    release: ReleaseCandidate,
    state: State<'_, AppState>,
) -> Result<(), MetadataError> {
    let Some(disc_id) = current_disc_id(&state) else {
        return Ok(());
    };

    state.store.save(&disc_id, &release)
}

#[tauri::command]
fn forget_release(state: State<'_, AppState>) -> Result<(), MetadataError> {
    let Some(disc_id) = current_disc_id(&state) else {
        return Ok(());
    };

    state.store.forget(&disc_id)
}

fn current_disc_id(state: &State<'_, AppState>) -> Option<String> {
    state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .as_ref()
        .map(|toc| toc.musicbrainz_disc_id())
}

/// The address comes from whichever database answered, so the fetch itself
/// decides whether that host may be contacted at all.
#[tauri::command]
async fn fetch_cover(
    url: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, MetadataError> {
    state.covers.fetch(&url).await
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
            let discs = app.path().app_data_dir()?.join("discs");

            app.manage(AppState {
                disc: Mutex::new(None),
                metadata: Cascade::standard(&discs),
                covers: Covers::default(),
                search: MusicBrainz::default(),
                store: Manual::new(discs),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            list_drives,
            read_disc,
            lookup_metadata,
            search_releases,
            fetch_release,
            fetch_cover,
            save_release,
            forget_release,
            eject
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Tauri application");
}
