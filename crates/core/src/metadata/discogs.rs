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

use super::{MetadataError, ReleaseCandidate, SourceId, TrackMetadata};

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
}

impl Default for Discogs {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(TIMEOUT)
                .build()
                .expect("the http client has no configuration that can fail"),
        }
    }
}

impl Discogs {
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
            .client
            .get(format!("{BASE_URL}/database/search"))
            .query(query)
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
            .client
            .get(format!("{BASE_URL}/releases/{id}"))
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
        cover_art: result.cover_image.filter(|url| url.starts_with("http")),
        tracks: Vec::new(),
    }
}

fn into_candidate(release: Release) -> ReleaseCandidate {
    let artist = release
        .artists
        .first()
        .map(|artist| clean_artist(&artist.name))
        .unwrap_or_default();

    // Sub-headings and index tracks have no position of their own; only real
    // tracks are numbered, and they are numbered by their order on the disc.
    let tracks: Vec<TrackMetadata> = release
        .tracklist
        .iter()
        .filter(|track| track.track_type.as_deref().unwrap_or("track") == "track")
        .enumerate()
        .map(|(index, track)| TrackMetadata {
            number: index as u8 + 1,
            title: track.title.trim().to_owned(),
            artist: track
                .artists
                .first()
                .map(|artist| clean_artist(&artist.name))
                .unwrap_or_else(|| artist.clone()),
            length_ms: parse_duration(&track.duration),
        })
        .collect();

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
        disc_number: 1,
        disc_total: None,
        medium_track_counts: vec![tracks.len() as u32],
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
        { "type_": "heading", "title": "Side A", "duration": "" },
        { "type_": "track", "title": "I To Jest Fakt", "duration": "2:56" },
        { "type_": "track", "title": "Perły", "duration": "", "artists": [{ "name": "Guest (3)" }] }
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

        assert_eq!(candidate.tracks.len(), 2, "a heading is not a track");
        assert_eq!(candidate.tracks[0].title, "I To Jest Fakt");
        assert_eq!(candidate.tracks[0].length_ms, Some(176_000));
        assert_eq!(candidate.tracks[0].artist, "Oki");
        assert_eq!(candidate.tracks[1].number, 2);
        assert_eq!(candidate.tracks[1].artist, "Guest");
    }
}
