// SPDX-License-Identifier: GPL-3.0-or-later

//! FLAC, which is what a rip should produce unless asked otherwise.
//!
//! The disc hands over exactly what the encoder wants — 16 bit stereo at
//! 44100 Hz — so nothing is resampled or converted on the way. That matters
//! beyond speed: a lossless format that went through a sample conversion is no
//! longer lossless.

use std::io::{self, Write};
use std::path::Path;

use ffmpeg::format::sample::{Sample, Type};
use ffmpeg::{ChannelLayout, codec, encoder, format, frame};
use ffmpeg_next as ffmpeg;

use super::{EncodeError, Encoder, SAMPLE_RATE, prepare};
use crate::drive::BYTES_PER_SAMPLE;

const NAME: &str = "flac";

/// The disc is read tens of times slower than any of these levels encode, so
/// the slowest one costs nothing in wall clock time and is simply the best
/// compression available.
pub const MAX_COMPRESSION: u8 = 12;

pub struct Flac {
    output: format::context::Output,
    encoder: encoder::Audio,
    /// Samples the codec wants per frame. FLAC has no variable frame size, so
    /// anything else is refused.
    frame_samples: usize,
    /// Audio that arrived but does not yet fill a frame.
    pending: Vec<u8>,
    /// Presentation timestamp, counted in samples, which is also the time base.
    pts: i64,
}

impl Flac {
    pub fn create(path: &Path, compression: u8) -> Result<Self, EncodeError> {
        prepare()?;

        let mut output = format::output(path).map_err(EncodeError::during("open"))?;

        let codec = encoder::find(codec::Id::FLAC)
            .ok_or_else(|| EncodeError::MissingEncoder {
                codec: NAME.to_owned(),
            })?
            .audio()
            .map_err(EncodeError::during("open"))?;

        // Asked for rather than assumed: a build without 16 bit FLAC would
        // otherwise write silence-shaped nonsense.
        let sample = Sample::I16(Type::Packed);
        let takes_sixteen_bit = codec
            .formats()
            .into_iter()
            .flatten()
            .any(|supported| supported == sample);

        if !takes_sixteen_bit {
            return Err(EncodeError::UnsupportedInput {
                codec: NAME.to_owned(),
            });
        }

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
            encoder.set_format(sample);
            encoder.set_channel_layout(ChannelLayout::STEREO);
            encoder.set_time_base((1, SAMPLE_RATE));
            stream.set_time_base((1, SAMPLE_RATE));

            let mut options = ffmpeg::Dictionary::new();
            options.set("compression_level", &compression.to_string());

            let encoder = encoder
                .open_as_with(codec, options)
                .map_err(EncodeError::during("openEncoder"))?;

            stream.set_parameters(&encoder);
            encoder
        };

        output
            .write_header()
            .map_err(EncodeError::during("writeHeader"))?;

        // FLAC has no variable frame size, so the codec dictates how much
        // audio a frame holds and everything is buffered up to it.
        let frame_samples = match encoder.frame_size() {
            0 => 4608,
            size => size as usize,
        };

