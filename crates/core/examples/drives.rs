// SPDX-License-Identifier: GPL-3.0-or-later

//! Lists the optical drives the system reports and dumps the table of contents
//! of whatever is loaded. Handy for checking a real drive without the GUI:
//!
//! ```text
//! cargo run -p toccata-core --example drives
//! ```

use toccata_core::drive;

fn main() {
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
            Err(error) => println!("  cannot read toc: {error}"),
        }
    }
}
