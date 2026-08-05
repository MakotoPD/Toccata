// SPDX-License-Identifier: GPL-3.0-or-later

//! What goes alongside the audio: the cue sheet and the rip log.
//!
//! Both describe the same finished album from the same data, so they share the
//! description rather than each being handed the pieces separately.

pub mod cue;
pub mod log;
pub mod write;

/// One track as it came off the disc, named by the file it was written to.
#[derive(Debug, Clone)]
pub struct RippedTrack {
    pub number: u8,
    /// The file name on its own. Both artefacts sit in the folder the audio
    /// went to, so a path here would only ever be wrong once the album moved.
    pub file: String,
    pub title: String,
    pub artist: String,
    /// Length in frames, taken from the table of contents.
    pub length: u32,
    pub pre_emphasis: bool,
    /// Sectors the drive refused and that came out as silence. Anything above
    /// zero means the rip is not bit-perfect.
    pub unreadable_sectors: u32,
}

/// The album those tracks belong to.
#[derive(Debug, Clone, Default)]
pub struct Album {
    pub title: String,
    pub artist: String,
    pub date: Option<String>,
    pub genre: Option<String>,
    /// Media catalogue number, which on a commercial disc is the barcode.
    pub barcode: Option<String>,

    /// Audio tracks on the disc, which is not the same as the number ripped:
    /// a tag saying "6 of 1" because only one track was wanted is worse than
    /// no tag at all.
    pub track_total: u32,
    pub tracks: Vec<RippedTrack>,
}

impl Album {
    pub fn unreadable_sectors(&self) -> u32 {
        self.tracks
            .iter()
            .map(|track| track.unreadable_sectors)
            .sum()
    }
}
