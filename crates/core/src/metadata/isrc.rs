// SPDX-License-Identifier: GPL-3.0-or-later

//! The disc's recordings, looked up on MusicBrainz.
//!
//! An ISRC names a recording rather than a pressing, which is what makes this
//! worth trying: a disc nobody has submitted a Disc ID for is still full of
//! songs that are catalogued. Ask what releases each recording appears on and
//! the release that keeps coming back is very probably the one in the drive.
//!
//! Most discs carry no ISRCs at all. Those that do are usually recent, and are
//! exactly the pressings a Disc ID lookup is most likely to miss.

use std::collections::HashMap;
use std::time::Duration;

use super::{Disc, Lookup, MetadataError, MetadataSource, SourceId, musicbrainz::MusicBrainz};

const BASE_URL: &str = "https://musicbrainz.org/ws/2";
const NAMESPACE: &str = "http://musicbrainz.org/ns/mmd-2.0#";
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);
const TIMEOUT: Duration = Duration::from_secs(20);

/// How many of the disc's identifiers to ask about. Enough for one release to
/// pull clearly ahead of the compilations that share a track or two with it,
/// and few enough to stay polite: MusicBrainz asks for one request a second.
const ASKED: usize = 4;

pub struct Isrc {
    client: reqwest::Client,
    releases: MusicBrainz,
}

impl Default for Isrc {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(TIMEOUT)
                .build()
                .expect("the http client has no configuration that can fail"),
            releases: MusicBrainz::default(),
        }
    }
}

impl MetadataSource for Isrc {
    fn id(&self) -> SourceId {
        SourceId::MusicBrainz
    }

    fn lookup<'a>(&'a self, disc: &'a Disc) -> Lookup<'a> {
        Box::pin(async move {
            if disc.isrcs.is_empty() {
                return Ok(Vec::new());
            }

            // In track order, so the same disc always asks the same questions
            // and a failure is reproducible.
            let mut identifiers: Vec<(&u8, &String)> = disc.isrcs.iter().collect();
            identifiers.sort_by_key(|(number, _)| **number);

            let mut votes: HashMap<String, usize> = HashMap::new();

            for (_, isrc) in identifiers.iter().take(ASKED) {
                // One identifier failing is not a reason to abandon the rest.
                if let Ok(releases) = self.releases_of(isrc).await {
                    for release in releases {
                        *votes.entry(release).or_default() += 1;
                    }
                }
            }

            let Some(best) = pick(&votes, disc.toc.tracks.len()) else {
                return Ok(Vec::new());
            };

            match self.releases.release(&best).await? {
                Some(release) => Ok(vec![release]),
                None => Ok(Vec::new()),
            }
        })
    }
}

impl Isrc {
    async fn releases_of(&self, isrc: &str) -> Result<Vec<String>, MetadataError> {
        let response = self
            .client
            .get(format!("{BASE_URL}/isrc/{isrc}"))
            .query(&[("inc", "releases")])
            .send()
            .await
            .map_err(|_| MetadataError::Unreachable {
                source_id: SourceId::MusicBrainz,
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }

        if !response.status().is_success() {
            return Err(MetadataError::Rejected {
                source_id: SourceId::MusicBrainz,
                status: response.status().as_u16(),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|_| MetadataError::Unreadable {
                source_id: SourceId::MusicBrainz,
            })?;

        Ok(release_ids(&body))
    }
}

/// Every release the answer mentions, however deeply nested.
fn release_ids(body: &str) -> Vec<String> {
    let Ok(document) = roxmltree::Document::parse(body) else {
        return Vec::new();
    };

    document
        .descendants()
        .filter(|node| node.has_tag_name((NAMESPACE, "release")))
        .filter_map(|node| node.attribute("id").map(str::to_owned))
        .collect()
}

/// The release the disc's recordings agree on.
///
/// A single vote decides nothing: nearly every song is on some compilation,
/// and picking one of those would put the wrong album on screen with an air of
/// confidence. Two recordings pointing at the same release is the least that
/// means anything, unless the disc only offered one to ask about.
fn pick(votes: &HashMap<String, usize>, tracks: usize) -> Option<String> {
    let asked = ASKED.min(tracks).max(1);
    let needed = if asked > 1 { 2 } else { 1 };

    let mut best: Vec<(&String, &usize)> = votes.iter().collect();
    best.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));

    best.first()
        .filter(|(_, count)| **count >= needed)
        .map(|(id, _)| (*id).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from what the service actually answered, so the shape is the
    /// real one rather than an invented one.
    const ANSWER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"><isrc id="GBAYE0601498">
<recording-list count="1"><recording id="b2181aae-5cba-496c-bb0c-b4cc0109ebf8">
<title>Yellow Submarine</title>
<release-list count="2">
<release id="c7f648e2-9c6d-32df-b5ee-ccab3e696a51"><title>Revolver</title></release>
<release id="b4b04cbf-118a-3944-9545-38a0a88ff1a2"><title>Revolver</title></release>
</release-list></recording></recording-list></isrc></metadata>"#;

    #[test]
    fn every_release_in_the_answer_is_read() {
        let ids = release_ids(ANSWER);

        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], "c7f648e2-9c6d-32df-b5ee-ccab3e696a51");
    }

    #[test]
    fn rubbish_is_not_an_error() {
        assert!(release_ids("not xml at all").is_empty());
    }

    // Nearly every song is on some compilation, so one recording pointing at a
    // release means very little on its own.
    #[test]
    fn one_vote_is_not_enough_when_more_were_asked_for() {
        let votes = HashMap::from([("a".to_owned(), 1), ("b".to_owned(), 1)]);
        assert_eq!(pick(&votes, 10), None);
    }

    #[test]
    fn the_release_the_recordings_agree_on_wins() {
        let votes = HashMap::from([("a".to_owned(), 1), ("b".to_owned(), 3)]);
        assert_eq!(pick(&votes, 10).as_deref(), Some("b"));
    }

    // A single track disc can only ever offer one vote, and refusing it would
    // mean never answering at all.
    #[test]
    fn a_disc_with_one_track_can_still_be_identified() {
        let votes = HashMap::from([("a".to_owned(), 1)]);

        assert_eq!(pick(&votes, 1).as_deref(), Some("a"));
        assert_eq!(pick(&HashMap::new(), 1), None);
    }

    // Two releases with the same score must not depend on hash order, or the
    // same disc identifies differently from one run to the next.
    #[test]
    fn a_tie_is_broken_the_same_way_every_time() {
        let votes = HashMap::from([("b".to_owned(), 2), ("a".to_owned(), 2)]);

        for _ in 0..8 {
            assert_eq!(pick(&votes, 10).as_deref(), Some("a"));
        }
    }
}
