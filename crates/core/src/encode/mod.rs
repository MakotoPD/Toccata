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

#[cfg(feature = "ape")]
pub mod ape;
pub mod ffmpeg;
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

/// Several encoders fed from one read of the disc.
///
/// The point is the disc, not the CPU: reading it again to produce a second
/// format would take another few minutes and put another few minutes of wear
/// on the drive, while encoding the same bytes twice costs almost nothing next
/// to that. So the samples go to every encoder as they arrive.
pub struct Fanout {
    encoders: Vec<Box<dyn Encoder>>,
}

impl Fanout {
    pub fn new(encoders: Vec<Box<dyn Encoder>>) -> Self {
        Self { encoders }
    }

    /// Closes every encoder, and reports the first refusal rather than the
    /// last: all of them are given the chance to finish, since a file left
    /// without its trailer is unplayable.
    pub fn finish(self) -> Result<(), EncodeError> {
        let mut first = None;

        for encoder in self.encoders {
            if let Err(error) = encoder.finish() {
                first.get_or_insert(error);
            }
        }

        match first {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl std::io::Write for Fanout {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for encoder in &mut self.encoders {
            encoder.write_all(buf)?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        for encoder in &mut self.encoders {
            encoder.flush()?;
        }

        Ok(())
    }
}

/// The formats a rip can be written to.
///
/// `M4a` and `M4aAac` are separate on purpose: M4A is a container, not a
/// codec, and "m4a" on its own gets the user something other than what they
/// thought they asked for.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    /// Lossless and compressed, which is what a rip is for.
    #[default]
    Flac,
    /// The samples with a header in front and nothing else done to them.
    Wav,
    Aiff,
    /// Apple Lossless, in the container Apple puts it in.
    M4a,
    /// AAC, in the same container, which is why the two are named apart.
    M4aAac,
    Mp3,
    /// AAC on its own, in a raw stream rather than a container.
    Aac,
    OggVorbis,
    /// Only present when built with the `ape` feature, which needs an SDK that
    /// cannot be shipped with the source.
    #[cfg(feature = "ape")]
    Ape,
}

/// What a format lets the user decide, and therefore what the encoder panel
/// puts on screen. Described here rather than in the interface so that adding
/// a format never means editing two places.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Tuning {
    /// Nothing to decide: the samples are written exactly as the disc holds
    /// them, and any knob here would only be a way of making them worse.
    Untouched,

    /// How hard a lossless codec works. Higher is smaller and slower, and
    /// never costs a single sample.
    Compression { max: u32, default: u32 },

    /// A lossy codec, where the choice is between a fixed rate and the codec's
    /// own variable scale. `max_quality` of zero means it has no such scale.
    Lossy {
        default_kbps: u32,
        max_quality: u32,
        default_quality: u32,
    },
}

/// What the user chose for one format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Quality {
    Compression {
        level: u32,
    },
    Bitrate {
        kbps: u32,
    },
    /// The codec's own scale, where a higher number is better and the rate
    /// follows the music rather than the clock.
    Variable {
        quality: u32,
    },
}

