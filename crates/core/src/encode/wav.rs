// SPDX-License-Identifier: GPL-3.0-or-later

//! WAV, which is the audio exactly as it left the disc.
//!
//! Nothing is encoded here, so this deliberately does not go through FFmpeg:
//! the format is a 44 byte header in front of the bytes the drive handed over,
//! and a dependency to copy bytes would be a strange thing to add.
//!
//! The header states the length of audio that follows, and that is not known
//! until the last sector is in. It is therefore written with zeroes and
//! corrected on the way out, which is what every other writer does too.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use super::{EncodeError, Encoder, SAMPLE_RATE};

const HEADER_BYTES: u32 = 44;
const CHANNELS: u16 = 2;
const BITS: u16 = 16;

pub struct Wav {
    output: BufWriter<File>,
    bytes: u32,
}

impl Wav {
    pub fn create(path: &Path) -> Result<Self, EncodeError> {
        let file = File::create(path).map_err(|_| EncodeError::Write)?;
        let mut output = BufWriter::new(file);

        write_header(&mut output, 0).map_err(|_| EncodeError::Write)?;

        Ok(Self { output, bytes: 0 })
    }
}

impl Write for Wav {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.output.write(buf)?;
        self.bytes += written as u32;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl Encoder for Wav {
    fn finish(mut self: Box<Self>) -> Result<(), EncodeError> {
        self.output.flush().map_err(|_| EncodeError::Write)?;

        let mut file = self.output.into_inner().map_err(|_| EncodeError::Write)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| EncodeError::Write)?;
        write_header(&mut file, self.bytes).map_err(|_| EncodeError::Write)?;
        file.flush().map_err(|_| EncodeError::Write)
    }
}

/// A CD is already 16 bit stereo at 44100 Hz, so the samples go into the file
/// untouched and only this has to be produced. `data` is the length of the
/// audio in bytes.
pub(crate) fn write_header<W: Write>(output: &mut W, data: u32) -> io::Result<()> {
    let rate = SAMPLE_RATE as u32;
    let byte_rate = rate * u32::from(CHANNELS) * u32::from(BITS / 8);

    output.write_all(b"RIFF")?;
    output.write_all(&(HEADER_BYTES - 8 + data).to_le_bytes())?;
    output.write_all(b"WAVEfmt ")?;
    output.write_all(&16u32.to_le_bytes())?;
    output.write_all(&1u16.to_le_bytes())?;
    output.write_all(&CHANNELS.to_le_bytes())?;
    output.write_all(&rate.to_le_bytes())?;
    output.write_all(&byte_rate.to_le_bytes())?;
    output.write_all(&(CHANNELS * BITS / 8).to_le_bytes())?;
    output.write_all(&BITS.to_le_bytes())?;
    output.write_all(b"data")?;
    output.write_all(&data.to_le_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::BYTES_PER_SAMPLE;

    #[test]
    fn the_header_describes_the_audio_that_follows() {
        let mut out = Vec::new();
        write_header(&mut out, 588 * BYTES_PER_SAMPLE as u32).unwrap();

        assert_eq!(out.len(), HEADER_BYTES as usize);
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WAVE");
        assert_eq!(&out[36..40], b"data");

        let data = u32::from_le_bytes(out[40..44].try_into().unwrap());
        assert_eq!(data, 588 * BYTES_PER_SAMPLE as u32);
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()), 36 + data);
        assert_eq!(u32::from_le_bytes(out[24..28].try_into().unwrap()), 44_100);
    }

    // The length is unknown while writing, so the header is only right if
    // `finish` goes back for it.
    #[test]
    fn the_length_is_corrected_once_the_audio_is_in() {
        let path = std::env::temp_dir().join("toccata-wav-length.wav");
        let audio = vec![7u8; 1000 * BYTES_PER_SAMPLE];

        let mut wav = Box::new(Wav::create(&path).expect("the file opens"));
        wav.write_all(&audio).expect("the audio is taken");
        wav.finish().expect("the file is finished");

        let written = std::fs::read(&path).expect("the file exists");
        let _ = std::fs::remove_file(&path);

        assert_eq!(written.len(), HEADER_BYTES as usize + audio.len());
        assert_eq!(
            u32::from_le_bytes(written[40..44].try_into().unwrap()),
            audio.len() as u32
        );
        assert_eq!(&written[HEADER_BYTES as usize..], &audio[..]);
    }
}
