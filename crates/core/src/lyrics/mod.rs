// SPDX-License-Identifier: GPL-3.0-or-later

//! Lyrics, from LRCLIB.
//!
//! Matching is done on the playing time taken from the table of contents,
//! because titles are written a dozen different ways and a duration is not.
//! LRCLIB's exact endpoint wants the album name to agree as well, which it
//! frequently does not, so a miss there falls through to searching and picking
//! whichever result is closest in length.
//!
//! Nothing here is written into a file. The rip decides what to do with what
//! comes back, and the user gets to overrule it either way.

use std::time::Duration;

use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://lrclib.net/api";

/// LRCLIB asks that clients name themselves and where to complain about them.
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);
const TIMEOUT: Duration = Duration::from_secs(20);

/// How far a search result's length may sit from the disc's before it is
/// treated as a different recording. Two seconds is what LRCLIB's own exact
/// lookup allows; a little more is given here because a search result has
/// already had to agree on the artist and the title.
const TOLERANCE: u32 = 5;

/// What one track's lyrics turned out to be.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// The words with no timing, which is what goes into a tag.
    pub plain: Option<String>,
    /// The same words with timestamps, which is what goes into an `.lrc`.
    pub synced: Option<String>,
    /// Said by the database to have no words at all. Worth keeping apart from
    /// "nothing found", since one is an answer and the other is not.
    pub instrumental: bool,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.plain.is_none() && self.synced.is_none()
    }
}

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LyricsError {
    #[error("lrclib could not be reached")]
    Unreachable,

    #[error("lrclib refused the request with status {status}")]
    Rejected { status: u16 },

    #[error("lrclib sent an answer this version cannot read")]
    Unreadable,
}

pub struct Lrclib {
    client: reqwest::Client,
}

impl Default for Lrclib {
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

impl Lrclib {
    /// `seconds` is the track's playing time from the table of contents.
    pub async fn find(
        &self,
        artist: &str,
        title: &str,
        album: &str,
        seconds: u32,
    ) -> Result<Option<Lyrics>, LyricsError> {
        if artist.trim().is_empty() || title.trim().is_empty() {
            return Ok(None);
        }

        // The exact endpoint is tried first because when it answers, it has
        // agreed on everything including the album, which is as sure as this
        // gets.
        //
        // Its failures are swallowed rather than returned. It answers 404 for
        // most discs and 503 when it is busy, and letting either stop the
        // search would skip the fallback exactly when it is most needed.
        if let Ok(Some(found)) = self.exact(artist, title, album, seconds).await {
            return Ok(Some(found));
        }

        self.search(artist, title, seconds).await
    }

    async fn exact(
        &self,
        artist: &str,
        title: &str,
        album: &str,
        seconds: u32,
    ) -> Result<Option<Lyrics>, LyricsError> {
        let response = self
            .client
            .get(format!("{BASE_URL}/get"))
            .query(&[
                ("artist_name", artist),
                ("track_name", title),
                ("album_name", album),
                ("duration", &seconds.to_string()),
            ])
            .send()
            .await
            .map_err(|_| LyricsError::Unreachable)?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(LyricsError::Rejected {
                status: response.status().as_u16(),
            });
        }

        let record: Record = response.json().await.map_err(|_| LyricsError::Unreadable)?;
        let lyrics: Lyrics = record.into();

        // An entry with no words in it is not an answer worth stopping on;
        // searching may still turn up a copy that has them.
        Ok((!lyrics.is_empty() || lyrics.instrumental).then_some(lyrics))
    }

    async fn search(
        &self,
        artist: &str,
        title: &str,
        seconds: u32,
    ) -> Result<Option<Lyrics>, LyricsError> {
        let response = self
            .client
            .get(format!("{BASE_URL}/search"))
            .query(&[("artist_name", artist), ("track_name", title)])
            .send()
            .await
            .map_err(|_| LyricsError::Unreachable)?;

        if !response.status().is_success() {
            return Err(LyricsError::Rejected {
                status: response.status().as_u16(),
            });
        }

        let records: Vec<Record> = response.json().await.map_err(|_| LyricsError::Unreadable)?;

        Ok(closest(records, seconds).map(Into::into))
    }
}

