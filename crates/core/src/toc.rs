// SPDX-License-Identifier: GPL-3.0-or-later

//! The raw table of contents, as reported by the drive.
//!
//! Everything here counts in frames, the 1/75 of a second sector that a CD is
//! addressed in. Two numbering schemes exist and confusing them is the single
//! easiest way to break disc identification: a **sector address** (LBA) puts
//! track 1 of an ordinary disc at 0, while the **frame offset** used by both
//! disc identifier specifications counts the lead-in as well and puts the same
//! track at 150. Fields on [`Toc`] and [`Track`] are always sector addresses;
//! [`Toc::frame_offsets`] is the only place the conversion happens.

use serde::Serialize;
use std::time::Duration;

use crate::discid;

pub const FRAMES_PER_SECOND: u32 = 75;

/// Frames of lead-in that separate a sector address from a frame offset.
pub const LEAD_IN_FRAMES: u32 = 150;

/// Like [`crate::drive::DriveError`], this travels to the UI as a code plus
/// the numbers behind it, never as a finished sentence.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TocError {
    #[error("the disc reports no tracks")]
    Empty,

    #[error(
        "track {number} starts at {start}, which is not after the previous track at {previous}"
    )]
    OutOfOrder {
        number: u8,
        start: u32,
        previous: u32,
    },

    #[error("lead-out at {lead_out} is not after the last track at {last_track_start}")]
    LeadOutTooEarly {
        lead_out: u32,
        last_track_start: u32,
    },
}

/// One entry exactly as the drive hands it over, before any interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TocEntry {
    pub number: u8,
    /// Sector address of the track start.
    pub start: u32,
    /// Control field from subchannel Q.
    pub control: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub number: u8,
    /// Sector address of the track start.
    pub start: u32,
    /// Length in frames, derived from where the next track or the lead-out
    /// begins.
    pub length: u32,
    pub audio: bool,
    pub pre_emphasis: bool,
}

impl Track {
    pub fn duration(&self) -> Duration {
        frames_to_duration(self.length)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Toc {
    pub tracks: Vec<Track>,
    /// Sector address where the lead-out begins.
    pub lead_out: u32,
}

impl Toc {
    pub fn from_entries(entries: &[TocEntry], lead_out: u32) -> Result<Self, TocError> {
        let Some(first) = entries.first() else {
            return Err(TocError::Empty);
        };

        for pair in entries.windows(2) {
            if pair[1].start <= pair[0].start {
                return Err(TocError::OutOfOrder {
                    number: pair[1].number,
                    start: pair[1].start,
                    previous: pair[0].start,
                });
            }
        }

        let last_track_start = entries.last().unwrap_or(first).start;
        if lead_out <= last_track_start {
            return Err(TocError::LeadOutTooEarly {
                lead_out,
                last_track_start,
            });
        }

        let tracks = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let next = entries
                    .get(index + 1)
                    .map_or(lead_out, |following| following.start);

                Track {
                    number: entry.number,
                    start: entry.start,
                    length: next - entry.start,
                    // Subchannel Q control: bit 2 marks a data track, bit 0
                    // marks pre-emphasis.
                    audio: entry.control & 0b0100 == 0,
                    pre_emphasis: entry.control & 0b0001 != 0,
                }
            })
            .collect();

        Ok(Self { tracks, lead_out })
    }

    pub fn first_track_number(&self) -> u8 {
        self.tracks.first().map_or(0, |track| track.number)
    }

    pub fn last_track_number(&self) -> u8 {
        self.tracks.last().map_or(0, |track| track.number)
    }

    /// Track starts as frame offsets, which is what both identifier
    /// specifications are written against.
    pub fn frame_offsets(&self) -> Vec<u32> {
        self.tracks
            .iter()
            .map(|track| track.start + LEAD_IN_FRAMES)
            .collect()
    }

    pub fn musicbrainz_disc_id(&self) -> String {
        discid::musicbrainz(
            self.first_track_number(),
            self.last_track_number(),
            self.lead_out + LEAD_IN_FRAMES,
            &self.frame_offsets(),
        )
    }

    pub fn freedb_id(&self) -> String {
        discid::freedb(self.lead_out + LEAD_IN_FRAMES, &self.frame_offsets())
    }

