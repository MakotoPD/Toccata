// SPDX-License-Identifier: GPL-3.0-or-later

//! Discogs, searched by name or by barcode.
//!
//! Discogs catalogues physical pressings better than anything else, which is
//! exactly what a CD ripper is looking at. It has no way to look a disc up by
//! its table of contents, so it sits behind searching rather than in the
//! cascade, and it answers anonymously at a lower rate limit than it would with
//! a token. That keeps the application usable without an account.

use std::time::Duration;

use serde::Deserialize;

use super::{Medium, MetadataError, ReleaseCandidate, SourceId, TrackMetadata};

const BASE_URL: &str = "https://api.discogs.com";

/// Discogs refuses requests from clients that do not name themselves.
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " +https://github.com/MakotoPD/Toccata"
);
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Discogs {
    client: reqwest::Client,
    /// Set only when the user has supplied one. Without it everything still
    /// works, at the lower rate limit Discogs gives anonymous callers.
    token: std::sync::Mutex<Option<String>>,
}

impl Default for Discogs {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(TIMEOUT)
                .build()
                .expect("the http client has no configuration that can fail"),
            token: std::sync::Mutex::new(None),
        }
    }
}

impl Discogs {
    /// Takes the key the user has put in the settings, or clears it.
    pub fn set_token(&self, token: Option<&str>) {
        *self
            .token
            .lock()
            .expect("the token lock is never held across a panic") = token.map(str::to_owned);
    }

    /// Discogs takes the key as a header rather than a query parameter, which
    /// keeps it out of logs and out of anything that records addresses.
    fn authorised(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let token = self
            .token
            .lock()
            .expect("the token lock is never held across a panic")
            .clone();

        match token {
            Some(token) => request.header("Authorization", format!("Discogs token={token}")),
            None => request,
        }
    }

    pub async fn search(
        &self,
        artist: &str,
        title: &str,
        barcode: &str,
    ) -> Result<Vec<ReleaseCandidate>, MetadataError> {
        // A barcode names one pressing, so it is not narrowed further.
        let query: Vec<(&str, &str)> = if !barcode.trim().is_empty() {
            vec![("barcode", barcode.trim()), ("type", "release")]
        } else {
            let terms = [artist.trim(), title.trim()]
                .into_iter()
                .filter(|term| !term.is_empty())
                .collect::<Vec<_>>()
                .join(" ");

            if terms.is_empty() {
                return Ok(Vec::new());
            }

            return self.run(&[("q", &terms), ("type", "release")]).await;
        };

        self.run(&query).await
    }

    async fn run(&self, query: &[(&str, &str)]) -> Result<Vec<ReleaseCandidate>, MetadataError> {
        let response = self
            .authorised(
                self.client
                    .get(format!("{BASE_URL}/database/search"))
                    .query(query),
            )
            .send()
            .await
            .map_err(|_| MetadataError::Unreachable {
                source_id: SourceId::Discogs,
            })?;

        if !response.status().is_success() {
            return Err(MetadataError::Rejected {
                source_id: SourceId::Discogs,
                status: response.status().as_u16(),
            });
        }

        let payload: SearchResponse =
            response
                .json()
                .await
                .map_err(|_| MetadataError::Unreadable {
                    source_id: SourceId::Discogs,
                })?;

        Ok(payload.results.into_iter().map(into_summary).collect())
    }

    /// The full release, which is the only place the track list lives.
    pub async fn release(&self, id: &str) -> Result<Option<ReleaseCandidate>, MetadataError> {
        let Some(id) = release_id_from(id) else {
            return Ok(None);
        };

        let response = self
            .authorised(self.client.get(format!("{BASE_URL}/releases/{id}")))
            .send()
            .await
            .map_err(|_| MetadataError::Unreachable {
                source_id: SourceId::Discogs,
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(MetadataError::Rejected {
                source_id: SourceId::Discogs,
                status: response.status().as_u16(),
            });
        }

        let release: Release = response
            .json()
            .await
            .map_err(|_| MetadataError::Unreadable {
                source_id: SourceId::Discogs,
            })?;

        Ok(Some(into_candidate(release)))
    }
}

/// Pulls a Discogs release number out of a bare id or an address. Master pages
/// are refused: a master is the work, not the pressing in the drive.
pub fn release_id_from(input: &str) -> Option<String> {
    let trimmed = input.trim();

    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Some(trimmed.to_owned());
    }

    let lowered = trimmed.to_ascii_lowercase();
    if !lowered.contains("discogs.com") {
        return None;
    }

