// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

/// The libraries this application links against. The rest of an FFmpeg package
/// is video and filtering, which nothing here touches.
const LIBRARIES: [&str; 4] = ["avcodec", "avformat", "avutil", "swresample"];

fn main() {
    tauri_build::build();
    carry_ffmpeg();
}

/// Copies the FFmpeg libraries next to the executable being built.
///
/// Windows looks for a DLL beside the program that wants it, so without this a
/// development build only runs from a shell that happens to have the FFmpeg
/// package on its PATH — which is the kind of thing nobody remembers setting
/// up until the day it is missing. The bundler does the same for a release
/// through `tauri.conf.json`; this is the same idea for `cargo run`.
///
/// Does nothing where the libraries come from the system, which is everywhere
/// except Windows.
fn carry_ffmpeg() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");

    if !cfg!(windows) {
        return;
    }

    let Ok(source) = std::env::var("FFMPEG_DIR") else {
        return;
    };

    let Some(target) = target_directory() else {
        return;
    };

    for library in LIBRARIES {
        let Some(found) = newest_matching(&Path::new(&source).join("bin"), library) else {
            continue;
        };

        let Some(name) = found.file_name() else {
            continue;
        };

        // Copying over one that is already there is cheap, and keeps the copy
        // honest when the package underneath gets updated.
        let _ = std::fs::copy(&found, target.join(name));
    }
}

/// The directory the executable is being written to.
///
/// Cargo does not hand this to a build script, so it is walked back to from
/// `OUT_DIR`, which is `<target>/<profile>/build/<crate>-<hash>/out`.
fn target_directory() -> Option<PathBuf> {
    let out = PathBuf::from(std::env::var("OUT_DIR").ok()?);

    out.ancestors()
        .nth(3)
        .filter(|path| path.join("build").is_dir())
        .map(Path::to_path_buf)
}

/// The library whatever its version suffix happens to be, since that changes
/// with every FFmpeg release: `avcodec-62.dll` today, something else tomorrow.
fn newest_matching(directory: &Path, library: &str) -> Option<PathBuf> {
    let prefix = format!("{library}-");

    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "dll"))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .max()
}
