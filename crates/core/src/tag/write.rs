// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting the metadata inside the files.
//!
//! Every format keeps its tags somewhere different — Vorbis comments in FLAC
//! and Ogg, ID3 in MP3, atoms in MP4, an APE tag in APE — and `lofty` knows
//! which of those a given file wants. The same fields are therefore written
//! once here and land wherever they belong, rather than once per format.
//!
//! Fields with nothing in them are left out entirely. An empty tag is worse
//! than a missing one: players show it, and it hides the fact that nobody has
//! filled it in.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::FileType;
use lofty::picture::{Picture, PictureType};
use lofty::tag::{Accessor, ItemKey, Tag, TagExt};

use super::{Album, RippedTrack};

/// Everything about a track that belongs in its tags but not in the cue sheet.
#[derive(Debug, Clone, Default)]
pub struct Extras<'a> {
    pub composer: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub compilation: bool,
    pub disc_number: u32,
    pub disc_total: Option<u32>,
    /// The cover, as the bytes of a JPEG or PNG. The format is sniffed rather
    /// than declared, since it came from whatever service had it.
    pub cover: Option<&'a [u8]>,
    pub musicbrainz_release_id: Option<&'a str>,
    /// The words with no timing. The timed version goes beside the file
    /// instead, since a tag has nowhere sensible to keep it.
    pub lyrics: Option<&'a str>,
}

/// Writes the timed lyrics beside the audio, as most players prefer to find
/// them, and returns whether anything was written.
///
/// The file takes the audio's name with `.lrc` in place of its extension, which
/// is the convention every player looks for.
pub fn lrc(path: &Path, synced: &str) -> Result<bool, TagError> {
    if synced.trim().is_empty() {
        return Ok(false);
    }

    std::fs::write(path.with_extension("lrc"), synced)
        .map(|()| true)
        .map_err(|_| TagError::Write)
}

/// Writes the tags for one finished file.
///
/// `path` has to be the file itself rather than the name in [`RippedTrack`],
/// which is only ever the last component.
pub fn track(
    path: &Path,
    album: &Album,
    track: &RippedTrack,
    extras: &Extras,
) -> Result<(), TagError> {
    // The container decides the tag; asking the extension avoids opening the
    // file only to be told what its name already said.
    let kind = FileType::from_path(path).ok_or(TagError::UnknownFormat)?;
    let mut tag = Tag::new(kind.primary_tag_type());

    tag.set_title(track.title.clone());
    tag.set_track(u32::from(track.number));

    put(&mut tag, ItemKey::TrackArtist, Some(track.artist.as_str()));
    put(&mut tag, ItemKey::AlbumTitle, Some(album.title.as_str()));
    put(&mut tag, ItemKey::AlbumArtist, Some(album.artist.as_str()));
    put(&mut tag, ItemKey::RecordingDate, album.date.as_deref());
    put(&mut tag, ItemKey::Genre, album.genre.as_deref());
    put(&mut tag, ItemKey::Barcode, album.barcode.as_deref());
    put(&mut tag, ItemKey::Composer, extras.composer);
    put(&mut tag, ItemKey::Comment, extras.comment);
    put(&mut tag, ItemKey::Lyrics, extras.lyrics);
    put(
        &mut tag,
        ItemKey::MusicBrainzReleaseId,
        extras.musicbrainz_release_id,
    );

    if album.track_total > 0 {
        tag.set_track_total(album.track_total);
    }

    if extras.disc_number > 0 {
        tag.set_disk(extras.disc_number);
    }

    if let Some(total) = extras.disc_total.filter(|total| *total > 0) {
        tag.set_disk_total(total);
    }

    // Written only when true. A player that meets "0" here treats the album as
    // a compilation anyway, which is exactly backwards.
    if extras.compilation {
        put(&mut tag, ItemKey::FlagCompilation, Some("1"));
    }

    if let Some(cover) = extras.cover {
        // A picture that cannot be read is not worth failing a rip over; the
        // audio is already on disk and correct.
        if let Ok(mut picture) = Picture::from_reader(&mut &cover[..]) {
            picture.set_pic_type(PictureType::CoverFront);
            tag.push_picture(picture);
        }
    }

    tag.save_to_path(path, WriteOptions::default())
        .map_err(|_| TagError::Write)
}

/// Sets a field, or leaves it out when there is nothing to say.
fn put(tag: &mut Tag, key: ItemKey, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };

    tag.insert_text(key, value.to_owned());
}

