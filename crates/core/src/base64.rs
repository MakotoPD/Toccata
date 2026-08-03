// SPDX-License-Identifier: GPL-3.0-or-later

//! Base64, with the alphabet left to the caller.
//!
//! Two encodings are needed and they disagree on three characters: data URIs
//! use the standard one, while MusicBrainz disc IDs swap `+/=` for `._-`.

pub const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8], alphabet: &[u8; 64], padding: char) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);

    for chunk in data.chunks(3) {
        let packed = chunk
            .iter()
            .chain(std::iter::repeat(&0))
            .take(3)
            .fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte));

        for position in 0..4 {
            if position <= chunk.len() {
                let index = (packed >> (18 - position * 6)) & 0x3f;
                out.push(alphabet[index as usize] as char);
            } else {
                out.push(padding);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_examples_in_rfc_4648() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(input.as_bytes(), STANDARD, '='), expected);
        }
    }

    #[test]
    fn honours_a_different_padding_character() {
        assert_eq!(encode(b"f", STANDARD, '-'), "Zg--");
    }
}
