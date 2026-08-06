// SPDX-License-Identifier: GPL-3.0-or-later

//! CUETools DB, queried by table of contents.
//!
//! CTDB matches on the TOC rather than on an exact Disc ID, and it relays
//! MusicBrainz, Discogs and freedb, so it reaches pressings that a Disc ID
//! lookup walks straight past. Each answer says which database it came from
//! and that label travels with the candidate.
//!
//! The same response also carries the checksums used for verifying a rip. Only
//! the metadata is read here; [`crate::verify`] will want the rest.

use std::collections::HashSet;
use std::time::Duration;

use super::{
    Disc, Lookup, MetadataError, MetadataSource, ReleaseCandidate, SourceId, TrackMetadata,
};
use crate::toc::Toc;

// Plain HTTP on purpose: the service does not answer on 443. Nothing
// confidential is sent, and nothing that arrives is trusted beyond being shown
// to the user as one candidate among several.
const BASE_URL: &str = "http://db.cuetools.net/lookup2.php";
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);
const TIMEOUT: Duration = Duration::from_secs(20);
const NAMESPACE: &str = "http://db.cuetools.net/ns/mmd-1.0#";

pub struct Ctdb {
    client: reqwest::Client,
}

impl Default for Ctdb {
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

impl MetadataSource for Ctdb {
    fn id(&self) -> SourceId {
        SourceId::Ctdb
    }

    fn lookup<'a>(&'a self, disc: &'a Disc) -> Lookup<'a> {
        let toc = &disc.toc;
        Box::pin(async move {
            let response = self
                .client
                .get(BASE_URL)
                .query(&[
                    ("version", "3"),
                    ("ctdb", "1"),
                    ("metadata", "extensive"),
                    ("fuzzy", "1"),
                    ("toc", &toc_parameter(toc)),
                ])
                .send()
                .await
                .map_err(|_| MetadataError::Unreachable {
                    source_id: SourceId::Ctdb,
                })?;

            if !response.status().is_success() {
                return Err(MetadataError::Rejected {
                    source_id: SourceId::Ctdb,
                    status: response.status().as_u16(),
                });
            }

            let body = response
                .text()
                .await
                .map_err(|_| MetadataError::Unreadable {
                    source_id: SourceId::Ctdb,
                })?;

            parse(&body)
        })
    }
}

/// Sector addresses of every track, then where the lead-out begins.
fn toc_parameter(toc: &Toc) -> String {
    toc.tracks
        .iter()
        .map(|track| track.start)
        .chain(std::iter::once(toc.lead_out))
        .map(|sector| sector.to_string())
        .collect::<Vec<_>>()
        .join(":")
}

fn parse(body: &str) -> Result<Vec<ReleaseCandidate>, MetadataError> {
    let document = roxmltree::Document::parse(body).map_err(|_| MetadataError::Unreadable {
        source_id: SourceId::Ctdb,
    })?;

    // The same pressing arrives once per rip that was ever submitted for it, so
    // the list has to be collapsed before anyone sees it.
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for node in document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name((NAMESPACE, "metadata")))
    {
        let candidate = into_candidate(node);

        // A bare `<metadata />` means the service has an entry for the disc but
        // nothing to say about it. Passing that on would end the cascade with a
        // release that has no title, no artist and no tracks.
        if candidate.title.is_empty() && candidate.tracks.is_empty() {
            continue;
        }

        let fingerprint = (
            candidate.title.clone(),
            candidate.artist.clone(),
            candidate.barcode.clone(),
            candidate.date.clone(),
            candidate.country.clone(),
            candidate.tracks.len(),
        );

        if seen.insert(fingerprint) {
            candidates.push(candidate);
        }
    }

    Ok(candidates)
}

