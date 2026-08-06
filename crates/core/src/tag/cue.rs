// SPDX-License-Identifier: GPL-3.0-or-later

//! The cue sheet that goes next to a ripped album.
//!
//! One `FILE` per track rather than one image, because the rip writes a file
//! per track. Every index is therefore zero: a track begins where its own file
//! begins.
//!
//! Cue sheets have no escape sequence for a quotation mark, so a title
//! carrying one is written without it. Every other writer does the same, and a
//! parser meeting an escaped quote would stop reading the line there.

use std::fmt::Write;

use super::Album;

/// Line endings are the ones every other cue sheet in the wild uses, and some
/// older players will not read anything else.
const BREAK: &str = "\r\n";

pub fn sheet(album: &Album) -> String {
    let mut out = String::new();

    if let Some(catalog) = album.barcode.as_deref().and_then(catalog) {
        let _ = writeln_cue(&mut out, 0, &format!("CATALOG {catalog}"));
    }

    if let Some(genre) = present(album.genre.as_deref()) {
        let _ = writeln_cue(&mut out, 0, &format!("REM GENRE {}", quoted(genre)));
    }

    if let Some(date) = present(album.date.as_deref()) {
        let _ = writeln_cue(&mut out, 0, &format!("REM DATE {}", quoted(year(date))));
    }

    let _ = writeln_cue(&mut out, 0, &format!("PERFORMER {}", quoted(&album.artist)));
    let _ = writeln_cue(&mut out, 0, &format!("TITLE {}", quoted(&album.title)));

    for track in &album.tracks {
        let _ = writeln_cue(&mut out, 0, &format!("FILE {} WAVE", quoted(&track.file)));
        let _ = writeln_cue(&mut out, 1, &format!("TRACK {:02} AUDIO", track.number));
        let _ = writeln_cue(&mut out, 2, &format!("TITLE {}", quoted(&track.title)));
        let _ = writeln_cue(&mut out, 2, &format!("PERFORMER {}", quoted(&track.artist)));

        if track.pre_emphasis {
            let _ = writeln_cue(&mut out, 2, "FLAGS PRE");
        }

        let _ = writeln_cue(&mut out, 2, "INDEX 01 00:00:00");
    }

    out
}

fn writeln_cue(out: &mut String, depth: usize, line: &str) -> std::fmt::Result {
    write!(out, "{}{line}{BREAK}", "  ".repeat(depth))
}

fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// `CATALOG` is specified as exactly thirteen digits, which is what an EAN
/// barcode is.
///
/// American releases carry a twelve digit UPC instead, and the two are the same
/// numbering scheme: an EAN-13 beginning with zero *is* a UPC. Padding is
/// therefore a conversion rather than a guess. Anything else belongs in the
/// tags but not here, since a strict parser refuses the whole sheet over it.
fn catalog(barcode: &str) -> Option<String> {
    let digits: &str = barcode.trim();

    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    match digits.len() {
        13 => Some(digits.to_owned()),
        12 => Some(format!("0{digits}")),
        _ => None,
    }
}

/// `REM DATE` carries a year by convention, but a release date arrives from
/// MusicBrainz as a full day. Anything that is not a leading year is passed
/// through: it is a comment, and guessing at it would lose more than it saves.
fn year(date: &str) -> &str {
    match date.split_once('-') {
        Some((year, _)) if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) => year,
        _ => date,
    }
}

