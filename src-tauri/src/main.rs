// SPDX-License-Identifier: GPL-3.0-or-later

// Without this the release build spawns a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use toccata_core::drive::{self, DriveError, DriveInfo, SAMPLES_PER_SECTOR};
use toccata_core::encode::{self, Format};
use toccata_core::library;
use toccata_core::lyrics;
use toccata_core::metadata::artwork::{Artwork, Artworks, Query as ArtworkQuery};
use toccata_core::metadata::cover::{self, Covers};
use toccata_core::metadata::discogs::Discogs;
use toccata_core::metadata::manual::Manual;
use toccata_core::metadata::musicbrainz::MusicBrainz;
use toccata_core::metadata::{Cascade, LookupReport, MetadataError, ReleaseCandidate, SourceId};
use toccata_core::naming::{self, template};
use toccata_core::rip::{self, Options, RipError};
use toccata_core::settings::Settings;
use toccata_core::tag;
use toccata_core::toc::Toc;
use toccata_core::verify::{self, Checksums};

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
    /// Discogs catalogues physical pressings better than anything else, but it
    /// cannot be asked about a table of contents, so it only answers searches.
    discogs: Discogs,
    /// Corrections the user has made, which the cascade also reads from.
    store: Manual,
    /// Raised to ask a rip in progress to stop at the next chunk.
    cancelled: Arc<AtomicBool>,
    artwork: Artworks,
    /// What the last rip hashed to, per track, kept so the window can ask for
    /// an online check whenever it likes rather than only as the rip ends.
    ripped: Arc<Mutex<Vec<(u8, Checksums)>>>,
    verification: verify::ctdb::Verification,
    lrclib: lyrics::Lrclib,
    /// What has been ripped before. Absent when the database refused to open,
    /// which costs the history and nothing else.
    library: Arc<Mutex<Option<library::Library>>>,
    /// Words found for the disc on screen, by track number. Fetched before the
    /// rip so they can be looked at and corrected before anything is written.
    lyrics: Arc<Mutex<HashMap<u8, lyrics::Lyrics>>>,
    settings: Mutex<Settings>,
    /// Where the settings live, so saving does not have to ask again.
    settings_path: PathBuf,
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
    barcode: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReleaseCandidate>, MetadataError> {
    // Both services are asked at once and their answers are shown side by side
    // with the name of whichever found them. One of them failing is not a
    // reason to show nothing.
    let (musicbrainz, discogs) = tokio::join!(
        state.search.search(&artist, &title, &barcode),
        state.discogs.search(&artist, &title, &barcode),
    );

    let mut results = musicbrainz.unwrap_or_default();
    results.extend(discogs.unwrap_or_default());

    Ok(results)
}

/// Accepts a release address or a bare identifier, so a disc can be pinned to
/// a pressing the user already found by other means.
#[tauri::command]
async fn fetch_release(
    reference: String,
    source_id: Option<SourceId>,
    state: State<'_, AppState>,
) -> Result<Option<ReleaseCandidate>, MetadataError> {
    // A pasted address names its own service; a chosen result says which list
    // it came from. Without either, the shape of the identifier decides.
    match source_id {
        Some(SourceId::Discogs) => state.discogs.release(&reference).await,
        Some(SourceId::MusicBrainz) => state.search.release(&reference).await,
        _ => match state.search.release(&reference).await? {
            Some(found) => Ok(Some(found)),
            None => state.discogs.release(&reference).await,
        },
    }
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

/// An image the user picked themselves, which is the last resort when no
/// service has the right cover.
#[tauri::command]
fn cover_from_file(path: String) -> Result<Option<String>, MetadataError> {
    toccata_core::metadata::cover::from_file(std::path::Path::new(&path))
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
fn get_settings(state: State<'_, AppState>) -> Settings {
    state
        .settings
        .lock()
        .expect("state lock is never held across a panic")
        .clone()
}

#[tauri::command]
fn set_settings(settings: Settings, state: State<'_, AppState>) -> Result<(), MetadataError> {
    settings
        .save(&state.settings_path)
        .map_err(|_| MetadataError::Unreadable {
            source_id: SourceId::Manual,
        })?;

    // The clients hold the keys, so they are handed over here rather than
    // looked up on every request.
    state.discogs.set_token(settings.tokens.discogs());

    *state
        .settings
        .lock()
        .expect("state lock is never held across a panic") = settings;

    Ok(())
}

/// Every placeholder a pattern may use, so the interface never has to keep its
/// own copy of the list.
#[tauri::command]
fn naming_tokens() -> Vec<&'static str> {
    template::TOKENS.to_vec()
}

/// One entry per format the build can actually write, so the interface never
/// offers something that is not there. Names of formats and codecs are not
/// translated, which is why the label travels with them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatInfo {
    id: Format,
    label: &'static str,
    extension: &'static str,
    lossy: bool,
    /// What the encoder panel offers for this format, so the panel itself
    /// holds no knowledge of any particular codec.
    tuning: encode::Tuning,
}

