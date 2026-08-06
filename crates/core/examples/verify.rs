// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads one track and looks for the read offset that makes its checksum agree
//! with everyone else's, which is also how a drive gets calibrated:
//!
//! ```text
//! cargo run -p toccata-core --example verify
//! ```
//!
//! The track is read once, with a little of its neighbours either side, and
//! every candidate offset is then a different window over the same bytes. That
//! is the whole trick: trying offsets by re-reading would take an afternoon.

use std::io::Write;

use toccata_core::drive::{self, BYTES_PER_SAMPLE, BYTES_PER_SECTOR, SAMPLES_PER_SECTOR};
use toccata_core::verify::{Verifier, ctdb::Verification};

/// How far either side of zero to look, in samples. Real drives sit well
/// inside this; anything further out is not an offset but a fault.
const RANGE: i32 = 1500;

#[tokio::main]
async fn main() {
    let Some(info) = drive::list().into_iter().next() else {
        println!("no optical drives found");
        return;
    };

    let mut handle = match drive::open(&info.id) {
        Ok(handle) => handle,
        Err(error) => return println!("cannot open {}: {error}", info.id),
    };

    let toc = match handle.read_toc() {
        Ok(toc) => toc,
        Err(error) => return println!("cannot read the disc: {error}"),
    };

    let entries = match Verification::default().lookup(&toc).await {
        Ok(entries) if !entries.is_empty() => entries,
        Ok(_) => return println!("nobody has submitted this disc, so there is nothing to match"),
        Err(error) => return println!("ctdb did not answer: {error}"),
    };

    println!("{} entries, best confidence {}", entries.len(), {
        entries.iter().map(|entry| entry.confidence).max().unwrap()
    });

    // A short track in the middle: no AccurateRip skipping to think about, and
    // it reads quickly.
    let audio: Vec<_> = toc.tracks.iter().filter(|track| track.audio).collect();
    let index = audio.len() / 2;
    let track = audio[index];

    let padding = RANGE.unsigned_abs().div_ceil(SAMPLES_PER_SECTOR) + 1;
    let first = track.start.saturating_sub(padding);
    let sectors = track.length + padding * 2;

    println!(
        "reading track {} ({} sectors plus {padding} either side)",
        track.number, track.length
    );

    let mut buffer = vec![0u8; sectors as usize * BYTES_PER_SECTOR];
    for (index, chunk) in buffer.chunks_mut(BYTES_PER_SECTOR * 25).enumerate() {
        let start = first + (index as u32 * 25);
        let count = (chunk.len() / BYTES_PER_SECTOR) as u32;

        if let Err(error) = handle.read_audio(start, count, chunk) {
            return println!("cannot read sector {start}: {error}");
        }
    }

    // Where the track sits in the buffer once the padding is accounted for.
    let base = (track.start - first) as usize * SAMPLES_PER_SECTOR as usize;
    let length = track.length as usize * SAMPLES_PER_SECTOR as usize;

    let wanted: Vec<u32> = entries
        .iter()
        .filter(|entry| entry.track_crcs.len() > index)
        .map(|entry| entry.track_crcs[index])
        .collect();

    println!("looking for {wanted:08x?}");

    for offset in -RANGE..=RANGE {
        let start = (base as i32 + offset) as usize;
        let window = &buffer[start * BYTES_PER_SAMPLE..(start + length) * BYTES_PER_SAMPLE];

        let mut verifier = Verifier::new(track.length * SAMPLES_PER_SECTOR, false, false);
        verifier.write_all(window).expect("hashing cannot fail");
        let sums = verifier.finish();

        if wanted.contains(&sums.crc32) {
            println!("offset {offset:+} gives crc32 {:08x} — a match", sums.crc32);
            return;
        }

        if offset == 0 {
            println!("offset  0 gives crc32 {:08x}", sums.crc32);
        }
    }

    println!("no offset in ±{RANGE} matches, so the checksum is not defined this way");
}
