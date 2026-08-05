// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding cover art to choose from.
//!
//! Four services, none of which needs an account. They do not overlap as much
//! as you would hope: the archives only know a release somebody has already
//! catalogued, while the streaming catalogues cover current releases and the
//! regional ones the archives miss entirely. Asking all of them is what makes
//! the difference between a wall of covers and an empty panel.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{MetadataError, SourceId};

const TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);

/// Enough results per service that a wrong guess is still visible, few enough
/// that the panel does not turn into a scrolling contest.
const PER_SERVICE: usize = 12;

/// One cover offered to the user.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Artwork {
    pub source_id: SourceId,
    /// Small enough to show a grid of them without waiting.
    pub thumbnail: String,
    /// What gets embedded once this one is chosen.
    pub full: String,
    /// What the service calls it, when it says: front, back, booklet.
    pub kind: Option<String>,
    /// Pixels, where the service admits to knowing.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// What is known about the disc, for the services that search by name.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub artist: String,
    pub album: String,
    /// MusicBrainz release identifier, when one is in use.
    pub musicbrainz_id: Option<String>,
    /// Discogs release number, when one is in use.
    pub discogs_id: Option<String>,
}

pub struct Artworks {
    client: reqwest::Client,
}

impl Default for Artworks {
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

impl Artworks {
    /// Asks every service at once and returns whatever came back. A service
    /// that fails contributes nothing rather than emptying the panel.
    pub async fn search(&self, query: &Query) -> Vec<Artwork> {
        let (archive, discogs, itunes, deezer) = tokio::join!(
            self.cover_art_archive(query.musicbrainz_id.as_deref()),
            self.discogs(query.discogs_id.as_deref()),
            self.itunes(query),
            self.deezer(query),
        );

        let mut found = Vec::new();
        for batch in [archive, discogs, itunes, deezer] {
            found.extend(batch.unwrap_or_default());
        }

        // The same image reaches us from more than one service often enough to
        // be worth collapsing.
        let mut seen = std::collections::HashSet::new();
        found.retain(|art| seen.insert(art.full.clone()));

        found
    }

    async fn cover_art_archive(
        &self,
        release_id: Option<&str>,
    ) -> Result<Vec<Artwork>, MetadataError> {
        let Some(release_id) = release_id else {
            return Ok(Vec::new());
        };

        let url = format!("https://coverartarchive.org/release/{release_id}");
        // Without this the service answers with a redirect to a web page.
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|_| unreachable(SourceId::CoverArtArchive))?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let payload: ArchiveResponse = response
            .json()
            .await
            .map_err(|_| unreadable(SourceId::CoverArtArchive))?;

        Ok(payload
            .images
            .into_iter()
            .take(PER_SERVICE)
            .map(|image| {
                // The large thumbnail is what gets embedded: the original
                // scan can be several megabytes of booklet.
                let full = image
                    .thumbnails
                    .large
                    .unwrap_or_else(|| image.image.clone());

                Artwork {
                    source_id: SourceId::CoverArtArchive,
                    thumbnail: image.thumbnails.small.unwrap_or_else(|| full.clone()),
                    kind: image.types.first().cloned(),
                    width: None,
                    height: None,
                    full,
                }
            })
            .collect())
    }

    async fn discogs(&self, release_id: Option<&str>) -> Result<Vec<Artwork>, MetadataError> {
        let Some(release_id) = release_id else {
            return Ok(Vec::new());
        };

        let url = format!("https://api.discogs.com/releases/{release_id}");
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| unreachable(SourceId::Discogs))?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let payload: DiscogsRelease = response
            .json()
            .await
            .map_err(|_| unreadable(SourceId::Discogs))?;

