// SPDX-License-Identifier: GPL-3.0-or-later

//! Extracts one track from the disc in the first drive, for checking the read
//! path against real hardware:
//!
//! ```text
//! cargo run -p toccata-core --example rip -- 1 out.wav
//! ```

use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::AtomicBool;

use toccata_core::drive;
use toccata_core::rip::{self, Options};

fn main() {
    let mut args = std::env::args().skip(1);
    let number: u8 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let path = args.next().unwrap_or_else(|| "track.wav".to_owned());

    let Some(info) = drive::list().into_iter().next() else {
        println!("no optical drives found");
        return;
    };

    let mut handle = match drive::open(&info.id) {
        Ok(handle) => handle,
        Err(error) => {
            println!("cannot open {}: {error}", info.id);
            return;
        }
    };

    let toc = match handle.read_toc() {
        Ok(toc) => toc,
        Err(error) => {
            println!("cannot read toc: {error}");
            return;
        }
    };

    let mut output = BufWriter::new(File::create(&path).expect("the output file can be created"));
    let started = std::time::Instant::now();
    let mut last = 0;

    let result = rip::track(
        handle.as_mut(),
        &toc,
        number,
        &Options::default(),
        &mut output,
        &mut |done, total| {
            let percent = done * 100 / total.max(1);
            if percent >= last + 10 {
                last = percent;
                println!("  {percent}%");
            }
        },
        &AtomicBool::new(false),
    );

    match result {
        Ok(extracted) => println!(
            "wrote {path}: track {}, {} samples, {} unreadable sectors, {:.1}s",
            extracted.track,
            extracted.samples,
            extracted.unreadable_sectors,
            started.elapsed().as_secs_f32(),
        ),
        Err(error) => println!("rip failed: {error}"),
    }
}