#[tauri::command]
fn formats() -> Vec<FormatInfo> {
    encode::ALL
        .iter()
        .map(|format| FormatInfo {
            id: *format,
            label: format.label(),
            extension: format.extension(),
            lossy: format.lossy(),
            tuning: format.tuning(),
        })
        .collect()
}

/// Cover art from every service that answers without an account.
#[tauri::command]
async fn search_artwork(
    query: ArtworkQuery,
    state: State<'_, AppState>,
) -> Result<Vec<Artwork>, ()> {
    Ok(state.artwork.search(&query).await)
}

/// Where a rip of this disc would land. The same function answers the panel
/// that shows the path and the rip that writes there, so the two cannot drift.
fn output_folder(
    app: &AppHandle,
    state: &State<'_, AppState>,
    toc: &toccata_core::toc::Toc,
    release: Option<&ReleaseCandidate>,
) -> PathBuf {
    let settings = state
        .settings
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    // The pattern names folders and the file together, so the folder is
    // everything the pattern produces except its last component.
    let mut components = render_pattern(&settings.pattern, toc, release, None);
    components.pop();

    let root = settings.output_root.unwrap_or_else(|| {
        app.path()
            .audio_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    });

    components.iter().fold(root, |path, part| path.join(part))
}

/// Fills the naming pattern in from the release, for one track or, with no
/// track given, for the disc as a whole.
fn render_pattern(
    pattern: &str,
    toc: &toccata_core::toc::Toc,
    release: Option<&ReleaseCandidate>,
    track: Option<&toccata_core::metadata::TrackMetadata>,
) -> Vec<String> {
    let mut values = template::Values::new();
    let mut put = |key: &str, value: String| {
        values.insert(key.to_owned(), value);
    };

    if let Some(release) = release {
        put("albumartist", release.artist.clone());
        put("album", release.title.clone());
        put("year", release.date.clone().unwrap_or_default());
        put("genre", release.genre.clone().unwrap_or_default());
        put("label", release.label.clone().unwrap_or_default());
        put(
            "catalog",
            release.disambiguation.clone().unwrap_or_default(),
        );
        put("disc", release.disc_number.to_string());
        put(
            "disctotal",
            release
                .disc_total
                .map(|total| total.to_string())
                .unwrap_or_default(),
        );
        put("tracktotal", release.tracks.len().to_string());
        put("artist", release.artist.clone());
    }

    if let Some(track) = track {
        put("title", track.title.clone());
        put("artist", track.artist.clone());
        put(
            "track",
            template::pad(
                u32::from(track.number),
                release.map(|r| r.tracks.len() as u32),
            ),
        );
    }

    template::render(pattern, &values, &toc.musicbrainz_disc_id())
}