        Ok(payload
            .images
            .into_iter()
            .take(PER_SERVICE)
            .filter_map(|image| {
                let full = image.uri?;
                Some(Artwork {
                    thumbnail: image.uri150.unwrap_or_else(|| full.clone()),
                    source_id: SourceId::Discogs,
                    kind: image.kind,
                    width: image.width,
                    height: image.height,
                    full,
                })
            })
            .collect())
    }

    async fn itunes(&self, query: &Query) -> Result<Vec<Artwork>, MetadataError> {
        let term = format!("{} {}", query.artist, query.album);
        if term.trim().is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .client
            .get("https://itunes.apple.com/search")
            .query(&[
                ("term", term.trim()),
                ("entity", "album"),
                ("limit", &PER_SERVICE.to_string()),
            ])
            .send()
            .await
            .map_err(|_| unreachable(SourceId::Itunes))?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let payload: ItunesResponse = response
            .json()
            .await
            .map_err(|_| unreadable(SourceId::Itunes))?;

        Ok(payload
            .results
            .into_iter()
            .filter_map(|album| album.artwork_url100)
            .map(|thumbnail| Artwork {
                source_id: SourceId::Itunes,
                full: itunes_full_size(&thumbnail),
                thumbnail,
                kind: None,
                width: None,
                height: None,
            })
            .collect())
    }

    async fn deezer(&self, query: &Query) -> Result<Vec<Artwork>, MetadataError> {
        let term = format!("{} {}", query.artist, query.album);
        if term.trim().is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .client
            .get("https://api.deezer.com/search/album")
            .query(&[("q", term.trim()), ("limit", &PER_SERVICE.to_string())])
            .send()
            .await
            .map_err(|_| unreachable(SourceId::Deezer))?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let payload: DeezerResponse = response
            .json()
            .await
            .map_err(|_| unreadable(SourceId::Deezer))?;

        Ok(payload
            .data
            .into_iter()
            .filter_map(|album| {
                let full = album.cover_xl.or_else(|| album.cover_big.clone())?;
                Some(Artwork {
                    source_id: SourceId::Deezer,
                    thumbnail: album.cover_medium.unwrap_or_else(|| full.clone()),
                    kind: None,
                    width: None,
                    height: None,
                    full,
                })
            })
            .collect())
    }
}

/// iTunes hands out a thumbnail and encodes the size in the file name, so the
/// full size is a substitution rather than another request.
fn itunes_full_size(thumbnail: &str) -> String {
    thumbnail.replace("100x100bb", "1000x1000bb")
}

fn unreachable(source_id: SourceId) -> MetadataError {
    MetadataError::Unreachable { source_id }
}

fn unreadable(source_id: SourceId) -> MetadataError {
    MetadataError::Unreadable { source_id }
}

#[derive(Deserialize)]
struct ArchiveResponse {
    #[serde(default)]
    images: Vec<ArchiveImage>,
}

#[derive(Deserialize)]
struct ArchiveImage {
    image: String,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    thumbnails: ArchiveThumbnails,
}

#[derive(Deserialize, Default)]
struct ArchiveThumbnails {
    large: Option<String>,
    small: Option<String>,
}

#[derive(Deserialize)]
struct DiscogsRelease {
    #[serde(default)]
    images: Vec<DiscogsImage>,
}

#[derive(Deserialize)]
struct DiscogsImage {
    #[serde(rename = "type")]
    kind: Option<String>,
    uri: Option<String>,
    uri150: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Deserialize)]
struct ItunesResponse {
    #[serde(default)]
    results: Vec<ItunesAlbum>,
}

#[derive(Deserialize)]
struct ItunesAlbum {
    #[serde(rename = "artworkUrl100")]
    artwork_url100: Option<String>,
}

#[derive(Deserialize)]
struct DeezerResponse {
    #[serde(default)]
    data: Vec<DeezerAlbum>,
}

#[derive(Deserialize)]
struct DeezerAlbum {
    cover_medium: Option<String>,
    cover_big: Option<String>,
    cover_xl: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_itunes_for_the_large_version_of_its_thumbnail() {
        assert_eq!(
            itunes_full_size(
                "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/dj.jpg/100x100bb.jpg"
            ),
            "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/dj.jpg/1000x1000bb.jpg"
        );
    }

    #[test]
    fn leaves_an_address_without_a_size_alone() {
        let plain = "https://example.invalid/cover.jpg";
        assert_eq!(itunes_full_size(plain), plain);
    }

    #[test]
    fn reads_the_archive_listing() {
        let body = r#"{"images":[
          {"image":"http://coverartarchive.org/release/x/1.jpg","front":true,
           "types":["Front"],
           "thumbnails":{"large":"http://coverartarchive.org/release/x/1-500.jpg",
                         "small":"http://coverartarchive.org/release/x/1-250.jpg"}}
        ]}"#;

        let payload: ArchiveResponse = serde_json::from_str(body).expect("the sample parses");
        assert_eq!(payload.images.len(), 1);
        assert_eq!(payload.images[0].types, ["Front"]);
        assert!(payload.images[0].thumbnails.small.is_some());
    }

    #[test]
    fn reads_the_deezer_listing() {
        let body = r#"{"data":[
          {"cover_medium":"https://cdn-images.dzcdn.net/m.jpg",
           "cover_big":"https://cdn-images.dzcdn.net/b.jpg",
           "cover_xl":"https://cdn-images.dzcdn.net/xl.jpg"}
        ]}"#;

        let payload: DeezerResponse = serde_json::from_str(body).expect("the sample parses");
        assert_eq!(payload.data.len(), 1);
        assert_eq!(
            payload.data[0].cover_xl.as_deref(),
            Some("https://cdn-images.dzcdn.net/xl.jpg")
        );
    }
}