fn into_candidate(node: roxmltree::Node<'_, '_>) -> ReleaseCandidate {
    let child = |name: &str| {
        node.children()
            .find(|child| child.has_tag_name((NAMESPACE, name)))
    };
    let label = child("label");
    let release = child("release");
    let coverart = child("coverart");

    ReleaseCandidate {
        source_id: SourceId::Ctdb,
        relayed_from: text(node.attribute("source")),
        id: node.attribute("id").unwrap_or_default().to_owned(),
        title: node.attribute("album").unwrap_or_default().to_owned(),
        artist: node.attribute("artist").unwrap_or_default().to_owned(),
        // A release element carries the full date; the year attribute is all
        // there is when it does not.
        date: release
            .and_then(|node| text(node.attribute("date")))
            .or_else(|| text(node.attribute("year"))),
        country: release.and_then(|node| text(node.attribute("country"))),
        label: label.and_then(|node| text(node.attribute("name"))),
        barcode: text(node.attribute("barcode")),
        disambiguation: text(node.attribute("discname")),
        genre: None,
        style: None,
        composer: None,
        comment: None,
        compilation: false,
        disc_number: number(node.attribute("discnumber")).unwrap_or(1),
        disc_total: number(node.attribute("disccount")),
        medium_track_counts: vec![
            node.children()
                .filter(|child| child.has_tag_name((NAMESPACE, "track")))
                .count() as u32,
        ],
        media: Vec::new(),
        cover_art: coverart.and_then(|node| text(node.attribute("uri"))),
        tracks: node
            .children()
            .filter(|child| child.has_tag_name((NAMESPACE, "track")))
            .enumerate()
            .map(|(index, track)| TrackMetadata {
                number: index as u8 + 1,
                title: track.attribute("name").unwrap_or_default().to_owned(),
                // CTDB has no per-track credits, so the release artist stands
                // in and compilations need a source that does.
                artist: node.attribute("artist").unwrap_or_default().to_owned(),
                length_ms: None,
            })
            .collect(),
    }
}

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn number(value: Option<&str>) -> Option<u32> {
    value.and_then(|value| value.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::{LEAD_IN_FRAMES, TocEntry};

    // Trimmed from a real answer, keeping one pressing twice so the collapsing
    // is exercised.
    const SAMPLE: &str = r#"<ctdb xmlns="http://db.cuetools.net/ns/mmd-1.0#">
      <entry confidence="2044" crc32="36238217" id="14172" />
      <metadata album="Hello Nasty" artist="Beastie Boys" barcode="724349572324"
                disccount="2" discname="" discnumber="1"
                id="44d66a09-491a-3143-89f2-dd7232424325" source="musicbrainz" year="1998">
        <track name="Super Disco Breakin&#8217;" />
        <track name="The Move" />
        <label catno="495 7232" name="Grand Royal" />
        <release country="GB" date="1998-07-06" />
        <coverart primary="1" uri="http://coverartarchive.org/release/44d66a09/8708986198.jpg" />
      </metadata>
      <metadata album="Hello Nasty" artist="Beastie Boys" barcode="724349572324"
                disccount="2" discname="" discnumber="1"
                id="84a4ba6a-cc66-4a8b-b443-198646fbf85f" source="musicbrainz" year="1998">
        <track name="Super Disco Breakin&#8217;" />
        <track name="The Move" />
        <label catno="495 7232" name="Grand Royal" />
        <release country="GB" date="1998-07-06" />
      </metadata>
      <metadata album="Hello Nasty" artist="Beastie Boys" barcode="724383771622"
                disccount="1" discnumber="1" id="other" source="freedb" year="1998">
        <track name="Super Disco Breakin'" />
        <track name="The Move" />
      </metadata>
    </ctdb>"#;

    #[test]
    fn reads_a_release_out_of_the_service_answer() {
        let candidates = parse(SAMPLE).expect("sample parses");

        assert_eq!(candidates.len(), 2, "identical pressings collapse into one");

        let first = &candidates[0];
        assert_eq!(first.title, "Hello Nasty");
        assert_eq!(first.artist, "Beastie Boys");
        assert_eq!(first.relayed_from.as_deref(), Some("musicbrainz"));
        assert_eq!(first.label.as_deref(), Some("Grand Royal"));
        assert_eq!(first.date.as_deref(), Some("1998-07-06"));
        assert_eq!(first.country.as_deref(), Some("GB"));
        assert_eq!(first.disc_total, Some(2));
        assert_eq!(
            first.disambiguation, None,
            "an empty disc name is not a note"
        );
        assert!(first.cover_art.is_some());
        assert_eq!(first.tracks.len(), 2);
        assert_eq!(first.tracks[1].number, 2);
        assert_eq!(first.tracks[1].title, "The Move");

        assert_eq!(candidates[1].relayed_from.as_deref(), Some("freedb"));
    }

    #[test]
    fn falls_back_to_the_year_when_there_is_no_release_date() {
        let candidates = parse(SAMPLE).unwrap();
        assert_eq!(candidates[1].date.as_deref(), Some("1998"));
    }

    #[test]
    fn sends_sector_addresses_ending_at_the_lead_out() {
        let entries: Vec<TocEntry> = [150u32, 9700, 25887]
            .iter()
            .enumerate()
            .map(|(index, offset)| TocEntry {
                number: index as u8 + 1,
                start: offset - LEAD_IN_FRAMES,
                control: 0,
            })
            .collect();

        let toc = Toc::from_entries(&entries, 303602 - LEAD_IN_FRAMES).unwrap();
        assert_eq!(toc_parameter(&toc), "0:9550:25737:303452");
    }

    // Exactly what the service sends for a disc it has an entry for but knows
    // nothing about.
    #[test]
    fn an_empty_metadata_block_is_not_a_candidate() {
        let body = r#"<ctdb xmlns="http://db.cuetools.net/ns/mmd-1.0#">
          <entry confidence="2" crc32="0" id="1" />
          <metadata />
        </ctdb>"#;

        assert!(parse(body).unwrap().is_empty());
    }

    #[test]
    fn an_answer_without_metadata_yields_no_candidates() {
        let body = r#"<ctdb xmlns="http://db.cuetools.net/ns/mmd-1.0#">
          <entry confidence="3" crc32="0" id="1" />
        </ctdb>"#;

        assert!(parse(body).unwrap().is_empty());
    }

    #[test]
    fn a_body_that_is_not_xml_is_reported_as_unreadable() {
        assert!(matches!(
            parse("<<<not xml"),
            Err(MetadataError::Unreadable { .. })
        ));
    }
}
