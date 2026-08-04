// SPDX-License-Identifier: GPL-3.0-or-later

//! Lists the optical drives the system reports, dumps the table of contents of
//! whatever is loaded and asks the metadata cascade what the disc is. Handy for
//! checking a real drive without the GUI:
//!
//! ```text
//! cargo run -p toccata-core --example drives
//! ```

use toccata_core::drive;
use toccata_core::metadata::Cascade;
use toccata_core::toc::Toc;

#[tokio::main]
async fn main() {
    let drives = drive::list();
    if drives.is_empty() {
        println!("no optical drives found");
        return;
    }

    for info in drives {
        println!("{} ({})", info.name, info.id);

        let mut handle = match drive::open(&info.id) {
            Ok(handle) => handle,
            Err(error) => {
                println!("  cannot open: {error}");
                continue;
            }
        };

        match handle.read_toc() {
            Ok(toc) => {
                print_toc(&toc);
                identify(&toc).await;
            }
            Err(error) => println!("  cannot read toc: {error}"),
        }
    }
}

fn print_toc(toc: &Toc) {
    println!("  disc id  {}", toc.musicbrainz_disc_id());
    println!("  freedb   {}", toc.freedb_id());
    println!("  duration {:?}", toc.duration());

    for track in &toc.tracks {
        println!(
            "  {:>2}  start {:>7}  {:>6} frames  {}{}",
            track.number,
            track.start,
            track.length,
            if track.audio { "audio" } else { "data" },
            if track.pre_emphasis {
                " pre-emphasis"
            } else {
                ""
            },
        );
    }
}

async fn identify(toc: &Toc) {
    // Corrections made in the application live under its own data directory;
    // this scratch path keeps the example from reading them.
    let store = std::env::temp_dir().join("toccata-example-discs");
    let report = Cascade::standard(store).lookup(toc).await;

    for failure in &report.failures {
        println!("  source failed: {failure}");
    }

    if report.candidates.is_empty() {
        println!("  no source recognised this disc");
        return;
    }

    for candidate in &report.candidates {
        println!(
            "  [{:?}] {} - {} ({}, {} tracks)",
            candidate.source_id,
            candidate.artist,
            candidate.title,
            candidate.date.as_deref().unwrap_or("no date"),
            candidate.tracks.len(),
        );
    }
}