#[tauri::command]
fn rip_folder(
    release: Option<ReleaseCandidate>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Option<String> {
    let toc = state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .clone()?;

    Some(
        output_folder(&app, &state, &toc, release.as_ref())
            .display()
            .to_string(),
    )
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
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn rip_disc(
    drive_id: String,
    release: Option<ReleaseCandidate>,
    // Track numbers to extract; empty means every audio track on the disc.
    tracks: Vec<u8>,
    cover: Option<String>,
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

    let root = output_folder(&app, &state, &toc, release.as_ref());
    let settings = state
        .settings
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    // Read before the settings are taken apart below.
    let options = Options {
        drive_offset: settings.drive_offset(&drive_id),
    };

    let pattern = settings.pattern;
    let formats: Vec<(Format, Option<encode::Quality>)> = settings
        .formats
        .iter()
        .map(|format| (*format, settings.qualities.get(format).copied()))
        .collect();

    // The offset belongs to the drive, which is why it is read from the
    // settings rather than accepted from the window. A rip with the wrong one
    // looks perfect and matches nobody, which is the hardest kind of mistake
    // to notice.
    let hashed = state.ripped.clone();
    let recorded = state.library.clone();
    let lyrics = state
        .lyrics
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    // The cover arrives as the data URI the window is showing, so what gets
    // embedded is what the user is looking at rather than a second download.
    let cover = cover.as_deref().and_then(cover::bytes_from_data_uri);

    tauri::async_runtime::spawn_blocking(move || {
        rip_all(
            &drive_id, &toc, release, &tracks, &options, &root, &pattern, &formats, cover, &lyrics,
            &hashed, &recorded, &channel, &cancelled,
        )
    })
    .await
    .map_err(|_| RipError::Write)?
}

#[allow(clippy::too_many_arguments)]
fn rip_all(
    drive_id: &str,
    toc: &toccata_core::toc::Toc,
    release: Option<ReleaseCandidate>,
    wanted: &[u8],
    options: &Options,
    root: &Path,
    pattern: &str,
    formats: &[(Format, Option<encode::Quality>)],
    cover: Option<Vec<u8>>,
    lyrics: &HashMap<u8, lyrics::Lyrics>,
    // Where the checksums are left for the online check to pick up.
    hashed: &Mutex<Vec<(u8, Checksums)>>,
    recorded: &Mutex<Option<library::Library>>,
    channel: &Channel<RipEvent>,
    cancelled: &AtomicBool,
) -> Result<(), RipError> {
    let mut handle = drive::open(drive_id)?;

    // Several formats share a folder only if they cannot collide, and two of
    // them can: `.m4a` holds either ALAC or AAC. So each gets its own.
    let folders: Vec<PathBuf> = formats
        .iter()
        .map(|(format, _)| match formats.len() {
            1 => root.to_path_buf(),
            _ => root.join(format.folder()),
        })
        .collect();

    for folder in &folders {
        fs::create_dir_all(folder).map_err(|_| RipError::Write)?;
    }

    let started = SystemTime::now();
    let drive_name = handle.info().name.clone();
    let mut ripped: Vec<tag::RippedTrack> = Vec::new();
    // Tagging waits until the last track is in, since a tag carries how many
    // there were and that is not known while they are still being read.
    let mut written: Vec<Vec<PathBuf>> = Vec::new();

    let audio: Vec<u8> = toc
        .tracks
        .iter()
        .filter(|track| track.audio)
        .filter(|track| wanted.is_empty() || wanted.contains(&track.number))
        .map(|track| track.number)
        .collect();

    let mut unreadable = 0;

    for (index, number) in audio.iter().copied().enumerate() {
        let entry = release
            .as_ref()
            .and_then(|release| release.tracks.iter().find(|entry| entry.number == number));

        // Only the last component of the pattern names the file; the folders
        // it produced are already part of `root`.
        let name = render_pattern(pattern, toc, release.as_ref(), entry)
            .pop()
            .unwrap_or_else(|| naming::track_file(number, ""));

        let files: Vec<PathBuf> = formats
            .iter()
            .zip(&folders)
            .map(|((format, _), folder)| folder.join(format!("{name}.{}", format.extension())))
            .collect();

        let _ = channel.send(RipEvent::Started {
            track: number,
            position: index as u32 + 1,
            of: audio.len() as u32,
            file: files[0].display().to_string(),
        });

        // One read of the disc feeds every format at once. Reading it again
        // per format would cost minutes and wear the drive for nothing.
        let fanout = encode::Fanout::new(
            formats
                .iter()
                .zip(&files)
                .map(|((format, quality), file)| format.create_with(file, *quality))
                .collect::<Result<Vec<_>, _>>()?,
        );

        // The checksums are taken from the same bytes the encoders are given,
        // which is what makes them describe the files that were written rather
        // than something the drive said on the way past.
        let length = toc
            .tracks
            .iter()
            .find(|track| track.number == number)
            .map_or(0, |track| track.length);

        let mut output = verify::Verified::new(
            fanout,
            length * SAMPLES_PER_SECTOR,
            Some(&number) == audio.first(),
            Some(&number) == audio.last(),
        );
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
                // The codec still holds frames and the container still owes a
                // trailer, so the file is only real once this returns.
                let (fanout, checksums) = output.finish();
                fanout.finish()?;
                unreadable += extracted.unreadable_sectors;

                ripped.push(tag::RippedTrack {
                    number,
                    // The name without its extension; each format's cue sheet
                    // puts its own back on.
                    file: name.clone(),
                    title: entry.map(|entry| entry.title.clone()).unwrap_or_default(),
                    artist: entry
                        .map(|entry| entry.artist.clone())
                        .or_else(|| release.as_ref().map(|release| release.artist.clone()))
                        .unwrap_or_default(),
                    length,
                    pre_emphasis: toc
                        .tracks
                        .iter()
                        .any(|track| track.number == number && track.pre_emphasis),
                    unreadable_sectors: extracted.unreadable_sectors,
                    checksums,
                });
                written.push(files);

                let _ = channel.send(RipEvent::Finished {
                    track: number,
                    unreadable_sectors: extracted.unreadable_sectors,
                });
            }
            Err(reason) => {
                // Half written files are worse than none at all.
                drop(output);
                for file in &files {
                    let _ = fs::remove_file(file);
                }

                let _ = channel.send(RipEvent::Failed {
                    track: number,
                    reason: reason.clone(),
                });
                return Err(reason);
            }
        }
    }

    *hashed
        .lock()
        .expect("state lock is never held across a panic") = ripped
        .iter()
        .map(|track| (track.number, track.checksums))
        .collect();

    let album = describe(toc, release.as_ref(), ripped);

    // Filed under the disc identifier rather than the folder, since a folder
    // can be moved or renamed and the disc cannot.
    if let Some(library) = recorded
        .lock()
        .expect("state lock is never held across a panic")
        .as_mut()
    {
        let _ = library.record(
            &toc.musicbrainz_disc_id(),
            &album,
            &root.display().to_string(),
            &drive_name,
            options.drive_offset,
        );
    }

    // Each format is a finished album of its own: its own files to tag, its
    // own cue sheet naming them, its own copy of the log beside them.
    for (index, ((format, _), folder)) in formats.iter().zip(&folders).enumerate() {
        let named = with_extension(&album, format.extension());
        let files: Vec<PathBuf> = written
            .iter()
            .filter_map(|per| per.get(index).cloned())
            .collect();

        write_tags(&named, release.as_ref(), &files, cover.as_deref(), lyrics);
        write_artefacts(toc, &named, folder, &drive_name, options, started)?;
    }

    let _ = channel.send(RipEvent::Done {
        folder: root.display().to_string(),
        tracks: audio.len() as u32,
        unreadable_sectors: unreadable,
    });

    Ok(())
}

