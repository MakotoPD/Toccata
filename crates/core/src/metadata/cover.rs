// SPDX-License-Identifier: GPL-3.0-or-later

//! Cover art, fetched here rather than by the webview.
//!
//! Pulling the image in the backend keeps the window's content policy shut to
//! the open internet, and the bytes are wanted anyway once they have to be
//! embedded in the tags.

use std::time::Duration;

use super::{MetadataError, SourceId};
use crate::base64;

const COVER_ART_ARCHIVE: &str = "https://coverartarchive.org";
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);
const TIMEOUT: Duration = Duration::from_secs(30);

/// Big enough to look right on a high density screen, small enough that the
/// whole thing crosses the process boundary in one go.
const THUMBNAIL: &str = "front-500";

/// Refuses anything larger than a cover has any business being, since the URL
/// can come from a third party database.
const SIZE_LIMIT: usize = 8 * 1024 * 1024;

/// Front cover of a MusicBrainz release, if the archive holds one. Whether it
/// does is already in the release data, so this is only called when it says so.
pub fn archive_url(release_id: &str) -> String {
    format!("{COVER_ART_ARCHIVE}/release/{release_id}/{THUMBNAIL}")
}

pub struct Covers {
    client: reqwest::Client,
}

impl Default for Covers {
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

/// Hosts an image may be fetched from.
///
/// The URL reaches this crate by way of a third party database and the frontend,
/// so it is never handed to the HTTP client unchecked. Each art source added
/// later brings its own host onto this list.
const ALLOWED_HOSTS: [&str; 3] = ["coverartarchive.org", "archive.org", "discogs.com"];

fn is_allowed(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };

    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();

    ALLOWED_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

impl Covers {
    /// Returns a data URI, or nothing at all when the archive has no image for
    /// this release. A missing cover is not an error.
    pub async fn fetch(&self, url: &str) -> Result<Option<String>, MetadataError> {
        if !is_allowed(url) {
            return Ok(None);
        }

        let response =
            self.client
                .get(url)
                .send()
                .await
                .map_err(|_| MetadataError::Unreachable {
                    source_id: SourceId::CoverArtArchive,
                })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(MetadataError::Rejected {
                source_id: SourceId::CoverArtArchive,
                status: response.status().as_u16(),
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|_| MetadataError::Unreadable {
                source_id: SourceId::CoverArtArchive,
            })?;

        if bytes.len() > SIZE_LIMIT {
            return Err(MetadataError::Unreadable {
                source_id: SourceId::CoverArtArchive,
            });
        }

        let Some(mime) = image_type(&bytes) else {
            return Err(MetadataError::Unreadable {
                source_id: SourceId::CoverArtArchive,
            });
        };

        Ok(Some(format!(
            "data:{mime};base64,{}",
            base64::encode(&bytes, base64::STANDARD, '=')
        )))
    }
}

/// Sniffs the format from the leading bytes instead of trusting the header a
/// redirect chain happened to end on.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const GIF: &[u8] = b"GIF8";

    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(PNG) {
        Some("image/png")
    } else if bytes.starts_with(GIF) {
        Some("image/gif")
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_archive_url_from_a_release_id() {
        assert_eq!(
            archive_url("44d66a09-491a-3143-89f2-dd7232424325"),
            "https://coverartarchive.org/release/44d66a09-491a-3143-89f2-dd7232424325/front-500"
        );
    }

    #[test]
    fn recognises_the_formats_the_archive_serves() {
        assert_eq!(image_type(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_type(b"GIF89a"), Some("image/gif"));
        assert_eq!(image_type(b"RIFF____WEBPVP8 "), Some("image/webp"));
    }

    #[test]
    fn refuses_bytes_that_are_not_an_image() {
        assert_eq!(image_type(b"<!DOCTYPE html>"), None);
        assert_eq!(image_type(b""), None);
    }

    #[test]
    fn accepts_the_hosts_art_actually_comes_from() {
        assert!(is_allowed(
            "https://coverartarchive.org/release/x/front-500"
        ));
        assert!(is_allowed("http://coverartarchive.org/release/x/front"));
        assert!(is_allowed("https://dn710309.ca.archive.org/0/items/x.jpg"));
        assert!(is_allowed("https://i.discogs.com/abc/front.jpeg"));
    }

    #[test]
    fn refuses_anywhere_else() {
        assert!(!is_allowed("https://example.com/cover.jpg"));
        assert!(!is_allowed("file:///etc/passwd"));
        assert!(!is_allowed("http://127.0.0.1:8080/"));
        // The host is what precedes the first slash, not what follows it.
        assert!(!is_allowed("https://example.com/coverartarchive.org/x.jpg"));
        // Userinfo and ports do not smuggle a different host past the check.
        assert!(!is_allowed(
            "https://coverartarchive.org@evil.example/x.jpg"
        ));
        assert!(!is_allowed("https://notcoverartarchive.org/x.jpg"));
    }
}
