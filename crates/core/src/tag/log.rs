// SPDX-License-Identifier: GPL-3.0-or-later

//! The rip log, written next to the album.
//!
//! This is a diagnostic artefact rather than something the interface shows, so
//! it stays in English and is not translated. What it has to answer, months
//! later, is whether a file can be trusted: which drive read it, with what
//! offset, and whether any sector came out as silence because the drive gave
//! up on it.

use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use super::Album;
use crate::toc::FRAMES_PER_SECOND;

/// The conditions a rip ran under, which is the half of the log that says
/// nothing about the music.
#[derive(Debug, Clone)]
pub struct Conditions<'a> {
    pub drive: &'a str,
    /// Read offset in samples, EAC convention.
    pub read_offset: i32,
    pub musicbrainz_disc_id: &'a str,
    pub freedb_id: &'a str,
    pub started: SystemTime,
}

pub fn write(conditions: &Conditions, album: &Album) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "Toccata {}", crate::version());
    let _ = writeln!(out, "Ripped {}", timestamp(conditions.started));
    let _ = writeln!(out);

    let _ = writeln!(out, "Drive               : {}", conditions.drive);
    let _ = writeln!(
        out,
        "Read offset         : {:+} samples",
        conditions.read_offset
    );
    let _ = writeln!(
        out,
        "MusicBrainz Disc ID : {}",
        conditions.musicbrainz_disc_id
    );
    let _ = writeln!(out, "FreeDB ID           : {}", conditions.freedb_id);
    let _ = writeln!(out);

    let _ = writeln!(out, "Artist              : {}", album.artist);
    let _ = writeln!(out, "Album               : {}", album.title);
    let _ = writeln!(out);

    // Wide enough to read across, because the point of the checksums is that
    // somebody can compare them with somebody else's by eye.
    let _ = writeln!(
        out,
        "  #  length      CRC32  AccurateRip v1  AccurateRip v2  state       file"
    );

    for track in &album.tracks {
        let _ = writeln!(
            out,
            "{:>3}  {}  {:08X}  {:>14X}  {:>14X}  {}  {}",
            track.number,
            length(track.length),
            track.checksums.crc32,
            track.checksums.accuraterip_v1,
            track.checksums.accuraterip_v2,
            outcome(track.unreadable_sectors),
            track.file
        );
    }

    let _ = writeln!(out);

    let tracks = plural(album.tracks.len() as u32, "track");
    let _ = match album.unreadable_sectors() {
        0 => writeln!(out, "{tracks}, every sector read without error."),
        count => writeln!(
            out,
            "{tracks}, {} filled with silence because the drive could not read them.",
            plural(count, "sector")
        ),
    };

    // Worth saying even when the rip came out whole: a disc that needed asking
    // twice today will need asking three times next year.
    if album.recovered_sectors() > 0 {
        let _ = writeln!(
            out,
            "{} read only after retrying, so the audio is whole but the disc is not what it was.",
            plural(album.recovered_sectors(), "sector")
        );
    }

    out
}

/// English has two forms and this file is only ever written in English, so the
/// i18n layer has no business here.
fn plural(count: u32, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        _ => format!("{count} {noun}s"),
    }
}

fn outcome(unreadable_sectors: u32) -> String {
    match unreadable_sectors {
        0 => "ok        ".to_owned(),
        count => format!("{count:<7} bad"),
    }
}

fn length(frames: u32) -> String {
    let seconds = frames / FRAMES_PER_SECOND;
    format!("{:>3}:{:02}", seconds / 60, seconds % 60)
}

/// A date needs no dependency: the calendar has been the same since 1582 and
/// the conversion is a dozen lines.
fn timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());

    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's `civil_from_days`, which counts from a March based year so
