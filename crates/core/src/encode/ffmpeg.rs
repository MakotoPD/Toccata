// SPDX-License-Identifier: GPL-3.0-or-later

//! One encoder, driven by a table.
//!
//! Every format here does the same thing: open a container, open a codec, feed
//! it frames of the audio that came off the disc. What differs is the codec,
//! the container and whether the codec wants its samples arranged some other
//! way. That is three fields, not seven files.
//!
//! A CD hands over interleaved 16 bit stereo. FLAC and PCM take that as it is,
//! which keeps the lossless path free of any conversion; ALAC, AAC, MP3 and
//! Vorbis all want planar or floating point samples, so those go through a
//! resampler first. Since the rate and the channels never change, the resampler
//! only rearranges — it does not resample, and holds nothing back.

use std::io::{self, Write};
use std::path::Path;

use ffmpeg::format::sample::{Sample, Type};
use ffmpeg::software::resampling;
use ffmpeg::{ChannelLayout, codec, encoder, format, frame};
use ffmpeg_next as ffmpeg;

use super::{EncodeError, Encoder, SAMPLE_RATE, prepare};
use crate::drive::BYTES_PER_SAMPLE;

/// What the disc hands over, and therefore what always goes in.
const SOURCE: Sample = Sample::I16(Type::Packed);

/// Frame size to fall back on for a codec that does not state one. Only
/// reached by codecs that accept any size, where the number is arbitrary.
const DEFAULT_FRAME_SAMPLES: usize = 4608;

/// How one format is produced.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Codec asked for by name, since `aac` and `libvorbis` are choices
    /// between several encoders for the same codec and the name is what
    /// distinguishes them.
    pub encoder: &'static str,
    /// Muxer name, forced rather than guessed: `.m4a` holds either ALAC or
    /// AAC, and the extension cannot say which.
    pub muxer: &'static str,
    /// What the user asked for, if the format has anything to ask about.
    pub quality: Option<super::Quality>,
    /// Set for codecs whose own quality scale runs downwards.
    pub invert_quality: bool,
}

/// What ffmpeg multiplies a quality number by before the codec sees it.
const QP2LAMBDA: u32 = 118;

pub struct Coder {
    output: format::context::Output,
    encoder: encoder::Audio,
    /// Present only when the codec refuses interleaved 16 bit.
    resampler: Option<resampling::Context>,
    target: Sample,
    frame_samples: usize,
    /// Audio that arrived but does not yet fill a frame.
    pending: Vec<u8>,
    /// Presentation timestamp, counted in samples, which is the time base too.
    pts: i64,
}

impl Coder {
    pub fn create(path: &Path, spec: &Spec) -> Result<Self, EncodeError> {
        prepare()?;

        let mut output =
            format::output_as(path, spec.muxer).map_err(EncodeError::during("open"))?;

        let codec = encoder::find_by_name(spec.encoder)
            .ok_or_else(|| EncodeError::MissingEncoder {
                codec: spec.encoder.to_owned(),
            })?
            .audio()
            .map_err(EncodeError::during("open"))?;

        // Taking what the codec offers rather than assuming: the same build
        // flag that drops an encoder can change what an encoder accepts.
        let target = pick_format(&codec).ok_or_else(|| EncodeError::UnsupportedInput {
            codec: spec.encoder.to_owned(),
        })?;

        // Some muxers keep the codec description in their own header rather
        // than in the stream, and say so through this flag. Read before the
        // stream exists, since that borrows the container.
        let global_header = output
            .format()
            .flags()
            .contains(format::flag::Flags::GLOBAL_HEADER);

        let encoder = {
            let mut stream = output
                .add_stream(codec)
                .map_err(EncodeError::during("addStream"))?;

            let context = codec::context::Context::from_parameters(stream.parameters())
                .map_err(EncodeError::during("addStream"))?;

            let mut encoder = context
                .encoder()
                .audio()
                .map_err(EncodeError::during("addStream"))?;

            encoder.set_rate(SAMPLE_RATE);
            encoder.set_format(target);
            encoder.set_channel_layout(ChannelLayout::STEREO);
            encoder.set_time_base((1, SAMPLE_RATE));
            stream.set_time_base((1, SAMPLE_RATE));

            if global_header {
                encoder.set_flags(codec::flag::Flags::GLOBAL_HEADER);
            }

            let mut options = ffmpeg::Dictionary::new();

            match spec.quality {
                Some(super::Quality::Bitrate { kbps }) => {
                    encoder.set_bit_rate(kbps as usize * 1000);
                }
                Some(super::Quality::Compression { level }) => {
                    options.set("compression_level", &level.to_string());
                }
                // Variable rate goes through the codec context's own options
                // rather than a setter, since that is where ffmpeg keeps it.
                Some(super::Quality::Variable { quality }) => {
                    let scale = match spec.invert_quality {
                        true => 9u32.saturating_sub(quality),
                        false => quality,
                    };

                    options.set("flags", "+qscale");
                    options.set("global_quality", &(scale * QP2LAMBDA).to_string());
                }
                None => {}
            }

            let encoder = encoder
                .open_as_with(codec, options)
                .map_err(EncodeError::during("openEncoder"))?;

            stream.set_parameters(&encoder);
            encoder
        };

        output
            .write_header()
            .map_err(EncodeError::during("writeHeader"))?;

        let resampler = if target == SOURCE {
            None
        } else {
            Some(
                resampling::Context::get(
                    SOURCE,
                    ChannelLayout::STEREO,
                    SAMPLE_RATE as u32,
                    target,
                    ChannelLayout::STEREO,
                    SAMPLE_RATE as u32,
                )
                .map_err(EncodeError::during("openResampler"))?,
            )
        };

        let frame_samples = match encoder.frame_size() {
            0 => DEFAULT_FRAME_SAMPLES,
            size => size as usize,
        };

        Ok(Self {
            output,
            encoder,
            resampler,
            target,
            frame_samples,
            pending: Vec::with_capacity(frame_samples * BYTES_PER_SAMPLE),
            pts: 0,
        })
    }

