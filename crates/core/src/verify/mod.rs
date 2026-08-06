// SPDX-License-Identifier: GPL-3.0-or-later

//! Checksums that say whether a rip matches everyone else's.
//!
//! Three numbers come out of every track. The CRC32 is the one EAC prints and
//! is useful for comparing two rips of your own. The AccurateRip pair is what
//! the rest of the world compares against: the same disc read on a different
//! drive, with that drive's offset corrected, produces the same two numbers.
//!
//! The sums are computed here and shown here. **The AccurateRip database is
//! not queried.** The algorithm is public, but the database belongs to
//! Illustrate and using it from other software needs their agreement, which
//! this project does not have. Online verification goes through CTDB instead.

pub mod ctdb;

use std::io::{self, Write};

use serde::Serialize;

use crate::drive::{BYTES_PER_SAMPLE, SAMPLES_PER_SECTOR};

/// Samples AccurateRip ignores at each end of the disc, being the first and
/// last five sectors. The pressing itself is unreliable there, which is the
/// whole reason the rule exists.
const SKIPPED_SAMPLES: u32 = 5 * SAMPLES_PER_SECTOR;

/// What one track hashed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checksums {
    /// The number EAC calls the copy CRC.
    pub crc32: u32,
    pub accuraterip_v1: u32,
    pub accuraterip_v2: u32,
}

/// Hashes audio as it goes past.
///
/// Fed the samples in order, in whatever sized pieces they arrive in. A sample
/// split across two writes is held until the rest of it turns up, since the
/// AccurateRip sums are defined over whole stereo frames.
#[derive(Debug, Clone)]
pub struct Verifier {
    crc: u32,
    v1: u32,
    v2: u32,
    /// Position of the next sample, counting from one across the whole track.
    position: u32,
    /// First and last position AccurateRip counts, both inclusive.
    from: u32,
    to: u32,
    /// Bytes of a sample that arrived without the rest of it.
    partial: [u8; BYTES_PER_SAMPLE],
    partial_len: usize,
}

impl Verifier {
    /// `samples` is the length of the track, which AccurateRip needs up front
    /// because the last track's window is measured from its end.
    pub fn new(samples: u32, first_track: bool, last_track: bool) -> Self {
        Self {
            crc: 0xFFFF_FFFF,
            v1: 0,
            v2: 0,
            position: 1,
            // The bounds are the ones AccurateRip's own implementation uses,
            // down to the comparison being inclusive at both ends. That makes
            // the first track skip one sample fewer than the last, which looks
            // like a mistake and is simply what everyone else computes.
            from: if first_track { SKIPPED_SAMPLES } else { 0 },
            to: match last_track {
                true => samples.saturating_sub(SKIPPED_SAMPLES),
                false => samples,
            },
            partial: [0; BYTES_PER_SAMPLE],
            partial_len: 0,
        }
    }

    pub fn finish(self) -> Checksums {
        Checksums {
            crc32: self.crc ^ 0xFFFF_FFFF,
            accuraterip_v1: self.v1,
            accuraterip_v2: self.v2,
        }
    }

    fn sample(&mut self, bytes: [u8; BYTES_PER_SAMPLE]) {
        // One stereo frame is read as a single little endian word, which is
        // what both AccurateRip versions multiply.
        let value = u32::from_le_bytes(bytes);

        if self.position >= self.from && self.position <= self.to {
            self.v1 = self.v1.wrapping_add(value.wrapping_mul(self.position));

            // The wide product is split and both halves are added, which is
            // the only thing separating version two from version one.
            let wide = u64::from(value) * u64::from(self.position);
            self.v2 = self
                .v2
                .wrapping_add((wide >> 32) as u32)
                .wrapping_add(wide as u32);
        }

        self.position = self.position.wrapping_add(1);
    }
}

impl Write for Verifier {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.crc = crc32(self.crc, buf);

        let mut rest = buf;

        // Finish whatever the previous write left half done.
        if self.partial_len > 0 {
            let wanted = BYTES_PER_SAMPLE - self.partial_len;
            let taken = wanted.min(rest.len());

            self.partial[self.partial_len..self.partial_len + taken]
                .copy_from_slice(&rest[..taken]);
            self.partial_len += taken;
            rest = &rest[taken..];

            if self.partial_len < BYTES_PER_SAMPLE {
                return Ok(buf.len());
            }

            let sample = self.partial;
            self.partial_len = 0;
            self.sample(sample);
        }

        let whole = rest.len() / BYTES_PER_SAMPLE * BYTES_PER_SAMPLE;
        for chunk in rest[..whole].chunks_exact(BYTES_PER_SAMPLE) {
            self.sample(chunk.try_into().expect("chunks_exact gives four bytes"));
        }

        let tail = &rest[whole..];
        self.partial[..tail.len()].copy_from_slice(tail);
        self.partial_len = tail.len();

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A writer with a verifier alongside it, so the audio is hashed on its way to
/// wherever it was going anyway. The disc is read once and this costs one pass
/// over memory, which is nothing next to the read.
pub struct Verified<W: Write> {
    inner: W,
    verifier: Verifier,
}

impl<W: Write> Verified<W> {
    pub fn new(inner: W, samples: u32, first_track: bool, last_track: bool) -> Self {
        Self {
            inner,
            verifier: Verifier::new(samples, first_track, last_track),
        }
    }