/// Every format, in the order the interface lists them.
pub const ALL: &[Format] = &[
    Format::Flac,
    Format::Wav,
    Format::Aiff,
    Format::M4a,
    Format::M4aAac,
    Format::Mp3,
    Format::Aac,
    Format::OggVorbis,
    #[cfg(feature = "ape")]
    Format::Ape,
];

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::M4a | Self::M4aAac => "m4a",
            Self::Mp3 => "mp3",
            Self::Aac => "aac",
            Self::OggVorbis => "ogg",
            #[cfg(feature = "ape")]
            Self::Ape => "ape",
        }
    }

    /// Whether anything is thrown away. Decides both what the interface offers
    /// to configure and whether a bit rate means anything.
    pub fn lossy(self) -> bool {
        matches!(self, Self::M4aAac | Self::Mp3 | Self::Aac | Self::OggVorbis)
    }

    /// What the format is called in the interface. Codec and format names are
    /// never translated, so this doubles as the label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
            Self::Aiff => "AIFF",
            Self::M4a => "M4A (ALAC)",
            Self::M4aAac => "M4A (AAC)",
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::OggVorbis => "Ogg Vorbis",
            #[cfg(feature = "ape")]
            Self::Ape => "APE",
        }
    }

    /// Folder a format goes into when several were asked for at once.
    ///
    /// Needed because two of them share an extension: `.m4a` holds either ALAC
    /// or AAC, so writing both into one folder would have them overwrite each
    /// other. Separating all of them keeps the layout predictable rather than
    /// having one format behave differently from the rest.
    pub fn folder(self) -> &'static str {
        match self {
            Self::M4a => "M4A-ALAC",
            Self::M4aAac => "M4A-AAC",
            Self::OggVorbis => "Ogg-Vorbis",
            other => other.label(),
        }
    }

    /// What this format lets the user decide.
    pub fn tuning(self) -> Tuning {
        match self {
            // Twelve costs nothing here: the drive is tens of times slower
            // than any of these levels, so the wall clock never notices.
            Self::Flac => Tuning::Compression {
                max: 12,
                default: 12,
            },
            #[cfg(feature = "ape")]
            Self::Ape => Tuning::Compression { max: 5, default: 2 },

            // LAME's scale runs the other way round, so it is turned here and
            // the interface only ever sees "higher is better".
            Self::Mp3 => Tuning::Lossy {
                default_kbps: 320,
                max_quality: 9,
                default_quality: 7,
            },
            Self::OggVorbis => Tuning::Lossy {
                default_kbps: 192,
                max_quality: 10,
                default_quality: 6,
            },
            // FFmpeg's own AAC encoder has a variable mode that its authors
            // call experimental, so only the fixed rate is offered.
            Self::M4aAac | Self::Aac => Tuning::Lossy {
                default_kbps: 256,
                max_quality: 0,
                default_quality: 0,
            },

            Self::Wav | Self::Aiff | Self::M4a => Tuning::Untouched,
        }
    }

    /// What the format falls back to when the settings say nothing.
    pub fn default_quality(self) -> Option<Quality> {
        match self.tuning() {
            Tuning::Untouched => None,
            Tuning::Compression { default, .. } => Some(Quality::Compression { level: default }),
            Tuning::Lossy { default_kbps, .. } => Some(Quality::Bitrate { kbps: default_kbps }),
        }
    }

    fn spec(self, quality: Option<Quality>) -> Option<ffmpeg::Spec> {
        let quality = quality.or_else(|| self.default_quality());

        // `libvorbis` and `libmp3lame` are asked for by name because FFmpeg
        // also carries its own encoders for both codecs, and both are worse.
        let (encoder, muxer) = match self {
            Self::Flac => ("flac", "flac"),
            Self::Aiff => ("pcm_s16be", "aiff"),
            Self::M4a => ("alac", "ipod"),
            Self::M4aAac => ("aac", "ipod"),
            Self::Mp3 => ("libmp3lame", "mp3"),
            Self::Aac => ("aac", "adts"),
            Self::OggVorbis => ("libvorbis", "ogg"),
            Self::Wav => return None,
            #[cfg(feature = "ape")]
            Self::Ape => return None,
        };

        Some(ffmpeg::Spec {
            encoder,
            muxer,
            quality,
            // LAME counts down from 0 while everything else counts up, so the
            // interface's scale is turned round here rather than in the panel.
            invert_quality: self == Self::Mp3,
        })
    }

    pub fn create(self, path: &std::path::Path) -> Result<Box<dyn Encoder>, EncodeError> {
        self.create_with(path, None)
    }

    /// Falls back to the format's own default when `quality` says nothing, or
    /// says something the format has no use for.
    pub fn create_with(
        self,
        path: &std::path::Path,
        quality: Option<Quality>,
    ) -> Result<Box<dyn Encoder>, EncodeError> {
        match self.spec(quality) {
            Some(spec) => Ok(Box::new(ffmpeg::Coder::create(path, &spec)?)),
            #[cfg(feature = "ape")]
            None if self == Self::Ape => Ok(Box::new(ape::Ape::create(path)?)),
            None => Ok(Box::new(wav::Wav::create(path)?)),
        }
    }
}

