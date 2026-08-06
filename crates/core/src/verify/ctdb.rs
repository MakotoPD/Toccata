// SPDX-License-Identifier: GPL-3.0-or-later

//! Checking a rip against other people's, through CUETools DB.
//!
//! CTDB is used here rather than AccurateRip because AccurateRip's database
//! belongs to Illustrate and querying it from other software needs their
//! agreement. CTDB answers the same question — has anyone else read this disc
//! and got what I got — and answers it per track.
//!
//! A track that does not match is not necessarily a bad rip. The usual cause
//! is a drive read offset that has not been set, which shifts every sample and
//! changes every checksum while the audio itself is perfectly readable. The
//! interface has to say that, or the first mismatch will be read as damage.

use std::time::Duration;

use serde::Serialize;

use super::Checksums;
use crate::metadata::{MetadataError, SourceId};
use crate::toc::Toc;

// Plain HTTP because the service does not answer on 443. Nothing confidential
// goes out, and what comes back is a set of numbers to compare against.
const BASE_URL: &str = "http://db.cuetools.net/lookup2.php";
const USER_AGENT: &str = concat!(
    "Toccata/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/MakotoPD/Toccata )"
);
const TIMEOUT: Duration = Duration::from_secs(20);
const NAMESPACE: &str = "http://db.cuetools.net/ns/mmd-1.0#";

/// One rip somebody else submitted for this disc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// How many people arrived at these numbers.
    pub confidence: u32,
    pub track_crcs: Vec<u32>,
}

/// What can be said about one track of ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Verdict {
    /// Somebody else read this track and got the same audio.
    Accurate { confidence: u32 },
    /// The disc is known but nobody's copy matches ours.
    Different,
    /// Nothing to compare against, which says nothing about the rip.
    Unknown,
}

pub struct Verification {
    client: reqwest::Client,
}

impl Default for Verification {
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

impl Verification {
    pub async fn lookup(&self, toc: &Toc) -> Result<Vec<Entry>, MetadataError> {
        let response = self
            .client
            .get(BASE_URL)
            .query(&[
                ("version", "3"),
                ("ctdb", "1"),
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

fn parse(body: &str) -> Result<Vec<Entry>, MetadataError> {
    let document = roxmltree::Document::parse(body).map_err(|_| MetadataError::Unreadable {
        source_id: SourceId::Ctdb,
    })?;

    Ok(document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name((NAMESPACE, "entry")))
        .map(|node| Entry {
            confidence: node
                .attribute("confidence")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            track_crcs: node
                .attribute("trackcrcs")
                .map(|value| {
                    value
                        .split_whitespace()
                        .map(|crc| u32::from_str_radix(crc, 16).unwrap_or(0))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

/// Compares our checksums against everyone else's, one track at a time.
///
/// Keyed by track number rather than by position, because a rip of a few
/// chosen tracks still has to line up against entries that describe the whole
/// disc. Confidences add up across entries: two people who each arrived at the
/// same numbers separately are two people who agree, and that is worth more
/// than whichever entry happened to be listed first.
pub fn compare(entries: &[Entry], ours: &[(u8, Checksums)]) -> Vec<Verdict> {
    ours.iter()
        .map(|(number, checksums)| {
            let index = usize::from(*number).saturating_sub(1);

            let known: Vec<&Entry> = entries
                .iter()
                .filter(|entry| entry.track_crcs.len() > index)
                .collect();

            if known.is_empty() {
                return Verdict::Unknown;
            }

            let confidence: u32 = known
                .iter()
                .filter(|entry| entry.track_crcs[index] == checksums.crc32)
                .map(|entry| entry.confidence)
                .sum();

            match confidence {
                0 => Verdict::Different,
                confidence => Verdict::Accurate { confidence },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the service actually answered for a disc in the drive, kept so the
    /// parsing is tested against the real shape rather than an invented one.
    const ANSWER: &str = r#"<ctdb xmlns="http://db.cuetools.net/ns/mmd-1.0#" xmlns:ext="http://db.cuetools.net/ns/ext-1.0#">
 <entry confidence="10" crc32="43f3794c" hasparity="http://p.cuetools.net/12878744" id="12878744" npar="8" stride="5880" syndrome="wBXeQ5E7i5sr8vsi6XdubQ==" toc="0:13173:24517:50699:64387:79012:88762:102748" trackcrcs="1a8ecbaf 04970de2 462f3658 58eb981f df2517e0 62db1881 24725bc0" />
 <metadata />
</ctdb>"#;

    fn checksums(crc32: u32) -> Checksums {
        Checksums {
            crc32,
            accuraterip_v1: 0,
            accuraterip_v2: 0,
        }
    }

    #[test]
    fn reads_the_confidence_and_every_track_checksum() {
        let entries = parse(ANSWER).expect("the answer parses");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].confidence, 10);
        assert_eq!(entries[0].track_crcs.len(), 7);
        assert_eq!(entries[0].track_crcs[0], 0x1a8e_cbaf);
        assert_eq!(entries[0].track_crcs[6], 0x2472_5bc0);
    }

    #[test]
    fn a_matching_track_carries_the_confidence_behind_it() {
        let entries = parse(ANSWER).unwrap();
        let ours = vec![(1, checksums(0x1a8e_cbaf)), (2, checksums(0xdead_beef))];

        assert_eq!(
            compare(&entries, &ours),
            vec![Verdict::Accurate { confidence: 10 }, Verdict::Different]
        );
    }

    // Ripping one track from the middle still has to line up with entries that
    // describe the whole disc.
    #[test]
    fn a_single_chosen_track_is_matched_against_its_own_place() {
        let entries = parse(ANSWER).unwrap();

        assert_eq!(
            compare(&entries, &[(6, checksums(0x62db_1881))]),
            vec![Verdict::Accurate { confidence: 10 }]
        );
        assert_eq!(
            compare(&entries, &[(6, checksums(0x1a8e_cbaf))]),
            vec![Verdict::Different],
            "track one's checksum is not track six's"
        );
    }

    // People agreeing separately are more reassuring than one person twice.
    #[test]
    fn confidences_add_up_across_entries() {
        let entries = vec![
            Entry {
                confidence: 3,
                track_crcs: vec![7],
            },
            Entry {
                confidence: 4,
                track_crcs: vec![7],
            },
            Entry {
                confidence: 9,
                track_crcs: vec![8],
            },
        ];

        assert_eq!(
            compare(&entries, &[(1, checksums(7))]),
            vec![Verdict::Accurate { confidence: 7 }]
        );
    }

    #[test]
    fn a_disc_nobody_has_submitted_says_nothing_either_way() {
        let empty = parse(r#"<ctdb xmlns="http://db.cuetools.net/ns/mmd-1.0#" />"#).unwrap();

        assert!(empty.is_empty());
        assert_eq!(
            compare(&empty, &[(1, checksums(1))]),
            vec![Verdict::Unknown]
        );
    }

    // A track past the end of what an entry lists must not be read as a
    // mismatch, since the entry simply says nothing about it.
    #[test]
    fn a_track_no_entry_covers_is_unknown_rather_than_wrong() {
        let entries = vec![Entry {
            confidence: 5,
            track_crcs: vec![1],
        }];

        assert_eq!(
            compare(&entries, &[(1, checksums(1)), (2, checksums(2))]),
            vec![Verdict::Accurate { confidence: 5 }, Verdict::Unknown]
        );
    }
}
