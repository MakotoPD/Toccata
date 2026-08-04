// SPDX-License-Identifier: GPL-3.0-or-later

//! MusicBrainz web service, queried by Disc ID.
//!
//! The service asks for a User-Agent that identifies the application and
//! limits callers to roughly one request a second. Both are respected here:
//! anonymous or rude clients get blocked, and a blocked client looks exactly
//! like a disc nobody has ever catalogued.

use std::time::Duration;

use serde::Deserialize;

use super::{
    Lookup, MetadataError, MetadataSource, ReleaseCandidate, SourceId, TrackMetadata, cover,
};
use crate::toc::Toc;

const BASE_URL: &str = "https://musicbrainz.org/ws/2";
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);
const TIMEOUT: Duration = Duration::from_secs(15);

/// The service allows roughly one request a second, so a rejected call is
/// worth repeating once after that window has passed.
const RETRY_AFTER: Duration = Duration::from_millis(1500);

pub struct MusicBrainz {
    client: reqwest::Client,
}

impl Default for MusicBrainz {
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

impl MusicBrainz {
    async fn get(&self, url: &str) -> Result<reqwest::Response, MetadataError> {
        self.request(
            url,
            &[
                ("fmt", "json"),
                ("inc", "recordings+artist-credits+release-groups+labels"),
            ],
        )
        .await
    }

    async fn request(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, MetadataError> {
        let send = || self.client.get(url).query(query).send();

        let mut response = send().await.map_err(|_| MetadataError::Unreachable {
            source_id: SourceId::MusicBrainz,
        })?;

        // The rate limiter answers 503 and expects the caller to wait, which
        // is worth one attempt before giving up.
        if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            tokio::time::sleep(RETRY_AFTER).await;
            response = send().await.map_err(|_| MetadataError::Unreachable {
                source_id: SourceId::MusicBrainz,
            })?;
        }

        Ok(response)
    }

    /// Free text search, for the discs no identifier reaches. Results carry no
    /// tracks: the service does not return them for a search, so only the
    /// number of tracks per disc can be lined up against the table of contents
    /// until one is picked and fetched in full.
    pub async fn search(
        &self,
        artist: &str,
        title: &str,
        barcode: &str,
    ) -> Result<Vec<ReleaseCandidate>, MetadataError> {
        let query = lucene_query(artist, title, barcode);
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{BASE_URL}/release");
        let response = self
            .request(&url, &[("fmt", "json"), ("limit", "25"), ("query", &query)])
            .await?;

        if !response.status().is_success() {
            return Err(MetadataError::Rejected {
                source_id: SourceId::MusicBrainz,
                status: response.status().as_u16(),
            });
        }

        let payload: SearchResponse =
            response
                .json()
                .await
                .map_err(|_| MetadataError::Unreadable {
                    source_id: SourceId::MusicBrainz,
                })?;

        Ok(payload
            .releases
            .into_iter()
            .map(|release| into_candidate(release, true))
            .collect())
    }

    /// One release in full, which is what turns a search hit into something
    /// worth tagging with.
    pub async fn release(&self, id: &str) -> Result<Option<ReleaseCandidate>, MetadataError> {
        let Some(id) = release_id_from(id) else {
            return Ok(None);
        };

        let url = format!("{BASE_URL}/release/{id}");
        let response = self.get(&url).await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(MetadataError::Rejected {
                source_id: SourceId::MusicBrainz,
                status: response.status().as_u16(),
            });
        }

        let release: Release = response
            .json()
            .await
            .map_err(|_| MetadataError::Unreadable {
                source_id: SourceId::MusicBrainz,
            })?;

        Ok(Some(into_candidate(release, true)))
    }
}

/// Pulls a release identifier out of whatever the user pasted: the bare
/// identifier, a musicbrainz.org address, or either with something around it.
pub fn release_id_from(input: &str) -> Option<String> {
    fn is_identifier(candidate: &str) -> bool {
        let groups: Vec<&str> = candidate.split('-').collect();

        groups.len() == 5
            && [8, 4, 4, 4, 12].iter().zip(&groups).all(|(length, group)| {
                group.len() == *length && group.chars().all(|c| c.is_ascii_hexdigit())
            })
    }

    input
        .split(['/', '?', '#', ' ', '\t'])
        .map(str::trim)
        .find(|part| is_identifier(&part.to_ascii_lowercase()))
        .map(|found| found.to_ascii_lowercase())
}

