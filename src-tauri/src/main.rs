// SPDX-License-Identifier: GPL-3.0-or-later

// Without this the release build spawns a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use toccata_core::drive::{self, DriveError, DriveInfo};
use toccata_core::metadata::cover::Covers;
use toccata_core::metadata::manual::Manual;
use toccata_core::metadata::musicbrainz::MusicBrainz;
use toccata_core::metadata::{Cascade, LookupReport, MetadataError, ReleaseCandidate};
use toccata_core::naming;
use toccata_core::rip::{self, Options, RipError};
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
    /// Raised to ask a rip in progress to stop at the next chunk.
    cancelled: Arc<AtomicBool>,
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

/// What the rip reports back while it runs. One channel per rip, rather than a
/// global event, so two of them could never be told apart.
#[derive(Clone, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum RipEvent {
    Started {
        track: u8,
        position: u32,
        of: u32,
        file: String,
    },
    Progress {
        track: u8,
        sectors: u32,
        of: u32,
    },
    Finished {
        track: u8,
        unreadable_sectors: u32,
    },
    Failed {
        track: u8,
        reason: RipError,
    },
    Done {
        folder: String,
        tracks: u32,
        unreadable_sectors: u32,
    },
}

/// Rips every audio track on the disc. The heavy part runs off the async
/// runtime, since reading a disc blocks for minutes at a time.
#[tauri::command]
async fn rip_disc(
    drive_id: String,
    release: Option<ReleaseCandidate>,
    options: Options,
    channel: Channel<RipEvent>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), RipError> {
    let toc = state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .clone()
        .ok_or(RipError::NoSuchTrack { number: 0 })?;

    let cancelled = state.cancelled.clone();
    cancelled.store(false, Ordering::Relaxed);

    let folder = naming::release_folder(
        release.as_ref().map_or("", |release| &release.artist),
        release.as_ref().map_or("", |release| &release.title),
        &toc.musicbrainz_disc_id(),
    );

    let root = app
        .path()
        .audio_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Toccata")
        .join(folder);

    tauri::async_runtime::spawn_blocking(move || {
        rip_all(
            &drive_id, &toc, release, &options, &root, &channel, &cancelled,
        )
    })
    .await
    .map_err(|_| RipError::Write)?
}

fn rip_all(
    drive_id: &str,
    toc: &toccata_core::toc::Toc,
    release: Option<ReleaseCandidate>,
    options: &Options,
    root: &PathBuf,
    channel: &Channel<RipEvent>,
    cancelled: &AtomicBool,
) -> Result<(), RipError> {
    let mut handle = drive::open(drive_id)?;
    fs::create_dir_all(root).map_err(|_| RipError::Write)?;

    let audio: Vec<u8> = toc
        .tracks
        .iter()
        .filter(|track| track.audio)
        .map(|track| track.number)
        .collect();

    let mut unreadable = 0;

    for (index, number) in audio.iter().copied().enumerate() {
        let title = release
            .as_ref()
            .and_then(|release| release.tracks.iter().find(|entry| entry.number == number))
            .map_or("", |entry| entry.title.as_str());

        let file = root.join(format!("{}.wav", naming::track_file(number, title)));
        let _ = channel.send(RipEvent::Started {
            track: number,
            position: index as u32 + 1,
            of: audio.len() as u32,
            file: file.display().to_string(),
        });

        let mut output = BufWriter::new(fs::File::create(&file).map_err(|_| RipError::Write)?);
        let outcome = rip::track(
            handle.as_mut(),
            toc,
            number,
            options,
            &mut output,
            &mut |sectors, of| {
                let _ = channel.send(RipEvent::Progress {
                    track: number,
                    sectors,
                    of,
                });
            },
            cancelled,
        );

        match outcome {
            Ok(extracted) => {
                unreadable += extracted.unreadable_sectors;
                let _ = channel.send(RipEvent::Finished {
                    track: number,
                    unreadable_sectors: extracted.unreadable_sectors,
                });
            }
            Err(reason) => {
                // A half written file is worse than none at all.
                drop(output);
                let _ = fs::remove_file(&file);

                let _ = channel.send(RipEvent::Failed {
                    track: number,
                    reason: reason.clone(),
                });
                return Err(reason);
            }
        }
    }

    let _ = channel.send(RipEvent::Done {
        folder: root.display().to_string(),
        tracks: audio.len() as u32,
        unreadable_sectors: unreadable,
    });

    Ok(())
}

#[tauri::command]
fn cancel_rip(state: State<'_, AppState>) {
    state.cancelled.store(true, Ordering::Relaxed);
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
                cancelled: Arc::new(AtomicBool::new(false)),
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
            rip_disc,
            cancel_rip,
            eject
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Tauri application");
}
