// SPDX-License-Identifier: GPL-3.0-or-later

//! Disc identifiers derived from the table of contents.
//!
//! Both functions take MSF frame offsets, that is sector addresses with the
//! 150 frame lead-in already added, because that is the form both identifier
//! specifications are written in. [`crate::toc::Toc`] does the conversion.

use sha1::{Digest, Sha1};

/// MusicBrainz Disc ID: SHA-1 over a fixed-width hex rendering of the TOC,
/// encoded with a base64 variant that swaps `+/=` for `._-`.
///
/// Multi-session discs get no special treatment here. The identifier has to
/// match what libdiscid produces for the same drive, and libdiscid hashes the
/// TOC exactly as the drive reports it.
pub fn musicbrainz(first_track: u8, last_track: u8, lead_out: u32, offsets: &[u32]) -> String {
    let mut input = format!("{first_track:02X}{last_track:02X}{lead_out:08X}");
    for slot in 0..99 {
        input.push_str(&format!("{:08X}", offsets.get(slot).copied().unwrap_or(0)));
    }

    let digest: [u8; 20] = Sha1::digest(input.as_bytes()).into();
    encode_base64(&digest)
}

/// FreeDB / CDDB disc ID, kept for the sources that still index by it.
pub fn freedb(lead_out: u32, offsets: &[u32]) -> String {
    let checksum: u32 = offsets
        .iter()
        .map(|offset| digit_sum(offset / FRAMES_PER_SECOND))
        .sum();
    let seconds = lead_out / FRAMES_PER_SECOND - offsets[0] / FRAMES_PER_SECOND;

    let id = ((checksum % 0xff) << 24) | (seconds << 8) | offsets.len() as u32;
    format!("{id:08x}")
}

const FRAMES_PER_SECOND: u32 = 75;

fn digit_sum(mut value: u32) -> u32 {
    let mut sum = 0;
    while value > 0 {
        sum += value % 10;
        value /= 10;
    }
    sum
}

/// The digest goes out in base64 with `+/=` swapped for `._-`.
fn encode_base64(digest: &[u8; 20]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._";

    crate::base64::encode(digest, ALPHABET, '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    // The canonical example from the MusicBrainz Disc ID documentation.
    const SIX_TRACK_OFFSETS: [u32; 6] = [150, 15363, 32314, 46592, 63414, 80489];

    // libdiscid checks itself against this one, which makes it the closest
    // thing to a reference implementation we can compare against.
    const TWENTY_TWO_TRACK_OFFSETS: [u32; 22] = [
        150, 9700, 25887, 39297, 53795, 63735, 77517, 94877, 107270, 123552, 135522, 148422,
        161197, 174790, 192022, 205545, 218010, 228700, 239590, 255470, 266932, 288750,
    ];

    #[test]
    fn musicbrainz_matches_documented_example() {
        assert_eq!(
            musicbrainz(1, 6, 95462, &SIX_TRACK_OFFSETS),
            "49HHV7Eb8UKF3aQiNmu1GR8vKTY-"
        );
    }

    #[test]
    fn musicbrainz_matches_libdiscid() {
        assert_eq!(
            musicbrainz(1, 22, 303602, &TWENTY_TWO_TRACK_OFFSETS),
            "xUp1F2NkfP8s8jaeFn_Av3jNEI4-"
        );
    }

    #[test]
    fn freedb_matches_libdiscid() {
        assert_eq!(freedb(303602, &TWENTY_TWO_TRACK_OFFSETS), "370fce16");
    }

    #[test]
    fn disc_id_is_always_twenty_eight_characters() {
        let id = musicbrainz(1, 6, 95462, &SIX_TRACK_OFFSETS);
        assert_eq!(id.len(), 28);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        );
    }

    // A single frame of drift produces a completely different identifier, which
    // is exactly why an off-by-one here is so hard to notice downstream.
    #[test]
    fn one_frame_of_drift_changes_the_identifier() {
        let correct = musicbrainz(1, 6, 95462, &SIX_TRACK_OFFSETS);

        let mut drifted = SIX_TRACK_OFFSETS;
        drifted[3] += 1;
        assert_ne!(musicbrainz(1, 6, 95462, &drifted), correct);
        assert_ne!(musicbrainz(1, 6, 95463, &SIX_TRACK_OFFSETS), correct);
    }
}