/// Builds the Lucene query the search endpoint expects. User input is quoted
/// and its own quotes escaped, so a stray character cannot rewrite the query.
fn lucene_query(artist: &str, title: &str, barcode: &str) -> String {
    fn term(field: &str, value: &str) -> Option<String> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        let escaped = value.replace('\\', r"\\").replace('"', "\\\"");
        Some(format!("{field}:\"{escaped}\""))
    }

    // A barcode identifies the physical pressing on its own, so when there is
    // one it stands alone rather than being narrowed by a guessed spelling.
    if let Some(barcode) = term("barcode", barcode) {
        return barcode;
    }

    [term("release", title), term("artist", artist)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" AND ")
}

impl MetadataSource for MusicBrainz {
    fn id(&self) -> SourceId {
        SourceId::MusicBrainz
    }

    fn lookup<'a>(&'a self, toc: &'a Toc) -> Lookup<'a> {
        Box::pin(async move {
            let disc_id = toc.musicbrainz_disc_id();
            let url = format!("{BASE_URL}/discid/{disc_id}");

            let response = self.get(&url).await?;

            // An unknown disc is a plain 404 and simply means no candidates.
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(Vec::new());
            }

            if !response.status().is_success() {
                return Err(MetadataError::Rejected {
                    source_id: SourceId::MusicBrainz,
                    status: response.status().as_u16(),
                });
            }

            let payload: DiscIdResponse =
                response
                    .json()
                    .await
                    .map_err(|_| MetadataError::Unreadable {
                        source_id: SourceId::MusicBrainz,
                    })?;

            Ok(payload
                .releases
                .into_iter()
                .map(|release| into_candidate(release, false))
                .collect())
        })
    }
}

