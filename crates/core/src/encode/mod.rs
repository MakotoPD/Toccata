// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning the audio coming off the disc into files.
//!
//! Everything here takes the same input, because a CD only ever holds one
//! thing: 16 bit stereo at 44100 Hz, interleaved, little endian. An encoder is
//! therefore a [`std::io::Write`] that happens to compress, which lets the rip
//! hand its bytes to any number of them without knowing what they are.
//!
//! Finishing is not flushing. A codec holds frames back and a container has a
//! trailer to write, so an encoder that is merely dropped leaves a file that
//! looks complete and is not. [`Encoder::finish`] is the only correct ending.

pub mod flac;

use std::sync::OnceLock;

use serde::Serialize;

/// Sample rate of every audio CD ever pressed.
pub const SAMPLE_RATE: i32 = 44_100;

/// Like every other error that leaves this crate, this is a code and its
/// parameters. The frontend decides what to say about it.
#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EncodeError {
    #[error("this ffmpeg build has no {codec} encoder")]
    MissingEncoder { codec: String },

    #[error("the {codec} encoder does not accept 16 bit stereo")]
    UnsupportedInput { codec: String },

    // Named `status` rather than `code`, which is taken by the tag that tells
    // these variants apart on the way to the frontend.
    #[error("ffmpeg refused {operation} with status {status}")]
    Ffmpeg { operation: String, status: i32 },

    #[error("writing the encoded audio failed")]
    Write,
}

impl EncodeError {
    fn during(operation: &'static str) -> impl Fn(ffmpeg_next::Error) -> Self {
        move |error| Self::Ffmpeg {
            operation: operation.to_owned(),
            status: error.into(),
        }
    }
}

/// What every encoder in this module can do. The trait proper, with a registry
/// and several formats behind it, comes once there is more than one of them;
/// for now this is the shape they all have to fit.
pub trait Encoder: std::io::Write {
    /// Drains the codec, writes the container trailer and closes the file.
    fn finish(self: Box<Self>) -> Result<(), EncodeError>;
}

/// ffmpeg wants setting up once per process, and no caller should have to know
/// whether somebody else got there first.
pub(crate) fn prepare() -> Result<(), EncodeError> {
    static OUTCOME: OnceLock<Result<(), i32>> = OnceLock::new();

    match OUTCOME.get_or_init(|| ffmpeg_next::init().map_err(Into::into)) {
        Ok(()) => Ok(()),
        Err(status) => Err(EncodeError::Ffmpeg {
            operation: "init".to_owned(),
            status: *status,
        }),
    }
}
