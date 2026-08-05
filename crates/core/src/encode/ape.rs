// SPDX-License-Identifier: GPL-3.0-or-later

//! Monkey's Audio, behind the `ape` feature.
//!
//! FFmpeg decodes APE and does not encode it, so this is the one format that
//! cannot go through the same path as the rest. The only encoder is the one in
//! the official Monkey's Audio SDK, whose licence has historically carried
//! terms that are not GPL compatible — which is why the SDK is not vendored
//! here, is not fetched by the build, and is not on by default.
//!
//! Building with `--features ape` means you have read the SDK's current
//! licence yourself and satisfied yourself that distributing the result is
//! allowed. `MONKEYS_AUDIO_DIR` points at the unpacked SDK.
//!
//! Without the feature the application builds and runs exactly as before, one
//! format short.

use std::io::{self, Write};
use std::path::Path;

use super::{EncodeError, Encoder};

pub struct Ape;

impl Ape {
    pub fn create(_path: &Path) -> Result<Self, EncodeError> {
        // The bindings live in a crate that only exists when the SDK is
        // present, so there is nothing to call until it is wired up.
        Err(EncodeError::MissingEncoder {
            codec: "ape".to_owned(),
        })
    }
}

impl Write for Ape {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        unreachable!("nothing can be written to an encoder that never opened")
    }

    fn flush(&mut self) -> io::Result<()> {
        unreachable!("nothing can be written to an encoder that never opened")
    }
}

impl Encoder for Ape {
    fn finish(self: Box<Self>) -> Result<(), EncodeError> {
        unreachable!("nothing can be written to an encoder that never opened")
    }
}
