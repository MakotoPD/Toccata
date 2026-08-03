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
        self.client
            .get(url)
            .query(&[
                ("fmt", "json"),
                ("inc", "recordings+artist-credits+release-groups+labels"),
            ])
            .send()
            .await
            .map_err(|_| MetadataError::Unreachable {
                source_id: SourceId::MusicBrainz,
            })
    }
}

impl MetadataSource for MusicBrainz {
    fn id(&self) -> SourceId {
        SourceId::MusicBrainz
    }

    fn lookup<'a>(&'a self, toc: &'a Toc) -> Lookup<'a> {
        Box::pin(async move {
            let disc_id = toc.musicbrainz_disc_id();
            let url = format!("{BASE_URL}/discid/{disc_id}");

            let mut response = self.get(&url).await?;

            // The rate limiter answers 503 and expects the caller to wait,
            // which is worth one attempt before giving up on the disc.
            if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                tokio::time::sleep(RETRY_AFTER).await;
                response = self.get(&url).await?;
            }

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

            Ok(payload.releases.into_iter().map(into_candidate).collect())
        })
    }
}

fn into_candidate(release: Release) -> ReleaseCandidate {
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
        disc_number: medium.map_or(1, |medium| medium.position),
        disc_total: None,
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
        let candidates: Vec<ReleaseCandidate> =
            payload.releases.into_iter().map(into_candidate).collect();

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
    fn an_answer_with_no_releases_yields_no_candidates() {
        let payload: DiscIdResponse = serde_json::from_str(r#"{"releases": []}"#).unwrap();
        assert!(payload.releases.is_empty());
    }
}