    /// Hands one frame to the codec and writes out whatever comes back. `pcm`
    /// is interleaved 16 bit stereo and is short of a full frame only at the
    /// very end.
    fn encode(&mut self, pcm: &[u8]) -> Result<(), EncodeError> {
        let samples = pcm.len() / BYTES_PER_SAMPLE;
        let mut source = frame::Audio::new(SOURCE, samples, ChannelLayout::STEREO);

        source.set_rate(SAMPLE_RATE as u32);
        source.data_mut(0)[..pcm.len()].copy_from_slice(pcm);

        let frame = match self.resampler.as_mut() {
            None => {
                source.set_pts(Some(self.pts));
                source
            }
            Some(resampler) => {
                let mut converted = frame::Audio::new(self.target, samples, ChannelLayout::STEREO);

                resampler
                    .run(&source, &mut converted)
                    .map_err(EncodeError::during("resample"))?;

                converted.set_pts(Some(self.pts));
                converted
            }
        };

        self.pts += samples as i64;

        self.encoder
            .send_frame(&frame)
            .map_err(EncodeError::during("sendFrame"))?;

        self.drain()
    }

    fn drain(&mut self) -> Result<(), EncodeError> {
        let mut packet = ffmpeg::Packet::empty();

        while self.encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(0);
            packet
                .write_interleaved(&mut self.output)
                .map_err(EncodeError::during("writePacket"))?;
        }

        Ok(())
    }
}

/// Interleaved 16 bit when the codec takes it, since that needs no conversion
/// at all; failing that, 16 bit in any arrangement.
///
/// The order matters more than it looks. ALAC lists `s32p` first, and taking
/// the first entry makes it write 24 bit files out of a 16 bit disc — larger,
/// no more faithful, and rejected by some players. Only when nothing 16 bit is
/// offered, as with AAC and Vorbis, does the codec's own first choice win.
fn pick_format(codec: &codec::Audio) -> Option<Sample> {
    let offered: Vec<Sample> = codec.formats()?.collect();

    offered
        .iter()
        .find(|entry| **entry == SOURCE)
        .or_else(|| offered.iter().find(|entry| matches!(entry, Sample::I16(_))))
        .or_else(|| offered.first())
        .copied()
}

impl Write for Coder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);

        let full = self.frame_samples * BYTES_PER_SAMPLE;
        while self.pending.len() >= full {
            // The tail is kept rather than copied out, so a rip handing over
            // odd sized chunks costs one move per frame, not per write.
            let rest = self.pending.split_off(full);
            let frame = std::mem::replace(&mut self.pending, rest);

            self.encode(&frame).map_err(io::Error::other)?;
        }

        Ok(buf.len())
    }

    /// Deliberately does nothing to the codec: flushing a compressed stream
    /// halfway would end it. What is still owed is written by `finish`.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Encoder for Coder {
    fn finish(mut self: Box<Self>) -> Result<(), EncodeError> {
        if !self.pending.is_empty() {
            let last = std::mem::take(&mut self.pending);
            self.encode(&last)?;
        }

        self.encoder
            .send_eof()
            .map_err(EncodeError::during("sendEof"))?;
        self.drain()?;

        self.output
            .write_trailer()
            .map_err(EncodeError::during("writeTrailer"))
    }
}