    /// Playing time of the whole disc, data tracks included.
    pub fn duration(&self) -> Duration {
        frames_to_duration(self.lead_out - self.tracks.first().map_or(0, |track| track.start))
    }

    pub fn has_data_track(&self) -> bool {
        self.tracks.iter().any(|track| !track.audio)
    }
}

fn frames_to_duration(frames: u32) -> Duration {
    Duration::from_nanos(u64::from(frames) * 1_000_000_000 / u64::from(FRAMES_PER_SECOND))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The disc libdiscid tests itself against, expressed the way a drive
    /// reports it: sector addresses, so 150 less than the frame offsets.
    fn libdiscid_reference() -> Toc {
        const FRAME_OFFSETS: [u32; 22] = [
            150, 9700, 25887, 39297, 53795, 63735, 77517, 94877, 107270, 123552, 135522, 148422,
            161197, 174790, 192022, 205545, 218010, 228700, 239590, 255470, 266932, 288750,
        ];

        let entries: Vec<TocEntry> = FRAME_OFFSETS
            .iter()
            .enumerate()
            .map(|(index, offset)| TocEntry {
                number: index as u8 + 1,
                start: offset - LEAD_IN_FRAMES,
                control: 0,
            })
            .collect();

        Toc::from_entries(&entries, 303602 - LEAD_IN_FRAMES).expect("reference TOC is valid")
    }

    #[test]
    fn computes_both_identifiers_from_sector_addresses() {
        let toc = libdiscid_reference();
        assert_eq!(toc.musicbrainz_disc_id(), "xUp1F2NkfP8s8jaeFn_Av3jNEI4-");
        assert_eq!(toc.freedb_id(), "370fce16");
    }

    #[test]
    fn track_lengths_run_up_to_the_next_start() {
        let toc = libdiscid_reference();
        assert_eq!(toc.tracks[0].length, 9700 - 150);
        assert_eq!(toc.tracks.last().unwrap().length, 303602 - 288750);
        assert_eq!(
            toc.tracks.iter().map(|track| track.length).sum::<u32>(),
            303602 - 150
        );
    }

    #[test]
    fn durations_come_out_in_seconds() {
        let toc = Toc::from_entries(
            &[TocEntry {
                number: 1,
                start: 0,
                control: 0,
            }],
            FRAMES_PER_SECOND * 90,
        )
        .unwrap();

        assert_eq!(toc.tracks[0].duration(), Duration::from_secs(90));
        assert_eq!(toc.duration(), Duration::from_secs(90));
    }

    #[test]
    fn reads_control_flags() {
        let toc = Toc::from_entries(
            &[
                TocEntry {
                    number: 1,
                    start: 0,
                    control: 0b0001,
                },
                TocEntry {
                    number: 2,
                    start: 1000,
                    control: 0b0100,
                },
            ],
            2000,
        )
        .unwrap();

        assert!(toc.tracks[0].audio && toc.tracks[0].pre_emphasis);
        assert!(!toc.tracks[1].audio && !toc.tracks[1].pre_emphasis);
        assert!(toc.has_data_track());
    }

    #[test]
    fn rejects_a_toc_without_tracks() {
        assert_eq!(Toc::from_entries(&[], 1000), Err(TocError::Empty));
    }

    #[test]
    fn rejects_tracks_that_do_not_advance() {
        let entries = [
            TocEntry {
                number: 1,
                start: 5000,
                control: 0,
            },
            TocEntry {
                number: 2,
                start: 5000,
                control: 0,
            },
        ];

        assert_eq!(
            Toc::from_entries(&entries, 9000),
            Err(TocError::OutOfOrder {
                number: 2,
                start: 5000,
                previous: 5000,
            })
        );
    }

    #[test]
    fn rejects_a_lead_out_inside_the_last_track() {
        let entries = [TocEntry {
            number: 1,
            start: 5000,
            control: 0,
        }];

        assert_eq!(
            Toc::from_entries(&entries, 5000),
            Err(TocError::LeadOutTooEarly {
                lead_out: 5000,
                last_track_start: 5000,
            })
        );
    }
}