/// The cue sheet and the log that describe the album that was just written.
///
/// A failure here is reported as a failed rip even though the audio is already
/// on disk: a folder that has just taken tens of megabytes and then refuses two
/// kilobytes of text has something wrong with it that the user should hear
/// about.
fn describe(
    toc: &toccata_core::toc::Toc,
    release: Option<&ReleaseCandidate>,
    tracks: Vec<tag::RippedTrack>,
) -> tag::Album {
    tag::Album {
        track_total: toc.tracks.iter().filter(|track| track.audio).count() as u32,
        title: release
            .map(|release| release.title.clone())
            .unwrap_or_default(),
        artist: release
            .map(|release| release.artist.clone())
            .unwrap_or_default(),
        date: release.and_then(|release| release.date.clone()),
        genre: release.and_then(|release| release.genre.clone()),
        barcode: release.and_then(|release| release.barcode.clone()),
        tracks,
    }
}

/// The same album with its file names carrying one format's extension.
fn with_extension(album: &tag::Album, extension: &str) -> tag::Album {
    let mut named = album.clone();

    for track in &mut named.tracks {
        track.file = format!("{}.{extension}", track.file);
    }

    named
}

/// Puts the metadata inside the audio files themselves, which is the only
/// place most players ever look.
///
/// A file that will not take tags does not fail the rip. The audio is correct
/// and on disk; a missing tag is something the user can fix, and throwing the
/// rip away over it would not be a kindness.
fn write_tags(
    album: &tag::Album,
    release: Option<&ReleaseCandidate>,
    files: &[PathBuf],
    cover: Option<&[u8]>,
    lyrics: &HashMap<u8, lyrics::Lyrics>,
) {
    let extras = tag::write::Extras {
        composer: release.and_then(|release| release.composer.as_deref()),
        comment: release.and_then(|release| release.comment.as_deref()),
        compilation: release.is_some_and(|release| release.compilation),
        disc_number: release.map_or(0, |release| release.disc_number),
        disc_total: release.and_then(|release| release.disc_total),
        cover,
        musicbrainz_release_id: release
            .filter(|release| release.source_id == SourceId::MusicBrainz)
            .map(|release| release.id.as_str()),
        // Filled in per track below, since the words differ for each.
        lyrics: None,
    };

    for (path, track) in files.iter().zip(&album.tracks) {
        let words = lyrics.get(&track.number);

        let extras = tag::write::Extras {
            lyrics: words.and_then(|found| found.plain.as_deref()),
            ..extras.clone()
        };

        let _ = tag::write::track(path, album, track, &extras);

        // Beside the audio rather than inside it: that is where players look
        // for timed words, and a tag has nowhere sensible to keep them.
        if let Some(synced) = words.and_then(|found| found.synced.as_deref()) {
            let _ = tag::write::lrc(path, synced);
        }
    }
}