    let after = lowered
        .split("/release/")
        .nth(1)
        .or_else(|| lowered.split("/releases/").nth(1))?;

    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

/// Discogs keeps artist names unique by adding a number to the duplicates, and
/// that number has no business ending up in a tag.
fn clean_artist(name: &str) -> String {
    let trimmed = name.trim();

    let Some(open) = trimmed.rfind(" (") else {
        return trimmed.to_owned();
    };

    let suffix = &trimmed[open + 2..];
    if suffix.ends_with(')')
        && suffix[..suffix.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        trimmed[..open].to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Search hits carry the title as "Artist - Album" in one string.
fn split_title(combined: &str) -> (String, String) {
    match combined.split_once(" - ") {
        Some((artist, title)) => (clean_artist(artist), title.trim().to_owned()),
        None => (String::new(), combined.trim().to_owned()),
    }
}

fn into_summary(result: SearchResult) -> ReleaseCandidate {
    let (artist, title) = split_title(&result.title);

    ReleaseCandidate {
        source_id: SourceId::Discogs,
        relayed_from: None,
        id: result.id.to_string(),
        title,
        artist,
        date: result.year.filter(|year| !year.is_empty()),
        country: result.country.filter(|value| !value.is_empty()),
        label: result.label.into_iter().next(),
        barcode: result.barcode.into_iter().next(),
        disambiguation: result.catno.filter(|value| !value.is_empty()),
        genre: result.genre.into_iter().next(),
        style: result.style.into_iter().next(),
        composer: None,
        comment: None,
        compilation: false,
        disc_number: 1,
        disc_total: None,
        // A search hit says nothing about how the tracks are spread over discs.
        medium_track_counts: Vec::new(),
        media: Vec::new(),
        cover_art: result.cover_image.filter(|url| url.starts_with("http")),
        tracks: Vec::new(),
    }
}

/// Splits a flat track list into the discs it actually describes.
///
/// Discogs writes a boxed set as one list with positions like `2-3`, and the
/// disc number in front is the only thing separating one CD from the next.
/// Flattening that puts the first disc's titles on whichever disc is in the
/// drive, which is worse than having no titles at all.
fn group_media(tracklist: &[Track], release_artist: &str) -> Vec<Medium> {
    let mut media: Vec<Medium> = Vec::new();

    for track in tracklist {
        // Headings and index entries name a section; they are not tracks.
        if track.track_type.as_deref().unwrap_or("track") != "track" {
            continue;
        }

        let disc = disc_of(&track.position).unwrap_or(1);
        let entry = TrackMetadata {
            number: 0,
            title: track.title.trim().to_owned(),
            artist: track
                .artists
                .first()
                .map(|artist| clean_artist(&artist.name))
                .unwrap_or_else(|| release_artist.to_owned()),
            length_ms: parse_duration(&track.duration),
        };

        match media.iter_mut().find(|medium| medium.position == disc) {
            Some(medium) => medium.tracks.push(entry),
            None => media.push(Medium {
                position: disc,
                title: None,
                format: None,
                tracks: vec![entry],
            }),
        }
    }

    media.sort_by_key(|medium| medium.position);

    // Numbering follows the order on the disc rather than the printed position,
    // which is what lines up with the table of contents.
    for medium in &mut media {
        for (index, track) in medium.tracks.iter_mut().enumerate() {
            track.number = index as u8 + 1;
        }
    }

    media
}

/// The disc a printed position belongs to, when it names one at all.
fn disc_of(position: &str) -> Option<u32> {
    let (disc, track) = position.trim().split_once('-')?;

    // The second half has to look like a track number, or this is a date, a
    // vinyl side range, or something else that only looks like a position.
    if track.is_empty() || !track.chars().next()?.is_ascii_digit() {
        return None;
    }

    let digits: String = disc.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn into_candidate(release: Release) -> ReleaseCandidate {
    let artist = release
        .artists
        .first()
        .map(|artist| clean_artist(&artist.name))
        .unwrap_or_default();

    let media = group_media(&release.tracklist, &artist);
    let tracks = media
        .first()
        .map(|medium| medium.tracks.clone())
        .unwrap_or_default();

    ReleaseCandidate {
        source_id: SourceId::Discogs,
        relayed_from: None,
        id: release.id.to_string(),
        title: release.title.trim().to_owned(),
        artist,
        date: release
            .released
            .filter(|value| !value.is_empty())
            .or_else(|| release.year.map(|year| year.to_string())),
        country: release.country.filter(|value| !value.is_empty()),
        label: release.labels.first().map(|label| label.name.clone()),
        barcode: release
            .identifiers
            .iter()
            .find(|entry| entry.kind.eq_ignore_ascii_case("barcode"))
            .map(|entry| entry.value.replace(' ', "")),
        disambiguation: release.labels.first().and_then(|label| label.catno.clone()),
        genre: release.genres.first().cloned(),
        style: release.styles.first().cloned(),
        composer: None,
        comment: None,
        compilation: false,
        disc_number: media.first().map_or(1, |medium| medium.position),
        disc_total: (media.len() > 1).then_some(media.len() as u32),
        medium_track_counts: media
            .iter()
            .map(|medium| medium.tracks.len() as u32)
            .collect(),
        media,
        // The primary image is the front cover; anything else is a booklet
        // scan or a photograph of the disc.
        cover_art: release
            .images
            .iter()
            .find(|image| image.kind.as_deref() == Some("primary"))
            .or_else(|| release.images.first())
            .and_then(|image| image.uri.clone()),
        tracks,
    }
}

/// Discogs writes durations as `m:ss`, and leaves them blank often enough that
/// a missing one cannot be an error.
fn parse_duration(value: &str) -> Option<u64> {
    let mut parts = value.trim().split(':').rev();
    let seconds: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next().unwrap_or("0").parse().ok()?;
    let hours: u64 = parts.next().unwrap_or("0").parse().ok()?;

    Some(((hours * 60 + minutes) * 60 + seconds) * 1000)
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    id: u64,
    title: String,
    year: Option<String>,
    country: Option<String>,
    catno: Option<String>,
    cover_image: Option<String>,
    #[serde(default)]
    label: Vec<String>,
    #[serde(default)]
    barcode: Vec<String>,
    #[serde(default)]
    genre: Vec<String>,
    #[serde(default)]
    style: Vec<String>,
}

#[derive(Deserialize)]
struct Release {
    id: u64,
    title: String,
    year: Option<u32>,
    released: Option<String>,
    country: Option<String>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    identifiers: Vec<Identifier>,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    tracklist: Vec<Track>,
}

#[derive(Deserialize)]
struct Artist {
    name: String,
}

#[derive(Deserialize)]
struct Label {
    name: String,
    catno: Option<String>,
}

#[derive(Deserialize)]
struct Identifier {
    #[serde(rename = "type")]
    kind: String,
    value: String,
}

#[derive(Deserialize)]
struct Image {
    #[serde(rename = "type")]
    kind: Option<String>,
    uri: Option<String>,
}

#[derive(Deserialize)]
struct Track {
    #[serde(rename = "type_")]
    track_type: Option<String>,
    #[serde(default)]
    position: String,
    title: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    artists: Vec<Artist>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_number_discogs_adds_to_duplicate_names() {
        assert_eq!(clean_artist("Oki (14)"), "Oki");
        assert_eq!(clean_artist("Nirvana (2)"), "Nirvana");
        assert_eq!(clean_artist("Beastie Boys"), "Beastie Boys");
        // A number that is part of the name stays put.
        assert_eq!(clean_artist("Blink (182)"), "Blink");
        assert_eq!(clean_artist("Sunset (Live)"), "Sunset (Live)");
    }

    #[test]
    fn splits_the_combined_title_a_search_hit_carries() {
        assert_eq!(
            split_title("Oki (14) - Produkt47"),
            ("Oki".to_owned(), "Produkt47".to_owned())
        );
        assert_eq!(
            split_title("Untitled"),
            (String::new(), "Untitled".to_owned())
        );
    }

    #[test]
    fn finds_a_release_number_in_whatever_was_pasted() {
        assert_eq!(release_id_from("23525564"), Some("23525564".to_owned()));
        assert_eq!(
            release_id_from("https://www.discogs.com/release/23525564-Oki-Produkt47"),
            Some("23525564".to_owned())
        );
        assert_eq!(
            release_id_from("https://api.discogs.com/releases/23525564"),
            Some("23525564".to_owned())
        );
    }

    #[test]
    fn refuses_anything_that_is_not_a_pressing() {
        assert_eq!(release_id_from(""), None);
        assert_eq!(release_id_from("Produkt47"), None);
        // A master groups every pressing; it is not the disc in the drive.
        assert_eq!(
            release_id_from("https://www.discogs.com/master/4284645-Oki-Produkt47"),
            None
        );
    }

    #[test]
    fn reads_durations_and_tolerates_the_missing_ones() {
        assert_eq!(parse_duration("2:56"), Some(176_000));
        assert_eq!(parse_duration("1:02:03"), Some(3_723_000));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("  "), None);
    }

    #[test]
    fn reads_the_disc_out_of_a_printed_position() {
        assert_eq!(disc_of("1-1"), Some(1));
        assert_eq!(disc_of("3-7"), Some(3));
        assert_eq!(disc_of("CD2-4"), Some(2));
        // A single disc prints bare numbers, and vinyl prints sides.
        assert_eq!(disc_of("4"), None);
        assert_eq!(disc_of("A1"), None);
        assert_eq!(disc_of(""), None);
    }

    // The real shape of a three disc set, taken from release 37598709.
    const BOXED_SET: &str = r#"{
      "id": 37598709,
      "title": "Reklamacja'47",
      "artists": [{ "name": "Oki (14)" }],
      "tracklist": [
        { "type_": "heading", "position": "", "title": "CD1" },
        { "type_": "track", "position": "1-1", "title": "My Love" },
        { "type_": "track", "position": "1-2", "title": "Nobodylovesu" },
        { "type_": "heading", "position": "", "title": "CD2" },
        { "type_": "track", "position": "2-1", "title": "Jeszcze Raz?" },
        { "type_": "heading", "position": "", "title": "CD3" },
        { "type_": "track", "position": "3-1", "title": "Znasz Mnie?" },
        { "type_": "track", "position": "3-2", "title": "Goat/Simp" },
        { "type_": "track", "position": "3-3", "title": "Bro" }
      ]
    }"#;

    // Flattening this used to write the first disc's titles onto whichever
    // disc was in the drive.
    #[test]
    fn a_boxed_set_keeps_its_discs_apart() {
        let release: Release = serde_json::from_str(BOXED_SET).expect("the sample parses");
        let candidate = into_candidate(release);

        assert_eq!(candidate.medium_track_counts, vec![2, 1, 3]);
        assert_eq!(candidate.disc_total, Some(3));
        assert_eq!(candidate.media.len(), 3);

        let third = &candidate.media[2];
        assert_eq!(third.position, 3);
        assert_eq!(third.tracks.len(), 3);
        assert_eq!(third.tracks[0].title, "Znasz Mnie?");
        assert_eq!(
            third.tracks[0].number, 1,
            "each disc numbers its own tracks from one"
        );
        assert_eq!(third.tracks[2].number, 3);
    }

    #[test]
    fn switching_disc_replaces_the_tracks_and_says_which_one_it_is() {
        let release: Release = serde_json::from_str(BOXED_SET).expect("the sample parses");
        let mut candidate = into_candidate(release);

        assert_eq!(
            candidate.tracks[0].title, "My Love",
            "the first disc to start"
        );

        candidate.use_medium(3);
        assert_eq!(candidate.disc_number, 3);
        assert_eq!(candidate.disc_total, Some(3));
        assert_eq!(candidate.tracks.len(), 3);
        assert_eq!(candidate.tracks[0].title, "Znasz Mnie?");
    }

    // Trimmed from the answer for release 23525564.
    const RELEASE: &str = r#"{
      "id": 23525564,
      "title": "Produkt47",
      "year": 2022,
      "released": "2022",
      "country": "Poland",
      "artists": [{ "name": "Oki (14)" }],
      "labels": [{ "name": "2020", "catno": "2020-016" }],
      "genres": ["Hip Hop"],
      "styles": [],
      "identifiers": [{ "type": "Barcode", "value": "5 905 279 999 992" }],
      "images": [
        { "type": "secondary", "uri": "https://i.discogs.com/back.jpeg" },
        { "type": "primary", "uri": "https://i.discogs.com/front.jpeg" }
      ],
      "tracklist": [
        { "type_": "heading", "position": "", "title": "Side A", "duration": "" },
        { "type_": "track", "position": "1", "title": "I To Jest Fakt", "duration": "2:56" },
        { "type_": "track", "position": "2", "title": "Perły", "duration": "", "artists": [{ "name": "Guest (3)" }] }
      ]
    }"#;

    #[test]
    fn reads_a_release_out_of_the_service_answer() {
        let release: Release = serde_json::from_str(RELEASE).expect("the sample parses");
        let candidate = into_candidate(release);

        assert_eq!(candidate.artist, "Oki");
        assert_eq!(candidate.title, "Produkt47");
        assert_eq!(candidate.label.as_deref(), Some("2020"));
        assert_eq!(candidate.disambiguation.as_deref(), Some("2020-016"));
        assert_eq!(candidate.genre.as_deref(), Some("Hip Hop"));
        assert_eq!(
            candidate.barcode.as_deref(),
            Some("5905279999992"),
            "barcodes are written with spaces and searched without"
        );
        assert_eq!(
            candidate.cover_art.as_deref(),
            Some("https://i.discogs.com/front.jpeg"),
            "the primary image wins over the one that comes first"
        );

        assert_eq!(candidate.media.len(), 1, "one disc, not one per track");
        assert_eq!(candidate.disc_total, None, "a single disc is not a set");
        assert_eq!(candidate.tracks.len(), 2, "a heading is not a track");
        assert_eq!(candidate.tracks[0].title, "I To Jest Fakt");
        assert_eq!(candidate.tracks[0].length_ms, Some(176_000));
        assert_eq!(candidate.tracks[0].artist, "Oki");
        assert_eq!(candidate.tracks[1].number, 2);
        assert_eq!(candidate.tracks[1].artist, "Guest");
    }
}
