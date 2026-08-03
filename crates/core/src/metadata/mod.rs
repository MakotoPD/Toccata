// SPDX-License-Identifier: GPL-3.0-or-later

//! Identifying the disc in the drive.
//!
//! No single database knows every pressing, so sources are tried one after
//! another and every candidate keeps a label saying where it came from. A
//! source that times out or answers with nonsense is skipped, never fatal:
//! ripping has to stay possible with no metadata at all.

use std::future::Future;
use std::pin::Pin;

use serde::Serialize;

use crate::toc::Toc;

pub mod ctdb;
pub mod musicbrainz;

/// Which database a candidate came from. Shown next to every result, because
/// two sources disagreeing about the same disc is normal rather than a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceId {
    MusicBrainz,
    Ctdb,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub number: u8,
    pub title: String,
    pub artist: String,
    /// As the database has it, which is not always what the disc says.
    pub length_ms: Option<u64>,
}

/// One candidate pressing. Several of these under a single disc ID is the
/// normal case, so the choice belongs to the user.
#[derive(Debug, Clone, Serialize)]
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
    /// Which disc of the set this is. The size of the set is unknown until the
    /// release itself is fetched, since a lookup by disc ID only ever returns
    /// the one medium that matched.
    pub disc_number: u32,
    pub disc_total: Option<u32>,
    /// Cover art the source already knows about, ready to use before the
    /// dedicated art sources are consulted.
    pub cover_art: Option<String>,
    pub tracks: Vec<TrackMetadata>,
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

impl Default for Cascade {
    fn default() -> Self {
        Self::new(vec![
            Box::new(musicbrainz::MusicBrainz::default()),
            // CTDB replicates MusicBrainz, Discogs and freedb and matches on a
            // fuzzy TOC, so it reaches discs an exact Disc ID misses.
            Box::new(ctdb::Ctdb::default()),
        ])
    }
}
