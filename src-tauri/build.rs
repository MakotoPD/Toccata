// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

/// The libraries this application links against. The rest of an FFmpeg package
/// is video and filtering, which nothing here touches.
const LIBRARIES: [&str; 4] = ["avcodec", "avformat", "avutil", "swresample"];

fn main() {
    // Before Tauri, not after: the bundler checks that every declared resource
    // exists, and on Windows the resources are the libraries collected here.
    // Running it the other way round fails the build on a fresh checkout.
    carry_ffmpeg();
    tauri_build::build();
}

/// Puts the FFmpeg libraries where both the bundler and Windows will find them.
///
/// Two copies, for two different readers. `src-tauri/ffmpeg` is what the
/// Windows bundle declares as a resource, so a release carries them; beside the
/// executable is where Windows looks when the program starts, so `cargo run`
/// works from an ordinary shell rather than only from one that happens to have
/// the FFmpeg package on its PATH.
///
/// Does nothing where the libraries come from the system, which is everywhere
/// except Windows, and nothing at all without `FFMPEG_DIR` — where the build
/// has already failed for want of the headers.
fn carry_ffmpeg() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");

    if !cfg!(windows) {
        return;
    }

    let Ok(source) = std::env::var("FFMPEG_DIR") else {
        return;
    };

    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg");
    let _ = std::fs::create_dir_all(&bundled);

    for library in LIBRARIES {
        let Some(found) = newest_matching(&Path::new(&source).join("bin"), library) else {
            continue;
        };

        let Some(name) = found.file_name() else {
            continue;
        };

        // Copying over one that is already there is cheap, and keeps the copy
        // honest when the package underneath gets updated.
        let _ = std::fs::copy(&found, bundled.join(name));

        if let Some(target) = target_directory() {
            let _ = std::fs::copy(&found, target.join(name));
        }
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