fn write_artefacts(
    toc: &toccata_core::toc::Toc,
    album: &tag::Album,
    root: &Path,
    drive: &str,
    options: &Options,
    started: SystemTime,
) -> Result<(), RipError> {
    let disc_id = toc.musicbrainz_disc_id();
    let freedb_id = toc.freedb_id();
    let base = naming::release_folder(&album.artist, &album.title, &disc_id);

    fs::write(root.join(format!("{base}.cue")), tag::cue::sheet(album))
        .map_err(|_| RipError::Write)?;

    let conditions = tag::log::Conditions {
        drive,
        read_offset: options.drive_offset,
        musicbrainz_disc_id: &disc_id,
        freedb_id: &freedb_id,
        started,
    };

    fs::write(
        root.join(format!("{base}.log")),
        tag::log::write(&conditions, album),
    )
    .map_err(|_| RipError::Write)?;

    Ok(())
}

/// How much of a track is read for listening. Long enough to tell one pressing
/// from another, short enough that the drive answers while the user waits.
const PREVIEW_SECONDS: u32 = 30;

/// The opening of a track, handed over as WAV bytes for the window to play.
///
/// The audio travels over the IPC channel rather than through a file, which
/// saves having to decide when a temporary file stops being needed.
#[tauri::command]
async fn preview_track(
    drive_id: String,
    number: u8,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, RipError> {
    let toc = state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .clone()
        .ok_or(RipError::NoSuchTrack { number })?;

    let audio = tauri::async_runtime::spawn_blocking(move || {
        let mut handle = drive::open(&drive_id)?;
        let mut audio = Vec::new();
        rip::preview(handle.as_mut(), &toc, number, PREVIEW_SECONDS, &mut audio)?;
        Ok::<_, RipError>(audio)
    })
    .await
    .map_err(|_| RipError::Write)??;

    Ok(tauri::ipc::Response::new(audio))
}

/// Words for one track, as the window shows and edits them.
#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackLyrics {
    track: u8,
    #[serde(flatten)]
    lyrics: lyrics::Lyrics,
}

/// Looks the whole disc up at LRCLIB, one track at a time.
///
/// Done before the rip rather than during it, so that what was found can be
/// looked at and corrected before it is written into anything. Tracks that
/// find nothing are simply absent from the answer.
#[tauri::command]
async fn fetch_lyrics(
    release: ReleaseCandidate,
    state: State<'_, AppState>,
) -> Result<Vec<TrackLyrics>, lyrics::LyricsError> {
    let toc = state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    let Some(toc) = toc else {
        return Ok(Vec::new());
    };

    let mut found = Vec::new();

    for entry in &release.tracks {
        let Some(track) = toc.tracks.iter().find(|track| track.number == entry.number) else {
            continue;
        };

        // The length comes from the disc rather than from the database that
        // named the track, since that is the one number nobody disagrees on.
        let seconds = track.duration().as_secs() as u32;
        let artist = match entry.artist.trim().is_empty() {
            true => release.artist.as_str(),
            false => entry.artist.as_str(),
        };

        // One track failing must not cost the rest of the disc its words.
        if let Ok(Some(lyrics)) = state
            .lrclib
            .find(artist, &entry.title, &release.title, seconds)
            .await
        {
            if !lyrics.is_empty() {
                found.push(TrackLyrics {
                    track: entry.number,
                    lyrics,
                });
            }
        }
    }

    *state
        .lyrics
        .lock()
        .expect("state lock is never held across a panic") = found
        .iter()
        .map(|entry| (entry.track, entry.lyrics.clone()))
        .collect();

    Ok(found)
}