/// `every_medium` says whether `media` lists the whole release. A lookup by
/// Disc ID narrows it to the one medium that matched, and calling that a
/// one-disc release would be wrong.
fn into_candidate(release: Release, every_medium: bool) -> ReleaseCandidate {
    let cover_art_id = release.id.clone();

    // Looking up by Disc ID narrows `media` to the medium this disc actually
    // is, so its position is the disc number.
    let medium = release.media.first();

    ReleaseCandidate {
        source_id: SourceId::MusicBrainz,
        relayed_from: None,
        id: release.id,
        title: release.title,
        artist: join_credits(&release.artist_credit),
        date: release.date,
        country: release.country,
        label: release
            .label_info
            .iter()
            .find_map(|info| info.label.as_ref().map(|label| label.name.clone())),
        barcode: release.barcode.filter(|value| !value.is_empty()),
        disambiguation: release.disambiguation.filter(|value| !value.is_empty()),
        genre: None,
        style: None,
        composer: None,
        comment: None,
        compilation: false,
        disc_number: medium.map_or(1, |medium| medium.position),
        disc_total: every_medium.then_some(release.media.len() as u32),
        medium_track_counts: release
            .media
            .iter()
            .map(|medium| medium.track_count.unwrap_or(medium.tracks.len() as u32))
            .collect(),
        // The release says whether the archive holds a front cover, which
        // saves asking for one that is not there.
        cover_art: release
            .cover_art_archive
            .filter(|archive| archive.front)
            .map(|_| cover::archive_url(&cover_art_id)),
        tracks: medium
            .map(|medium| {
                medium
                    .tracks
                    .iter()
                    .map(|track| TrackMetadata {
                        number: track.position,
                        title: track.title.clone(),
                        artist: join_credits(&track.artist_credit),
                        length_ms: track.length,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// MusicBrainz splits collaborations into credits carrying their own join
/// phrases, so "A feat. B" round-trips exactly as the editors entered it.
fn join_credits(credits: &[ArtistCredit]) -> String {
    credits
        .iter()
        .map(|credit| {
            format!(
                "{}{}",
                credit.name,
                credit.joinphrase.as_deref().unwrap_or("")
            )
        })
        .collect()
}

#[derive(Deserialize)]
struct DiscIdResponse {
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct Release {
    id: String,
    title: String,
    date: Option<String>,
    country: Option<String>,
    barcode: Option<String>,
    disambiguation: Option<String>,
    #[serde(default, rename = "artist-credit")]
    artist_credit: Vec<ArtistCredit>,
    #[serde(default, rename = "label-info")]
    label_info: Vec<LabelInfo>,
    #[serde(default)]
    media: Vec<Medium>,
    #[serde(rename = "cover-art-archive")]
    cover_art_archive: Option<CoverArtArchive>,
}

#[derive(Deserialize, Clone, Copy)]
struct CoverArtArchive {
    #[serde(default)]
    front: bool,
}

#[derive(Deserialize)]
struct ArtistCredit {
    name: String,
    joinphrase: Option<String>,
}

#[derive(Deserialize)]
struct LabelInfo {
    label: Option<Label>,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
struct Medium {
    #[serde(default = "one")]
    position: u32,
    /// Present on search hits, which carry no tracks of their own.
    #[serde(rename = "track-count")]
    track_count: Option<u32>,
    #[serde(default)]
    tracks: Vec<MediumTrack>,
}

#[derive(Deserialize)]
struct MediumTrack {
    position: u8,
    title: String,
    length: Option<u64>,
    #[serde(default, rename = "artist-credit")]
    artist_credit: Vec<ArtistCredit>,
}

fn one() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real answer for disc ID xUp1F2NkfP8s8jaeFn_Av3jNEI4-.
    const SAMPLE: &str = r#"{
      "releases": [
        {
          "id": "1e5f2b4e-0000-4000-8000-000000000001",
          "title": "Hello Nasty",
          "date": "1998-07-14",
          "country": "US",
          "barcode": "724384524623",
          "disambiguation": "",
          "artist-credit": [{ "name": "Beastie Boys", "joinphrase": "" }],
          "label-info": [{ "label": { "name": "Grand Royal" } }],
          "media": [
            {
              "position": 2,
              "tracks": [
                {
                  "position": 1,
                  "title": "Super Disco Breakin'",
                  "length": 121533,
                  "artist-credit": [{ "name": "Beastie Boys", "joinphrase": "" }]
                },
                {
                  "position": 2,
                  "title": "The Move",
                  "length": 178226,
                  "artist-credit": [
                    { "name": "Beastie Boys", "joinphrase": " feat. " },
                    { "name": "Mix Master Mike", "joinphrase": "" }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }"#;

    #[test]
    fn reads_a_release_out_of_the_service_answer() {
        let payload: DiscIdResponse = serde_json::from_str(SAMPLE).expect("sample parses");
        let candidates: Vec<ReleaseCandidate> = payload
            .releases
            .into_iter()
            .map(|release| into_candidate(release, false))
            .collect();

        assert_eq!(candidates.len(), 1);
        let release = &candidates[0];

        assert_eq!(release.title, "Hello Nasty");
        assert_eq!(release.artist, "Beastie Boys");
        assert_eq!(release.label.as_deref(), Some("Grand Royal"));
        assert_eq!(release.barcode.as_deref(), Some("724384524623"));
        assert_eq!(release.disambiguation, None);
        assert_eq!(release.disc_number, 2);
        assert_eq!(release.tracks.len(), 2);
        assert_eq!(release.tracks[1].title, "The Move");
        assert_eq!(
            release.tracks[1].artist,
            "Beastie Boys feat. Mix Master Mike"
        );
        assert_eq!(release.tracks[1].length_ms, Some(178226));
    }

    #[test]
    fn finds_a_release_identifier_in_whatever_was_pasted() {
        let expected = Some("44d66a09-491a-3143-89f2-dd7232424325".to_owned());

        assert_eq!(
            release_id_from("44d66a09-491a-3143-89f2-dd7232424325"),
            expected
        );
        assert_eq!(
            release_id_from("https://musicbrainz.org/release/44d66a09-491a-3143-89f2-dd7232424325"),
            expected
        );
        assert_eq!(
            release_id_from(
                "  https://musicbrainz.org/release/44D66A09-491A-3143-89F2-DD7232424325/cover-art  "
            ),
            expected
        );
    }

    #[test]
    fn refuses_anything_that_is_not_a_release_identifier() {
        assert_eq!(release_id_from(""), None);
        assert_eq!(release_id_from("Hello Nasty"), None);
        assert_eq!(release_id_from("https://musicbrainz.org/release/"), None);
        // Right shape, wrong alphabet.
        assert_eq!(
            release_id_from("zzzzzzzz-491a-3143-89f2-dd7232424325"),
            None
        );
    }

    #[test]
    fn quotes_search_terms_so_they_cannot_rewrite_the_query() {
        assert_eq!(
            lucene_query("Beastie Boys", "Hello Nasty", ""),
            r#"release:"Hello Nasty" AND artist:"Beastie Boys""#
        );
        assert_eq!(
            lucene_query("", "Hello Nasty", ""),
            r#"release:"Hello Nasty""#
        );
        assert_eq!(lucene_query("  ", "  ", " "), "");
        assert_eq!(
            lucene_query("", r#"a" OR release:"b"#, ""),
            r#"release:"a\" OR release:\"b""#
        );
    }

    #[test]
    fn a_barcode_identifies_the_pressing_by_itself() {
        assert_eq!(
            lucene_query("Beastie Boys", "Hello Nasty", "724349572324"),
            r#"barcode:"724349572324""#
        );
    }

    #[test]
    fn an_answer_with_no_releases_yields_no_candidates() {
        let payload: DiscIdResponse = serde_json::from_str(r#"{"releases": []}"#).unwrap();
        assert!(payload.releases.is_empty());
    }
}