/// that the leap day lands at the end and needs no special case.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let toward_zero = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    };

    let era = toward_zero / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;

    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;

    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;

    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month as u32 + 3
    } else {
        shifted_month as u32 - 9
    };

    let year = if month <= 2 { year + 1 } else { year };

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::RippedTrack;
    use std::time::Duration;

    fn conditions() -> Conditions<'static> {
        Conditions {
            drive: "HL-DT-ST BD-RE BH16NS55",
            read_offset: 6,
            musicbrainz_disc_id: "xUp1F2NkfP8s8jaeFn_Av3jNEI4-",
            freedb_id: "370fce16",
            started: UNIX_EPOCH + Duration::from_secs(1_754_380_364),
        }
    }

    fn album(unreadable: u32) -> Album {
        Album {
            title: "Reklamacja'47".to_owned(),
            artist: "Oki".to_owned(),
            tracks: vec![
                RippedTrack {
                    number: 1,
                    file: "01 - Znasz Mnie?.flac".to_owned(),
                    title: "Znasz Mnie?".to_owned(),
                    artist: "Oki".to_owned(),
                    length: 176 * FRAMES_PER_SECOND,
                    pre_emphasis: false,
                    unreadable_sectors: 0,
                    recovered_sectors: 0,
                    checksums: crate::verify::Checksums {
                        crc32: 0,
                        ctdb_crc32: 0,
                        accuraterip_v1: 0,
                        accuraterip_v2: 0,
                    },
                },
                RippedTrack {
                    number: 2,
                    file: "02 - Goat.flac".to_owned(),
                    title: "Goat".to_owned(),
                    artist: "Oki".to_owned(),
                    length: 151 * FRAMES_PER_SECOND,
                    pre_emphasis: false,
                    unreadable_sectors: unreadable,
                    recovered_sectors: 0,
                    checksums: crate::verify::Checksums {
                        crc32: 0,
                        ctdb_crc32: 0,
                        accuraterip_v1: 0,
                        accuraterip_v2: 0,
                    },
                },
            ],
            ..Album::default()
        }
    }

    #[test]
    fn a_clean_rip_says_so_and_names_every_file() {
        let log = write(&conditions(), &album(0));

        assert!(log.contains("every sector read without error."));
        assert!(log.contains("01 - Znasz Mnie?.flac"));
        assert!(log.contains("02 - Goat.flac"));
        assert!(log.contains("2:56"), "the length is in minutes and seconds");
    }

    // A rip with silenced sectors is the one case the log exists for, so the
    // count has to survive to the summary rather than only sit on the track.
    #[test]
    fn silenced_sectors_are_counted_per_track_and_in_total() {
        let log = write(&conditions(), &album(1234));

        assert!(log.contains("1234    bad"));
        assert!(log.contains("2 tracks, 1234 sectors filled with silence"));
    }

    #[test]
    fn a_single_track_or_sector_is_not_pluralised() {
        let mut album = album(1);
        album.tracks.truncate(1);
        album.tracks[0].unreadable_sectors = 1;

        let log = write(&conditions(), &album);
        assert!(log.contains("1 track, 1 sector filled with silence"));
    }

    #[test]
    fn the_offset_keeps_its_sign() {
        let mut conditions = conditions();
        assert!(write(&conditions, &album(0)).contains("+6 samples"));

        conditions.read_offset = -582;
        assert!(write(&conditions, &album(0)).contains("-582 samples"));

        conditions.read_offset = 0;
        assert!(write(&conditions, &album(0)).contains("+0 samples"));
    }

    #[test]
    fn dates_come_out_of_the_epoch_correctly() {
        let at = |seconds| timestamp(UNIX_EPOCH + Duration::from_secs(seconds));

        assert_eq!(at(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(at(1_000_000_000), "2001-09-09 01:46:40 UTC");
        // The end of a leap day, which is where a home made calendar breaks.
        assert_eq!(at(1_709_251_199), "2024-02-29 23:59:59 UTC");
        assert_eq!(at(1_709_251_200), "2024-03-01 00:00:00 UTC");
        // 2000 was a leap year and 1900 was not, the rule most code forgets.
        assert_eq!(at(951_782_400), "2000-02-29 00:00:00 UTC");
    }
}