        Ok(Self {
            output,
            encoder,
            frame_samples,
            pending: Vec::with_capacity(frame_samples * BYTES_PER_SAMPLE),
            pts: 0,
        })
    }

    /// Hands one frame of audio to the codec and writes out whatever comes
    /// back. `pcm` is interleaved 16 bit stereo and may be shorter than a full
    /// frame, which only happens on the last one.
    fn encode(&mut self, pcm: &[u8]) -> Result<(), EncodeError> {
        let samples = pcm.len() / BYTES_PER_SAMPLE;
        let mut frame =
            frame::Audio::new(Sample::I16(Type::Packed), samples, ChannelLayout::STEREO);

        frame.set_rate(SAMPLE_RATE as u32);
        frame.set_pts(Some(self.pts));
        frame.data_mut(0)[..pcm.len()].copy_from_slice(pcm);

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

impl Write for Flac {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);

        let full = self.frame_samples * BYTES_PER_SAMPLE;
        while self.pending.len() >= full {
            // The tail is kept rather than copied out, so a rip that hands
            // over odd sized chunks costs one move per frame, not per write.
            let rest = self.pending.split_off(full);
            let frame = std::mem::replace(&mut self.pending, rest);

            self.encode(&frame).map_err(io::Error::other)?;
        }

        Ok(buf.len())
    }

    /// Deliberately does nothing to the codec: flushing a FLAC stream halfway
    /// would end it. The audio still owed is written by `finish`.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Encoder for Flac {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One second of a 441 Hz sine, which is a whole number of cycles and so
    /// has no discontinuity at either end to confuse the encoder.
    fn tone(seconds: u32) -> Vec<u8> {
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

    /// Tests run at the same time, so a name shared between two of them means
    /// one decodes the other's audio.
    fn encode_to_temp(pcm: &[u8], chunk: usize) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let path = std::env::temp_dir().join(format!("toccata-flac-{unique}.flac"));
        let _ = std::fs::remove_file(&path);

        let mut encoder =
            Box::new(Flac::create(&path, MAX_COMPRESSION).expect("the encoder opens"));

        for piece in pcm.chunks(chunk) {
            encoder.write_all(piece).expect("the encoder takes audio");
        }

        encoder.finish().expect("the file is finished");
        path
    }

    /// Reads a FLAC file back into interleaved 16 bit stereo.
    fn decode(path: &Path) -> Vec<u8> {
        let mut input = format::input(path).expect("the file opens");
        let stream = input
            .streams()
            .best(ffmpeg::media::Type::Audio)
            .expect("the file has audio");

        let index = stream.index();
        let context = codec::context::Context::from_parameters(stream.parameters())
            .expect("the parameters are usable");
        let mut decoder = context.decoder().audio().expect("flac decodes");

        let mut pcm = Vec::new();
        let collect = |decoder: &mut ffmpeg::decoder::Audio, pcm: &mut Vec<u8>| {
            let mut frame = frame::Audio::empty();
            while decoder.receive_frame(&mut frame).is_ok() {
                let bytes = frame.samples() * BYTES_PER_SAMPLE;
                pcm.extend_from_slice(&frame.data(0)[..bytes]);
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

    // The whole point of a lossless format. If this ever fails, nothing else
    // about the rip is worth anything.
    #[test]
    fn what_comes_back_out_is_what_went_in() {
        let pcm = tone(1);
        let path = encode_to_temp(&pcm, 4096);

        let decoded = decode(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(decoded.len(), pcm.len(), "the length survived");
        assert_eq!(decoded, pcm, "every sample survived");
    }

    #[test]
    fn writes_a_flac_file_smaller_than_the_audio_it_was_given() {
        let pcm = tone(2);
        let path = encode_to_temp(&pcm, 4096);

        let written = std::fs::read(&path).expect("the file exists");
        let _ = std::fs::remove_file(&path);

        assert_eq!(&written[..4], b"fLaC", "the container is what it claims");
        assert!(
            written.len() < pcm.len(),
            "a sine wave should compress, got {} from {}",
            written.len(),
            pcm.len()
        );
    }

    // The rip hands over whatever a sector read produced, which is never the
    // codec's frame size, so the buffering between the two has to hold.
    #[test]
    fn the_result_does_not_depend_on_how_the_audio_was_chunked() {
        let pcm = tone(1);

        let read = |chunk| {
            let path = encode_to_temp(&pcm, chunk);
            let bytes = std::fs::read(&path).expect("the file exists");
            let _ = std::fs::remove_file(&path);
            bytes
        };

        let one = read(4);
        let two = read(7331);
        let whole = read(pcm.len());

        assert_eq!(one, two);
        assert_eq!(one, whole);
    }

    #[test]
    fn audio_that_does_not_fill_a_frame_is_still_written() {
        // Well under one FLAC frame, so everything rests on `finish`.
        let pcm = vec![0u8; 100 * BYTES_PER_SAMPLE];
        let path = encode_to_temp(&pcm, 4096);

        let written = std::fs::read(&path).expect("the file exists");
        let _ = std::fs::remove_file(&path);

        assert_eq!(&written[..4], b"fLaC");
        assert!(written.len() > 42, "a header alone would be shorter");
    }
}