fn quoted(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| *c != '"' && !c.is_control())
        .collect();

    format!("\"{cleaned}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::RippedTrack;

    fn track(number: u8, title: &str) -> RippedTrack {
        RippedTrack {
            number,
            file: format!("{number:02} - {title}.flac"),
            title: title.to_owned(),
            artist: "Oki".to_owned(),
            length: 75 * 100,
            pre_emphasis: false,
            unreadable_sectors: 0,
            recovered_sectors: 0,
            checksums: crate::verify::Checksums {
                crc32: 0,
                ctdb_crc32: 0,
                accuraterip_v1: 0,
                accuraterip_v2: 0,
            },
        }
    }

    fn album() -> Album {
        Album {
            title: "Reklamacja'47".to_owned(),
            artist: "Oki".to_owned(),
            date: Some("2020".to_owned()),
            genre: Some("Hip Hop".to_owned()),
            barcode: Some("199957731546".to_owned()),
            track_total: 2,
            tracks: vec![track(1, "Znasz Mnie?"), track(2, "Goat/Simp")],
        }
    }

    #[test]
    fn every_track_gets_its_own_file_and_starts_at_zero() {
        let sheet = sheet(&album());

        assert!(sheet.contains("FILE \"01 - Znasz Mnie?.flac\" WAVE\r\n"));
        assert!(sheet.contains("FILE \"02 - Goat/Simp.flac\" WAVE\r\n"));
        assert_eq!(sheet.matches("INDEX 01 00:00:00").count(), 2);
        assert_eq!(sheet.matches("TRACK 0").count(), 2);
    }

    #[test]
    fn the_album_is_described_before_the_first_file() {
        let sheet = sheet(&album());
        let first_file = sheet.find("FILE").expect("the sheet lists files");

        let header = &sheet[..first_file];
        assert!(header.contains("PERFORMER \"Oki\""));
        assert!(header.contains("TITLE \"Reklamacja'47\""));
        assert!(header.contains("REM GENRE \"Hip Hop\""));
        assert!(header.contains("REM DATE \"2020\""));
    }

    #[test]
    fn a_full_release_date_is_written_as_its_year() {
        let mut album = album();
        album.date = Some("2026-06-12".to_owned());
        assert!(sheet(&album).contains("REM DATE \"2026\""));

        // Nothing that looks like a year is left as it came.
        album.date = Some("late 1997".to_owned());
        assert!(sheet(&album).contains("REM DATE \"late 1997\""));
    }

    // A cue sheet carrying anything but thirteen digits under CATALOG is
    // rejected outright by strict parsers rather than merely ignored.
    #[test]
    fn a_barcode_becomes_a_catalog_number_only_at_the_length_the_spec_wants() {
        let mut album = album();

        // The disc this was written against carries a twelve digit UPC.
        assert!(sheet(&album).contains("CATALOG 0199957731546"));

        album.barcode = Some("5051442936926".to_owned());
        assert!(sheet(&album).contains("CATALOG 5051442936926"));

        album.barcode = Some("74640888625".to_owned());
        assert!(
            !sheet(&album).contains("CATALOG"),
            "eleven digits is neither"
        );

        album.barcode = Some("none".to_owned());
        assert!(!sheet(&album).contains("CATALOG"));
    }

    #[test]
    fn a_quotation_mark_in_a_title_is_dropped_rather_than_escaped() {
        let mut album = album();
        album.tracks[0].title = "The \"Real\" Thing".to_owned();

        let sheet = sheet(&album);
        assert!(sheet.contains("TITLE \"The Real Thing\""));
        assert_eq!(
            sheet.lines().filter(|line| line.contains("TITLE")).count(),
            3,
            "no line was cut in half by a stray quote"
        );
    }

    #[test]
    fn pre_emphasis_is_flagged_only_where_the_disc_reports_it() {
        let mut album = album();
        album.tracks[1].pre_emphasis = true;

        let sheet = sheet(&album);
        assert_eq!(sheet.matches("FLAGS PRE").count(), 1);
    }

    #[test]
    fn missing_optional_fields_leave_no_empty_lines_behind() {
        let album = Album {
            title: "Demo".to_owned(),
            artist: "Nobody".to_owned(),
            tracks: vec![track(1, "One")],
            ..Album::default()
        };

        let sheet = sheet(&album);
        assert!(!sheet.contains("REM"));
        assert!(!sheet.contains("CATALOG"));
        assert!(sheet.lines().all(|line| !line.trim().is_empty()));
    }
}