#[derive(Debug, Clone, thiserror::Error, serde::Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TagError {
    #[error("nothing here knows how to tag that file")]
    UnknownFormat,

    #[error("the tags could not be written")]
    Write,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::Format;

    fn album() -> Album {
        Album {
            title: "Reklamacja'47".to_owned(),
            artist: "Oki".to_owned(),
            date: Some("2020-06-12".to_owned()),
            genre: Some("Hip Hop".to_owned()),
            barcode: Some("199957731546".to_owned()),
            track_total: 12,
            tracks: vec![ripped(1), ripped(2)],
        }
    }

    fn ripped(number: u8) -> RippedTrack {
        RippedTrack {
            number,
            file: format!("{number:02}.flac"),
            title: format!("Track {number}"),
            artist: "Oki".to_owned(),
            length: 7500,
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

    /// A real file of each format, since what is being tested is whether the
    /// tag survives the container.
    fn encode(format: Format, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("toccata-tag-{name}.{}", format.extension()));
        let _ = std::fs::remove_file(&path);

        let mut encoder = format.create(&path).expect("the encoder opens");
        encoder
            .write_all(&vec![0u8; 44_100 * 4])
            .expect("the audio is taken");
        encoder.finish().expect("the file is finished");

        path
    }

    fn read_back(path: &Path) -> Tag {
        use lofty::file::TaggedFileExt;

        lofty::read_from_path(path)
            .expect("the file opens")
            .primary_tag()
            .cloned()
            .expect("the file carries a tag")
    }

    #[test]
    fn the_fields_survive_the_round_trip() {
        let path = encode(Format::Flac, "fields");
        let album = album();

        track(&path, &album, &album.tracks[1], &Extras::default()).expect("the tags are written");

        let tag = read_back(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(tag.title().as_deref(), Some("Track 2"));
        assert_eq!(tag.track(), Some(2));
        assert_eq!(
            tag.track_total(),
            Some(12),
            "the disc had twelve tracks even though two were ripped"
        );
        assert_eq!(tag.album().as_deref(), Some("Reklamacja'47"));
        assert_eq!(tag.get_string(ItemKey::AlbumArtist), Some("Oki"));
        assert_eq!(tag.genre().as_deref(), Some("Hip Hop"));
        assert_eq!(tag.get_string(ItemKey::Barcode), Some("199957731546"));
    }

    // Every format keeps its tags somewhere different, and the point of going
    // through lofty is that none of that has to be known here.
    #[test]
    fn both_containers_come_back_with_the_same_fields() {
        for (format, name) in [(Format::Flac, "both-flac"), (Format::Wav, "both-wav")] {
            let path = encode(format, name);
            let album = album();

            track(&path, &album, &album.tracks[0], &Extras::default())
                .unwrap_or_else(|_| panic!("{name} takes tags"));

            let tag = read_back(&path);
            let _ = std::fs::remove_file(&path);

            assert_eq!(tag.title().as_deref(), Some("Track 1"), "{name}");
            assert_eq!(tag.album().as_deref(), Some("Reklamacja'47"), "{name}");
        }
    }

    #[test]
    fn the_words_go_into_the_tag_and_the_timed_ones_beside_it() {
        let path = encode(Format::Flac, "lyrics");
        let album = album();

        let extras = Extras {
            lyrics: Some("Nie no, dobra"),
            ..Extras::default()
        };

        track(&path, &album, &album.tracks[0], &extras).expect("the tags are written");
        assert!(lrc(&path, "[00:12.00] Nie no, dobra").expect("the lrc is written"));

        let tag = read_back(&path);
        let beside = path.with_extension("lrc");
        let timed = std::fs::read_to_string(&beside).expect("the lrc exists");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&beside);

        assert_eq!(tag.get_string(ItemKey::Lyrics), Some("Nie no, dobra"));
        assert_eq!(timed, "[00:12.00] Nie no, dobra");
    }

    // An empty `.lrc` beside every instrumental would be worse than none.
    #[test]
    fn nothing_is_written_beside_a_track_with_no_timed_words() {
        let path = std::env::temp_dir().join("toccata-empty-lrc.flac");

        assert!(!lrc(&path, "   ").expect("an empty write is not a failure"));
        assert!(!path.with_extension("lrc").exists());
    }

    #[test]
    fn nothing_empty_is_written() {
        let path = encode(Format::Flac, "empty");
        let album = Album {
            title: "Demo".to_owned(),
            artist: "  ".to_owned(),
            tracks: vec![ripped(1)],
            ..Album::default()
        };

        track(&path, &album, &album.tracks[0], &Extras::default()).expect("the tags are written");

        let tag = read_back(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            tag.get_string(ItemKey::AlbumArtist),
            None,
            "whitespace only"
        );
        assert_eq!(tag.genre(), None);
        assert_eq!(tag.get_string(ItemKey::Barcode), None);
        assert_eq!(tag.get_string(ItemKey::FlagCompilation), None);
    }

    #[test]
    fn the_cover_goes_in_with_the_rest() {
        let path = encode(Format::Flac, "cover");
        let album = album();

        // A one pixel PNG, which is enough for the format to be recognised.
        let png = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52,
        ];

        let extras = Extras {
            cover: Some(&png),
            disc_number: 3,
            disc_total: Some(3),
            compilation: true,
            ..Extras::default()
        };

        track(&path, &album, &album.tracks[0], &extras).expect("the tags are written");

        let tag = read_back(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(tag.pictures().len(), 1);
        assert_eq!(tag.pictures()[0].pic_type(), PictureType::CoverFront);
        assert_eq!(tag.disk(), Some(3));
        assert_eq!(tag.disk_total(), Some(3));
        assert_eq!(tag.get_string(ItemKey::FlagCompilation), Some("1"));
    }
}
