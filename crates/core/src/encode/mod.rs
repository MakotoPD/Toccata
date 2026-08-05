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
pub mod wav;

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

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

/// What every encoder in this module can do.
pub trait Encoder: std::io::Write {
    /// Drains the codec, writes the container trailer and closes the file.
    fn finish(self: Box<Self>) -> Result<(), EncodeError>;
}

/// The formats a rip can be written to. The rest of the table in the brief
/// arrives one implementation at a time; nothing outside this enum has to
/// change when it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    /// Lossless and compressed, which is what a rip is for.
    #[default]
    Flac,
    /// The samples with a header in front and nothing else done to them.
    Wav,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Flac => "flac",
            Self::Wav => "wav",
        }
    }

    pub fn create(self, path: &std::path::Path) -> Result<Box<dyn Encoder>, EncodeError> {
        Ok(match self {
            Self::Flac => Box::new(flac::Flac::create(path, flac::MAX_COMPRESSION)?),
            Self::Wav => Box::new(wav::Wav::create(path)?),
        })
    }
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