/// Replaces what was found with what the user decided, for one track.
#[tauri::command]
fn set_lyrics(entry: TrackLyrics, state: State<'_, AppState>) {
    let mut held = state
        .lyrics
        .lock()
        .expect("state lock is never held across a panic");

    match entry.lyrics.is_empty() {
        true => held.remove(&entry.track),
        false => held.insert(entry.track, entry.lyrics),
    };
}

/// The rips that have already happened, newest first.
#[tauri::command]
fn rip_history(limit: u32, state: State<'_, AppState>) -> Vec<library::Entry> {
    state
        .library
        .lock()
        .expect("state lock is never held across a panic")
        .as_ref()
        .and_then(|library| library.history(limit).ok())
        .unwrap_or_default()
}

/// Every earlier rip of the disc currently in the drive, so that putting one
/// in a second time says so before anything is written again.
#[tauri::command]
fn disc_history(state: State<'_, AppState>) -> Vec<library::Entry> {
    let Some(disc_id) = current_disc_id(&state) else {
        return Vec::new();
    };

    state
        .library
        .lock()
        .expect("state lock is never held across a panic")
        .as_ref()
        .and_then(|library| library.by_disc(&disc_id).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn rip_tracks(rip: i64, state: State<'_, AppState>) -> Vec<library::TrackEntry> {
    state
        .library
        .lock()
        .expect("state lock is never held across a panic")
        .as_ref()
        .and_then(|library| library.tracks(rip).ok())
        .unwrap_or_default()
}

/// Drops one rip from the history. The files stay where they are: this is a
/// record of what happened, not the music.
#[tauri::command]
fn forget_rip(rip: i64, state: State<'_, AppState>) {
    if let Some(library) = state
        .library
        .lock()
        .expect("state lock is never held across a panic")
        .as_ref()
    {
        let _ = library.forget(rip);
    }
}

/// What one track hashed to and what the world makes of it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackVerification {
    track: u8,
    checksums: Checksums,
    verdict: verify::ctdb::Verdict,
}

/// Compares the rip that just finished against everyone else's.
///
/// Asked for separately rather than done as part of the rip, so that a service
/// being slow or down never holds up the audio, which is already on disk and
/// correct whatever this says.
#[tauri::command]
async fn verify_rip(state: State<'_, AppState>) -> Result<Vec<TrackVerification>, MetadataError> {
    let ripped = state
        .ripped
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    let toc = state
        .disc
        .lock()
        .expect("state lock is never held across a panic")
        .clone();

    let (Some(toc), false) = (toc, ripped.is_empty()) else {
        return Ok(Vec::new());
    };

    let last = toc
        .tracks
        .iter()
        .rfind(|track| track.audio)
        .map_or(0, |track| track.number);

    let entries = state.verification.lookup(&toc).await?;
    let verdicts = verify::ctdb::compare(&entries, &ripped, last);

    Ok(ripped
        .iter()
        .zip(verdicts)
        .map(|((track, checksums), verdict)| TrackVerification {
            track: *track,
            checksums: *checksums,
            verdict,
        })
        .collect())
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let discs = app.path().app_data_dir()?.join("discs");

            let settings_path = app.path().app_data_dir()?.join("settings.json");
            let settings = Settings::load(&settings_path);

            let discogs = Discogs::default();
            discogs.set_token(settings.tokens.discogs());

            app.manage(AppState {
                disc: Mutex::new(None),
                metadata: Cascade::standard(&discs),
                covers: Covers::default(),
                search: MusicBrainz::default(),
                discogs,
                store: Manual::new(discs),
                cancelled: Arc::new(AtomicBool::new(false)),
                artwork: Artworks::default(),
                ripped: Arc::new(Mutex::new(Vec::new())),
                verification: verify::ctdb::Verification::default(),
                lrclib: lyrics::Lrclib::default(),
                library: Arc::new(Mutex::new(
                    library::Library::open(&app.path().app_data_dir()?.join("library.db")).ok(),
                )),
                lyrics: Arc::new(Mutex::new(HashMap::new())),
                settings: Mutex::new(settings),
                settings_path,
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
            cover_from_file,
            save_release,
            forget_release,
            rip_disc,
            rip_folder,
            preview_track,
            get_settings,
            set_settings,
            naming_tokens,
            formats,
            fetch_lyrics,
            set_lyrics,
            search_artwork,
            verify_rip,
            rip_history,
            disc_history,
            rip_tracks,
            forget_rip,
            cancel_rip,
            eject
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Tauri application");
}
