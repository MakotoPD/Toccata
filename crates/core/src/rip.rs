// SPDX-License-Identifier: GPL-3.0-or-later

//! Pulling audio off the disc.
//!
//! A CD is addressed in sectors, but a drive does not necessarily hand back the
//! sector it was asked for: every model reads a fixed number of samples early
//! or late. That offset is a property of the drive rather than of the disc or
//! the system, and correcting for it is what makes a rip line up with everyone
//! else's. The convention, and the sign, are the ones EAC established.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::drive::{BYTES_PER_SAMPLE, BYTES_PER_SECTOR, Drive, DriveError, SAMPLES_PER_SECTOR};
use crate::toc::Toc;

/// A CD holds seventy five sectors of audio per second.
pub const SECTORS_PER_SECOND: u32 = 75;

/// Sectors per read. One second of audio, which is also the most the Linux
/// driver accepts in a single call.
const CHUNK_SECTORS: u32 = SECTORS_PER_SECOND;

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RipError {
    #[error("the disc has no track {number}")]
    NoSuchTrack { number: u8 },

    #[error("track {number} is not audio")]
    NotAudio { number: u8 },

    #[error(transparent)]
    Drive(#[from] DriveError),

    #[error(transparent)]
    Encode(#[from] crate::encode::EncodeError),

    #[error("writing the extracted audio failed")]
    Write,

    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    /// Read offset of the drive, in samples, EAC convention. A drive with a
    /// positive offset reads late, so the wanted audio sits that many samples
    /// further along than the table of contents says.
    pub drive_offset: i32,
}

/// What a finished extraction produced.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extracted {
    pub track: u8,
    pub samples: u32,
    /// Sectors the drive refused, filled with silence so the track keeps its
    /// length. Anything above zero means the rip is not bit-perfect.
    pub unreadable_sectors: u32,
}

/// Reads one track and writes its samples out.
///
/// What comes out is the audio and nothing else: interleaved 16 bit stereo,
/// the way it sits on the disc. Wrapping that in a container is the encoder's
/// job, which is what lets one read feed several of them.
///
/// `progress` is called with the number of sectors done and the total. The rip
/// stops as soon as `cancelled` is set, leaving a partial file for the caller
/// to remove.
pub fn track<W: Write>(
    drive: &mut dyn Drive,
    toc: &Toc,
    number: u8,
    options: &Options,
    output: &mut W,
    progress: &mut dyn FnMut(u32, u32),
    cancelled: &AtomicBool,
) -> Result<Extracted, RipError> {
    let track = toc
        .tracks
        .iter()
        .find(|track| track.number == number)
        .ok_or(RipError::NoSuchTrack { number })?;

    if !track.audio {
        return Err(RipError::NotAudio { number });
    }

    let samples = track.length * SAMPLES_PER_SECTOR;
    let first_sample =
        i64::from(track.start) * i64::from(SAMPLES_PER_SECTOR) + i64::from(options.drive_offset);
    let plan = Plan::new(first_sample, samples);

    let mut buffer = vec![0u8; CHUNK_SECTORS as usize * BYTES_PER_SECTOR];
    let mut unreadable = 0;
    let mut written = 0usize;
    let total_bytes = samples as usize * BYTES_PER_SAMPLE;
    let mut skip = plan.byte_offset;

    let mut sector = plan.first_sector;
    let end = plan.first_sector + i64::from(plan.sectors);

    while sector < end {
        if cancelled.load(Ordering::Relaxed) {
            return Err(RipError::Cancelled);
        }

        let batch = ((end - sector) as u32).min(CHUNK_SECTORS);
        let filled = &mut buffer[..batch as usize * BYTES_PER_SECTOR];

        // Audio wanted from before the first sector or past the lead-out does
        // not exist on the disc; the offset correction simply shifts silence in.
        unreadable += read_clamped(drive, toc, sector, batch, filled)?;

        let usable = &filled[skip.min(filled.len())..];
        let take = usable.len().min(total_bytes - written);
        output
            .write_all(&usable[..take])
            .map_err(|_| RipError::Write)?;

        written += take;
        skip = 0;
        sector += i64::from(batch);

        let done = (sector - plan.first_sector) as u32;
        progress(done.min(plan.sectors), plan.sectors);
    }

    output.flush().map_err(|_| RipError::Write)?;

    Ok(Extracted {
        track: number,
        samples,
        unreadable_sectors: unreadable,
    })
}

/// Reads the opening of a track so it can be listened to.
///
/// This is not a rip: the drive offset is left alone, since a handful of
/// samples of drift cannot be heard and the audio is thrown away as soon as it
/// has been played. A sector the drive refuses comes out as silence rather than
/// as an error, because a preview that stops halfway is worse than one with a
/// gap in it.
pub fn preview<W: Write>(
    drive: &mut dyn Drive,
    toc: &Toc,
    number: u8,
    seconds: u32,
    output: &mut W,
) -> Result<(), RipError> {
    let track = toc
        .tracks
        .iter()
        .find(|track| track.number == number)
        .ok_or(RipError::NoSuchTrack { number })?;

    if !track.audio {
        return Err(RipError::NotAudio { number });
    }

    // Unlike a rip, a preview carries its own container: it goes straight to
    // the window to be played, and the length is known before a sector is read.
    let sectors = track.length.min(seconds * SECTORS_PER_SECOND);
    let bytes = sectors * SAMPLES_PER_SECTOR * BYTES_PER_SAMPLE as u32;
    crate::encode::wav::write_header(output, bytes).map_err(|_| RipError::Write)?;

    let mut buffer = vec![0u8; CHUNK_SECTORS as usize * BYTES_PER_SECTOR];
    let mut done = 0;

    while done < sectors {
        let batch = (sectors - done).min(CHUNK_SECTORS);
        let filled = &mut buffer[..batch as usize * BYTES_PER_SECTOR];

        read_clamped(drive, toc, i64::from(track.start + done), batch, filled)?;
        output.write_all(filled).map_err(|_| RipError::Write)?;

        done += batch;
    }

    output.flush().map_err(|_| RipError::Write)?;

    Ok(())
}

/// Reads a run of sectors, silencing the parts that fall outside the disc and
/// the ones the drive refuses. Returns how many sectors came back as silence
/// because of a read failure, which is what stops a rip counting as perfect.
fn read_clamped(
    drive: &mut dyn Drive,
    toc: &Toc,
    start: i64,
    sectors: u32,
    into: &mut [u8],
) -> Result<u32, RipError> {
    into.fill(0);

    let lead_out = i64::from(toc.lead_out);
    let first = start.max(0);
    let last = (start + i64::from(sectors)).min(lead_out);

    if last <= first {
        return Ok(0);
    }

    let available = (last - first) as u32;
    let offset = (first - start) as usize * BYTES_PER_SECTOR;
    let slice = &mut into[offset..offset + available as usize * BYTES_PER_SECTOR];

    match drive.read_audio(first as u32, available, slice) {
        Ok(()) => Ok(0),
        Err(DriveError::UnreadableAudio { .. }) => {
            // Keep going with silence rather than abandoning the track: the
            // count travels back so the result can be reported as imperfect.
            slice.fill(0);
            Ok(available)
        }
        Err(error) => Err(error.into()),
    }
}

/// Where the wanted samples sit once the drive offset has moved them.
#[derive(Debug, PartialEq, Eq)]
struct Plan {
    /// May be negative, or run past the lead-out, when the offset pushes the
    /// track past the edge of the disc.
    first_sector: i64,
    sectors: u32,
    /// Bytes to drop from the front of the first sector.
    byte_offset: usize,
}

impl Plan {
    fn new(first_sample: i64, samples: u32) -> Self {
        let per_sector = i64::from(SAMPLES_PER_SECTOR);
        let first_sector = first_sample.div_euclid(per_sector);
        let within = first_sample.rem_euclid(per_sector) as u32;
        let sectors = (within + samples).div_ceil(SAMPLES_PER_SECTOR);

        Self {
            first_sector,
            sectors,
            byte_offset: within as usize * BYTES_PER_SAMPLE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::TocEntry;

    #[test]
    fn without_an_offset_the_plan_starts_on_a_sector_boundary() {
        let plan = Plan::new(100 * i64::from(SAMPLES_PER_SECTOR), 3 * SAMPLES_PER_SECTOR);

        assert_eq!(
            plan,
            Plan {
                first_sector: 100,
                sectors: 3,
                byte_offset: 0
            }
        );
    }

    #[test]
    fn a_positive_offset_reads_further_along_and_needs_one_more_sector() {
        let plan = Plan::new(100 * i64::from(SAMPLES_PER_SECTOR) + 6, SAMPLES_PER_SECTOR);

        assert_eq!(plan.first_sector, 100);
        assert_eq!(plan.byte_offset, 6 * BYTES_PER_SAMPLE);
        assert_eq!(plan.sectors, 2, "the tail spills into the next sector");
    }

    // A drive that reads early pushes track one before the start of the disc,
    // which is the case that turns a signed division into silent corruption.
    #[test]
    fn a_negative_offset_before_the_first_sector_rounds_downwards() {
        let plan = Plan::new(-6, SAMPLES_PER_SECTOR);

        assert_eq!(plan.first_sector, -1);
        assert_eq!(
            plan.byte_offset,
            (SAMPLES_PER_SECTOR as usize - 6) * BYTES_PER_SAMPLE
        );
        assert_eq!(plan.sectors, 2);
    }

    /// A disc that hands back a counter in every sample, so the extracted bytes
    /// say exactly which sectors they came from.
    struct Counting {
        info: crate::drive::DriveInfo,
        refuse: Option<u32>,
    }

    impl Drive for Counting {
        fn info(&self) -> &crate::drive::DriveInfo {
            &self.info
        }

        fn read_toc(&mut self) -> Result<Toc, DriveError> {
            unreachable!("the test builds its own table of contents")
        }

        fn read_audio(
            &mut self,
            start: u32,
            sectors: u32,
            into: &mut [u8],
        ) -> Result<(), DriveError> {
            if self.refuse == Some(start) {
                return Err(DriveError::UnreadableAudio {
                    device: self.info.id.clone(),
                    start,
                    sectors,
                    status: 0,
                });
            }

            for sector in 0..sectors as usize {
                let value = (start as u16).wrapping_add(sector as u16);
                let bytes = value.to_le_bytes();

                for chunk in into[sector * BYTES_PER_SECTOR..(sector + 1) * BYTES_PER_SECTOR]
                    .chunks_exact_mut(2)
                {
                    chunk.copy_from_slice(&bytes);
                }
            }

            Ok(())
        }

        fn eject(&mut self) -> Result<(), DriveError> {
            unreachable!("nothing in these tests ejects")
        }
    }

    fn disc() -> (Counting, Toc) {
        let entries = [
            TocEntry {
                number: 1,
                start: 0,
                control: 0,
            },
            TocEntry {
                number: 2,
                start: 10,
                control: 0,
            },
        ];

        (
            Counting {
                info: crate::drive::DriveInfo {
                    id: "test".to_owned(),
                    name: "test".to_owned(),
                },
                refuse: None,
            },
            Toc::from_entries(&entries, 20).unwrap(),
        )
    }

    fn rip(drive: &mut Counting, toc: &Toc, number: u8, offset: i32) -> (Vec<u8>, Extracted) {
        let mut out = Vec::new();
        let extracted = track(
            drive,
            toc,
            number,
            &Options {
                drive_offset: offset,
            },
            &mut out,
            &mut |_, _| {},
            &AtomicBool::new(false),
        )
        .expect("the track is readable");

        (out, extracted)
    }

    #[test]
    fn a_track_comes_out_the_length_the_toc_says() {
        let (mut drive, toc) = disc();
        let (out, extracted) = rip(&mut drive, &toc, 2, 0);

        assert_eq!(extracted.samples, 10 * SAMPLES_PER_SECTOR);
        assert_eq!(extracted.unreadable_sectors, 0);
        assert_eq!(
            out.len(),
            10 * SAMPLES_PER_SECTOR as usize * BYTES_PER_SAMPLE,
            "the samples come out on their own, with no container around them"
        );
        // Track two starts at sector 10, and the disc numbers its sectors.
        assert_eq!(&out[0..2], &10u16.to_le_bytes());
    }

    #[test]
    fn an_offset_shifts_the_audio_without_changing_its_length() {
        let (mut drive, toc) = disc();
        let (straight, _) = rip(&mut drive, &toc, 2, 0);
        let (shifted, _) = rip(&mut drive, &toc, 2, SAMPLES_PER_SECTOR as i32);

        assert_eq!(straight.len(), shifted.len());
        // A one sector offset makes track two start where sector 11 does.
        assert_eq!(&shifted[0..2], &11u16.to_le_bytes());
    }

    #[test]
    fn audio_from_beyond_the_lead_out_comes_back_as_silence() {
        let (mut drive, toc) = disc();
        let (out, extracted) = rip(&mut drive, &toc, 2, SAMPLES_PER_SECTOR as i32);

        assert_eq!(
            extracted.unreadable_sectors, 0,
            "running off the disc is not a read failure"
        );
        assert_eq!(
            &out[out.len() - 2..],
            &0u16.to_le_bytes(),
            "the sector past the lead-out is silent"
        );
    }

    #[test]
    fn a_refused_sector_is_silenced_and_counted() {
        let (mut drive, toc) = disc();
        drive.refuse = Some(10);

        let (out, extracted) = rip(&mut drive, &toc, 2, 0);

        assert_eq!(extracted.unreadable_sectors, 10);
        assert!(out.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn cancelling_stops_the_rip() {
        let (mut drive, toc) = disc();
        let mut out = Vec::new();

        let result = track(
            &mut drive,
            &toc,
            2,
            &Options::default(),
            &mut out,
            &mut |_, _| {},
            &AtomicBool::new(true),
        );

        assert!(matches!(result, Err(RipError::Cancelled)));
    }

    #[test]
    fn a_preview_stops_at_the_end_of_a_short_track() {
        let (mut drive, toc) = disc();
        let mut out = Vec::new();

        // Track two is ten sectors long, so a minute of preview cannot happen.
        preview(&mut drive, &toc, 2, 60, &mut out).expect("the track is readable");

        assert_eq!(
            out.len(),
            44 + 10 * SAMPLES_PER_SECTOR as usize * BYTES_PER_SAMPLE
        );
        assert_eq!(&out[44..46], &10u16.to_le_bytes());
    }

    #[test]
    fn a_track_that_is_not_on_the_disc_is_refused() {
        let (mut drive, toc) = disc();
        let mut out = Vec::new();

        let result = track(
            &mut drive,
            &toc,
            9,
            &Options::default(),
            &mut out,
            &mut |_, _| {},
            &AtomicBool::new(false),
        );

        assert!(matches!(result, Err(RipError::NoSuchTrack { number: 9 })));
    }
}