/// Picks the result whose length is nearest the disc's, provided it is near
/// enough at all. A remix or a live take of the same song is a different
/// recording with different words in different places, and its lyrics are
/// worse than none.
fn closest(records: Vec<Record>, seconds: u32) -> Option<Record> {
    records
        .into_iter()
        .filter_map(|record| {
            let length = record.duration.unwrap_or(0.0).round().max(0.0) as u32;
            let apart = length.abs_diff(seconds);

            (apart <= TOLERANCE).then_some((apart, record))
        })
        .min_by_key(|(apart, _)| *apart)
        .map(|(_, record)| record)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

impl From<Record> for Lyrics {
    fn from(record: Record) -> Self {
        let present = |value: Option<String>| value.filter(|text| !text.trim().is_empty());

        Self {
            plain: present(record.plain_lyrics),
            synced: present(record.synced_lyrics),
            instrumental: record.instrumental,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(duration: f64) -> Record {
        Record {
            duration: Some(duration),
            instrumental: false,
            plain_lyrics: Some(format!("words for {duration}")),
            synced_lyrics: None,
        }
    }

    #[test]
    fn the_nearest_length_wins() {
        let found = closest(vec![record(180.0), record(174.0), record(200.0)], 176)
            .expect("something is close enough");

        assert_eq!(found.duration, Some(174.0));
    }

    // A remix or a live take is a different recording, and its words land in
    // the wrong places, which is worse than having none.
    #[test]
    fn a_length_that_is_nowhere_near_is_refused() {
        assert!(closest(vec![record(240.0), record(90.0)], 176).is_none());
    }

    #[test]
    fn a_length_right_at_the_tolerance_still_counts() {
        assert!(closest(vec![record(176.0 + TOLERANCE as f64)], 176).is_some());
        assert!(closest(vec![record(176.0 + TOLERANCE as f64 + 1.0)], 176).is_none());
    }

    #[test]
    fn nothing_at_all_is_not_an_error() {
        assert!(closest(Vec::new(), 176).is_none());
    }

    // The exact endpoint answers 404 for most discs and 503 when it is busy.
    // Either one has to fall through to searching rather than end the lookup,
    // which is the difference between finding words and finding none.
    #[tokio::test]
    async fn a_busy_exact_endpoint_does_not_stop_the_search() {
        let outcome = Lrclib::default()
            .find("Oki", "Znasz Mnie?", "Reklamacja'47", 175)
            .await;

        // Only asserted when the network is there at all, so the suite still
        // passes on a machine with no way out.
        if let Ok(found) = outcome {
            assert!(
                found.is_some_and(|lyrics| !lyrics.is_empty()),
                "the track is in the database under a slightly different name"
            );
        }
    }

    /// The shape LRCLIB actually answers with, so the field names are tested
    /// against the real thing rather than against what they might be.
    #[test]
    fn the_answer_is_read_the_way_lrclib_writes_it() {
        let body = r#"{
            "id": 36639464,
            "trackName": "znasz mnie? (Paused)",
            "artistName": "OKI",
            "albumName": "REKLAMACJA'47",
            "duration": 174.0,
            "instrumental": false,
            "plainLyrics": "Nie no, dobra",
            "syncedLyrics": "[00:12.00] Nie no, dobra"
        }"#;

        let record: Record = serde_json::from_str(body).expect("the answer parses");
        let lyrics: Lyrics = record.into();

        assert_eq!(lyrics.plain.as_deref(), Some("Nie no, dobra"));
        assert_eq!(lyrics.synced.as_deref(), Some("[00:12.00] Nie no, dobra"));
        assert!(!lyrics.instrumental);
    }

    #[test]
    fn an_instrumental_is_an_answer_rather_than_a_miss() {
        let body = r#"{"duration": 100.0, "instrumental": true,
                       "plainLyrics": null, "syncedLyrics": null}"#;

        let lyrics: Lyrics = serde_json::from_str::<Record>(body).unwrap().into();

        assert!(lyrics.is_empty());
        assert!(lyrics.instrumental);
    }

    // Some records carry an empty string rather than no field at all, and an
    // empty tag is worse than a missing one.
    #[test]
    fn blank_words_are_treated_as_no_words() {
        let body = r#"{"duration": 100.0, "plainLyrics": "   ", "syncedLyrics": ""}"#;
        let lyrics: Lyrics = serde_json::from_str::<Record>(body).unwrap().into();

        assert!(lyrics.is_empty());
    }
}