    pub fn finish(self) -> (W, Checksums) {
        (self.inner, self.verifier.finish())
    }
}

impl<W: Write> Write for Verified<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write_all(buf)?;
        let _ = self.verifier.write(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// CRC-32 as everything else means it: the reflected polynomial, table driven.
///
/// A disc is several hundred megabytes and the bit-at-a-time version spends
/// eight shifts on every byte of it, which is felt on a rip and is unusable
/// for searching a drive's offset, where the same audio is hashed thousands of
/// times over.
fn crc32(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc = TABLE[((crc ^ u32::from(*byte)) & 0xFF) as usize] ^ (crc >> 8);
    }

    crc
}

static TABLE: [u32; 256] = {
    const POLYNOMIAL: u32 = 0xEDB8_8320;

    let mut table = [0u32; 256];
    let mut index = 0;

    while index < 256 {
        let mut entry = index as u32;
        let mut bit = 0;

        while bit < 8 {
            entry = match entry & 1 {
                1 => (entry >> 1) ^ POLYNOMIAL,
                _ => entry >> 1,
            };
            bit += 1;
        }

        table[index] = entry;
        index += 1;
    }

    table
};

#[cfg(test)]
mod tests {
    use super::*;

    fn checksums(samples: &[u32], first: bool, last: bool) -> Checksums {
        let mut verifier = Verifier::new(samples.len() as u32, first, last);

        for value in samples {
            verifier
                .write_all(&value.to_le_bytes())
                .expect("a verifier never refuses");
        }

        verifier.finish()
    }

    #[test]
    fn crc32_matches_the_usual_check_value() {
        // The string every CRC-32 implementation is checked against.
        assert_eq!(crc32(0xFFFF_FFFF, b"123456789") ^ 0xFFFF_FFFF, 0xCBF4_3926);
        assert_eq!(crc32(0xFFFF_FFFF, b"") ^ 0xFFFF_FFFF, 0);
    }

    /// The sum of 1..=n, which is what a track of nothing but ones adds up to.
    fn triangle(n: u32) -> u32 {
        (1..=n).fold(0u32, |sum, value| sum.wrapping_add(value))
    }

    #[test]
    fn a_middle_track_counts_every_sample() {
        let samples = vec![1u32; 1000];
        let sums = checksums(&samples, false, false);

        assert_eq!(sums.accuraterip_v1, triangle(1000));
        assert_eq!(
            sums.accuraterip_v2, sums.accuraterip_v1,
            "small products have nothing in their high half"
        );
    }

    // Both ends of the disc are skipped, and the two ends do not skip the same
    // amount. That is not a bug here: the original implementation compares
    // inclusively at both ends and everyone reproduces it.
    #[test]
    fn the_first_track_skips_all_but_one_of_the_first_five_sectors() {
        let total = SKIPPED_SAMPLES + 100;
        let samples = vec![1u32; total as usize];

        let sums = checksums(&samples, true, false);
        let expected = triangle(total).wrapping_sub(triangle(SKIPPED_SAMPLES - 1));

        assert_eq!(sums.accuraterip_v1, expected);
    }

    #[test]
    fn the_last_track_skips_the_final_five_sectors() {
        let total = SKIPPED_SAMPLES + 100;
        let samples = vec![1u32; total as usize];

        let sums = checksums(&samples, false, true);

        assert_eq!(sums.accuraterip_v1, triangle(total - SKIPPED_SAMPLES));
    }

    #[test]
    fn a_single_track_disc_skips_at_both_ends() {
        let total = SKIPPED_SAMPLES * 3;
        let samples = vec![1u32; total as usize];

        let sums = checksums(&samples, true, true);
        let expected =
            triangle(total - SKIPPED_SAMPLES).wrapping_sub(triangle(SKIPPED_SAMPLES - 1));

        assert_eq!(sums.accuraterip_v1, expected);
    }

    // The only difference between the versions, and the reason v2 exists.
    #[test]
    fn version_two_keeps_what_overflows_out_of_version_one() {
        let samples = [0u32, 0xFFFF_FFFF];
        let sums = checksums(&samples, false, false);

        // Position two, so the product is 0x1_FFFF_FFFE.
        assert_eq!(sums.accuraterip_v1, 0xFFFF_FFFE);
        assert_eq!(sums.accuraterip_v2, 0xFFFF_FFFF);
    }

    // The rip hands over whatever a sector read produced, and a sample split
    // across two of those must not be counted twice or dropped.
    #[test]
    fn the_sums_do_not_depend_on_how_the_audio_was_chunked() {
        let audio: Vec<u8> = (0..4000u32).flat_map(|value| value.to_le_bytes()).collect();

        let whole = {
            let mut verifier = Verifier::new(1000, true, true);
            verifier.write_all(&audio).unwrap();
            verifier.finish()
        };

        for chunk in [1usize, 3, 7, 1024] {
            let mut verifier = Verifier::new(1000, true, true);
            for piece in audio.chunks(chunk) {
                verifier.write_all(piece).unwrap();
            }

            assert_eq!(verifier.finish(), whole, "chunks of {chunk}");
        }
    }

    #[test]
    fn the_audio_passes_through_unchanged() {
        let audio: Vec<u8> = (0..400u32).flat_map(|value| value.to_le_bytes()).collect();

        let mut verified = Verified::new(Vec::new(), 400, false, false);
        for piece in audio.chunks(7) {
            verified.write_all(piece).unwrap();
        }

        let (written, sums) = verified.finish();
        assert_eq!(written, audio);
        assert_ne!(sums.crc32, 0);
    }
}