/// Audio for the tests: a whole number of cycles of a 441 Hz sine, so there is
/// no discontinuity at either end for a codec to trip over.
#[cfg(test)]
pub(crate) fn tone(seconds: u32) -> Vec<u8> {
    use crate::drive::BYTES_PER_SAMPLE;

    let total = SAMPLE_RATE as usize * seconds as usize;
    let mut pcm = Vec::with_capacity(total * BYTES_PER_SAMPLE);

    for index in 0..total {
        let phase = index as f32 / SAMPLE_RATE as f32 * 441.0 * std::f32::consts::TAU;
        let value = (phase.sin() * 12000.0) as i16;

        pcm.extend_from_slice(&value.to_le_bytes());
        pcm.extend_from_slice(&value.to_le_bytes());
    }

    pcm
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn encode(format: Format, name: &str, pcm: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "toccata-{name}-{}.{}",
            pcm.len(),
            format.extension()
        ));
        let _ = std::fs::remove_file(&path);

        let mut encoder = format
            .create(&path)
            .unwrap_or_else(|error| panic!("{} opens: {error}", format.label()));

        for piece in pcm.chunks(4096) {
            encoder.write_all(piece).expect("the audio is taken");
        }

        encoder
            .finish()
            .unwrap_or_else(|error| panic!("{} finishes: {error}", format.label()));

        path
    }

    /// Every format has to produce a file a decoder recognises, holding about
    /// as much audio as went in. A file that opens but is half empty is the
    /// failure worth catching, since nothing else would notice it.
    #[test]
    fn every_format_writes_back_the_audio_it_was_given() {
        let pcm = tone(1);

        for format in ALL {
            let path = encode(*format, "all", &pcm);
            let decoded = decode(&path);
            let _ = std::fs::remove_file(&path);

            // The lossy codecs pad the start with their own delay, so the
            // length is compared loosely rather than exactly.
            let difference = decoded.len().abs_diff(pcm.len());
            assert!(
                difference < pcm.len() / 10,
                "{} decoded to {} bytes from {}",
                format.label(),
                decoded.len(),
                pcm.len()
            );
        }
    }

    // The lossless formats are the reason this application exists, so they get
    // checked rather than assumed: what comes back has to be what went in.
    #[test]
    fn the_lossless_formats_give_back_exactly_what_they_took() {
        let pcm = tone(1);

        for format in [Format::Flac, Format::Wav, Format::Aiff, Format::M4a] {
            let path = encode(format, "lossless", &pcm);
            let decoded = decode(&path);
            let _ = std::fs::remove_file(&path);

            assert_eq!(
                decoded.len(),
                pcm.len(),
                "{} changed the length",
                format.label()
            );
            assert_eq!(decoded, pcm, "{} changed the samples", format.label());
        }
    }

    #[test]
    fn a_lossy_format_is_smaller_than_the_audio_it_was_given() {
        let pcm = tone(2);

        for format in ALL.iter().filter(|format| format.lossy()) {
            let path = encode(*format, "lossy", &pcm);
            let written = std::fs::metadata(&path).expect("the file exists").len();
            let _ = std::fs::remove_file(&path);

            assert!(
                (written as usize) < pcm.len() / 2,
                "{} came out at {written} from {}",
                format.label(),
                pcm.len()
            );
        }
    }

    // Both write a file called `.m4a`, and the whole reason they are separate
    // entries is that the container does not say which codec is inside.
    #[test]
    fn the_two_m4a_entries_hold_different_codecs() {
        let pcm = tone(1);

        let alac = encode(Format::M4a, "alac", &pcm);
        let aac = encode(Format::M4aAac, "aac", &pcm);

        let (lossless, lossy) = (codec_of(&alac), codec_of(&aac));

        let _ = std::fs::remove_file(&alac);
        let _ = std::fs::remove_file(&aac);

        assert_eq!(lossless, ffmpeg_next::codec::Id::ALAC);
        assert_eq!(lossy, ffmpeg_next::codec::Id::AAC);
    }

    // The disc is read once, so what every format gets has to be identical to
    // what a single format would have got.
    #[test]
    fn a_fanout_writes_the_same_files_one_encoder_would_have() {
        let pcm = tone(1);
        let wanted = [Format::Flac, Format::Mp3, Format::OggVorbis];

        let paths: Vec<std::path::PathBuf> = wanted
            .iter()
            .map(|format| {
                std::env::temp_dir().join(format!("toccata-fanout.{}", format.extension()))
            })
            .collect();

        let mut fanout = Fanout::new(
            wanted
                .iter()
                .zip(&paths)
                .map(|(format, path)| format.create(path).expect("the encoder opens"))
                .collect(),
        );

        for piece in pcm.chunks(4096) {
            fanout.write_all(piece).expect("the audio is taken");
        }
        fanout.finish().expect("every file is finished");

        for (format, path) in wanted.iter().zip(&paths) {
            let alone = encode(*format, "alone", &pcm);

            // The audio is compared rather than the bytes: the Ogg muxer
            // stamps every stream with a serial number of its own choosing, so
            // two identical encodes never produce identical files.
            let together = decode(path);
            let separately = decode(&alone);

            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(&alone);

            assert_eq!(
                together,
                separately,
                "{} differs when written alongside others",
                format.label()
            );
        }
    }

    // The knobs have to actually reach the codec. A compression level that is
    // quietly ignored looks exactly like one that works.
    #[test]
    fn compression_level_changes_the_size_without_changing_the_audio() {
        let pcm = tone(2);

        let sizes: Vec<u64> = [0, 12]
            .into_iter()
            .map(|level| {
                let path = std::env::temp_dir().join(format!("toccata-level-{level}.flac"));
                let _ = std::fs::remove_file(&path);

                let mut encoder = Format::Flac
                    .create_with(&path, Some(Quality::Compression { level }))
                    .expect("the encoder opens");

                encoder.write_all(&pcm).expect("the audio is taken");
                encoder.finish().expect("the file is finished");

                assert_eq!(decode(&path), pcm, "level {level} is still lossless");

                let size = std::fs::metadata(&path).expect("the file exists").len();
                let _ = std::fs::remove_file(&path);
                size
            })
            .collect();

        assert!(
            sizes[1] < sizes[0],
            "level 12 should beat level 0, got {} against {}",
            sizes[1],
            sizes[0]
        );
    }

    #[test]
    fn a_lossy_format_follows_the_bit_rate_it_was_given() {
        let pcm = tone(4);

        let sizes: Vec<u64> = [128, 320]
            .into_iter()
            .map(|kbps| {
                let path = std::env::temp_dir().join(format!("toccata-rate-{kbps}.mp3"));
                let _ = std::fs::remove_file(&path);

                let mut encoder = Format::Mp3
                    .create_with(&path, Some(Quality::Bitrate { kbps }))
                    .expect("the encoder opens");

                encoder.write_all(&pcm).expect("the audio is taken");
                encoder.finish().expect("the file is finished");

                let size = std::fs::metadata(&path).expect("the file exists").len();
                let _ = std::fs::remove_file(&path);
                size
            })
            .collect();

        assert!(
            sizes[1] > sizes[0] * 2,
            "320k should be far larger than 128k, got {} against {}",
            sizes[1],
            sizes[0]
        );
    }

    // Variable rate goes in through the codec context's options rather than a
    // setter, so it is the setting most likely to be silently ignored.
    #[test]
    fn variable_quality_reaches_the_codec() {
        let pcm = tone(4);

        let sizes: Vec<u64> = [0, 9]
            .into_iter()
            .map(|quality| {
                let path = std::env::temp_dir().join(format!("toccata-vbr-{quality}.mp3"));
                let _ = std::fs::remove_file(&path);

                let mut encoder = Format::Mp3
                    .create_with(&path, Some(Quality::Variable { quality }))
                    .expect("the encoder opens");

                encoder.write_all(&pcm).expect("the audio is taken");
                encoder.finish().expect("the file is finished");

                let size = std::fs::metadata(&path).expect("the file exists").len();
                let _ = std::fs::remove_file(&path);
                size
            })
            .collect();

        assert!(
            sizes[1] > sizes[0],
            "quality 9 should be larger than quality 0, got {} against {}",
            sizes[1],
            sizes[0]
        );
    }

    fn codec_of(path: &std::path::Path) -> ffmpeg_next::codec::Id {
        use ffmpeg_next as ff;

        prepare().expect("ffmpeg starts");

        let input = ff::format::input(path).expect("the file opens");
        input
            .streams()
            .best(ff::media::Type::Audio)
            .expect("the file has audio")
            .parameters()
            .id()
    }

    /// Reads a file back as interleaved 16 bit stereo, whatever it holds.
    fn decode(path: &std::path::Path) -> Vec<u8> {
        use ffmpeg_next as ff;

        prepare().expect("ffmpeg starts");

        let mut input = ff::format::input(path).expect("the file opens");
        let stream = input
            .streams()
            .best(ff::media::Type::Audio)
            .expect("the file has audio");

        let index = stream.index();
        let context = ff::codec::context::Context::from_parameters(stream.parameters())
            .expect("the parameters are usable");
        let mut decoder = context.decoder().audio().expect("it decodes");

        let mut pcm = Vec::new();
        let collect = |decoder: &mut ff::decoder::Audio, pcm: &mut Vec<u8>| {
            let mut frame = ff::frame::Audio::empty();
            while decoder.receive_frame(&mut frame).is_ok() {
                interleave(&frame, pcm);
            }
        };

        for (stream, packet) in input.packets() {
            if stream.index() == index {
                decoder.send_packet(&packet).expect("the packet decodes");
                collect(&mut decoder, &mut pcm);
            }
        }

        decoder.send_eof().expect("the decoder drains");
        collect(&mut decoder, &mut pcm);

        pcm
    }

    /// Flattens a decoded frame into interleaved 16 bit stereo by hand.
    ///
    /// swresample would do this, but only after being told a channel layout
    /// that matches the one ffmpeg keeps on the frame, and getting that
    /// agreement through the wrapper is more trouble than the arithmetic.
    fn interleave(frame: &ffmpeg_next::frame::Audio, out: &mut Vec<u8>) {
        use crate::drive::BYTES_PER_SAMPLE;
        use ffmpeg_next::format::sample::{Sample, Type};

        let samples = frame.samples();

        let plane = |index: usize, width: usize| -> &[u8] { &frame.data(index)[..samples * width] };

        match frame.format() {
            Sample::I16(Type::Packed) => {
                out.extend_from_slice(plane(0, BYTES_PER_SAMPLE));
            }
            Sample::I16(Type::Planar) => {
                let (left, right) = (plane(0, 2), plane(1, 2));
                for index in 0..samples {
                    out.extend_from_slice(&left[index * 2..index * 2 + 2]);
                    out.extend_from_slice(&right[index * 2..index * 2 + 2]);
                }
            }
            // A 16 bit stream widened by the decoder rather than by the
            // encoder: the low half is zero, so the high half is the original.
            Sample::I32(Type::Planar) => {
                let (left, right) = (plane(0, 4), plane(1, 4));
                for index in 0..samples {
                    for channel in [left, right] {
                        out.extend_from_slice(&channel[index * 4 + 2..index * 4 + 4]);
                    }
                }
            }
            Sample::F32(Type::Planar) => {
                let (left, right) = (plane(0, 4), plane(1, 4));
                for index in 0..samples {
                    for channel in [left, right] {
                        let bytes: [u8; 4] = channel[index * 4..index * 4 + 4]
                            .try_into()
                            .expect("four bytes of a float");
                        let value =
                            (f32::from_le_bytes(bytes) * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        out.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
            other => panic!("nothing here decodes {other:?}"),
        }
    }
}
