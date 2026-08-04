// SPDX-License-Identifier: GPL-3.0-or-later

//! Identifying the disc in the drive.
//!
//! No single database knows every pressing, so sources are tried one after
//! another and every candidate keeps a label saying where it came from. A
//! source that times out or answers with nonsense is skipped, never fatal:
//! ripping has to stay possible with no metadata at all.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::toc::Toc;

pub mod cover;
pub mod ctdb;
pub mod discogs;
pub mod manual;
pub mod musicbrainz;

/// Which database a candidate came from. Shown next to every result, because
/// two sources disagreeing about the same disc is normal rather than a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceId {
    /// Typed or corrected by the user, and therefore the last word.
    Manual,
    MusicBrainz,
    Ctdb,
    Discogs,
    CoverArtArchive,
}

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MetadataError {
    #[error("{source_id:?} could not be reached")]
    Unreachable { source_id: SourceId },

    #[error("{source_id:?} answered with {status}")]
    Rejected { source_id: SourceId, status: u16 },

    #[error("{source_id:?} sent something this version cannot read")]
    Unreadable { source_id: SourceId },
}

impl MetadataError {
    pub fn source_id(&self) -> SourceId {
        match self {
            Self::Unreachable { source_id }
            | Self::Rejected { source_id, .. }
            | Self::Unreadable { source_id } => *source_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub number: u8,
    pub title: String,
    pub artist: String,
    /// As the database has it, which is not always what the disc says.
    pub length_ms: Option<u64>,
}

/// One disc of a release. A boxed set is several of these, and only the user
/// knows which one is in the drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Medium {
    pub position: u32,
    /// Named discs happen on boxed sets and on nothing else.
    pub title: Option<String>,
    /// CD, Vinyl, Digital Media, as the source records it.
    pub format: Option<String>,
    pub tracks: Vec<TrackMetadata>,
}

/// One candidate pressing. Several of these under a single disc ID is the
/// normal case, so the choice belongs to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCandidate {
    pub source_id: SourceId,
    /// Where the answering source got it from, for the ones that aggregate
    /// other databases rather than curating their own.
    pub relayed_from: Option<String>,
    /// Identifier within the source, for fetching the rest later.
    pub id: String,
    pub title: String,
    pub artist: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
    pub barcode: Option<String>,
    /// The source's own note telling near-identical pressings apart.
    pub disambiguation: Option<String>,
    /// Tag fields no identification source reliably provides. They are here
    /// because they end up in the files, and because the user can fill them in
    /// even when every database is silent.
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub composer: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    /// Set when the tracks are by different artists, which changes how most
    /// players group the album.
    #[serde(default)]
    pub compilation: bool,
    /// Which disc of the set this is. The size of the set is unknown until the
    /// release itself is fetched, since a lookup by disc ID only ever returns
    /// the one medium that matched.
    pub disc_number: u32,
    pub disc_total: Option<u32>,
    /// Tracks on each disc of the release. Searching does not return the
    /// tracks themselves, so this is what a search hit can be compared with
    /// the table of contents on.
    pub medium_track_counts: Vec<u32>,
    /// Every disc of the release, with its own tracks. Empty on a search hit,
    /// since neither service lists tracks until the release itself is fetched.
    #[serde(default)]
    pub media: Vec<Medium>,
    /// Cover art the source already knows about, ready to use before the
    /// dedicated art sources are consulted.
    pub cover_art: Option<String>,
    /// Tracks of the disc currently chosen out of [`Self::media`].
    pub tracks: Vec<TrackMetadata>,
}

impl ReleaseCandidate {
    /// Switches to another disc of the release, which is what a boxed set
    /// needs before anything can be tagged with it.
    pub fn use_medium(&mut self, position: u32) {
        let Some(medium) = self.media.iter().find(|medium| medium.position == position) else {
            return;
        };

        self.tracks = medium.tracks.clone();
        self.disc_number = medium.position;
        self.disc_total = Some(self.media.len() as u32);
    }
}

type Lookup<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ReleaseCandidate>, MetadataError>> + Send + 'a>>;

/// Boxed rather than `async fn` so the cascade can hold sources behind `dyn`,
/// which async functions in traits still do not allow.
pub trait MetadataSource: Send + Sync {
    fn id(&self) -> SourceId;

    fn lookup<'a>(&'a self, toc: &'a Toc) -> Lookup<'a>;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupReport {
    pub candidates: Vec<ReleaseCandidate>,
    /// Sources that failed, so the UI can say what was not consulted instead
    /// of pretending the disc is simply unknown.
    pub failures: Vec<MetadataError>,
}

/// Sources in the order they are consulted.
pub struct Cascade {
    sources: Vec<Box<dyn MetadataSource>>,
}

impl Cascade {
    pub fn new(sources: Vec<Box<dyn MetadataSource>>) -> Self {
        Self { sources }
    }

    /// Walks the cascade until something answers. Later sources are only asked
    /// when the earlier ones found nothing, but a failure never stops the walk.
    pub async fn lookup(&self, toc: &Toc) -> LookupReport {
        let mut report = LookupReport {
            candidates: Vec::new(),
            failures: Vec::new(),
        };

        for source in &self.sources {
            match source.lookup(toc).await {
                Ok(found) if !found.is_empty() => {
                    report.candidates = found;
                    break;
                }
                Ok(_) => {}
                Err(error) => report.failures.push(error),
            }
        }

        report
    }
}

impl Cascade {
    /// The order sources are consulted in. `manual_root` is the directory
    /// holding releases the user has already corrected by hand.
    ///
    /// GnuDB, the freedb successor, is deliberately absent. It only answers
    /// clients whose name is on a list it keeps, and the data it holds reaches
    /// us through CTDB anyway, tagged `freedb` and carrying the fields a bare
    /// CDDB record does not have.
    pub fn standard(manual_root: impl Into<PathBuf>) -> Self {
        Self::new(vec![
            // Somebody who already fixed this disc should not have to do it
            // again, so their answer is asked for first and wins outright.
            Box::new(manual::Manual::new(manual_root)),
            Box::new(musicbrainz::MusicBrainz::default()),
            // CTDB replicates MusicBrainz, Discogs and freedb and matches on a
            // fuzzy TOC, so it reaches discs an exact Disc ID misses.
            Box::new(ctdb::Ctdb::default()),
        ])
    }
}
