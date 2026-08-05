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

/// Decodes standard base64, ignoring padding and any whitespace that a data
/// URI picked up on its way through the interface. Returns nothing when a
/// character does not belong to the alphabet at all.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut packed = 0u32;
    let mut bits = 0;

    for character in text.chars() {
        if character == '=' || character.is_whitespace() {
            continue;
        }

        let value = STANDARD
            .iter()
            .position(|entry| *entry as char == character)? as u32;

        packed = (packed << 6) | value;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            out.push((packed >> bits) as u8);
        }
    }

    Some(out)
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

    #[test]
    fn decoding_undoes_encoding() {
        for input in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            let encoded = encode(input.as_bytes(), STANDARD, '=');
            assert_eq!(
                decode(&encoded).as_deref(),
                Some(input.as_bytes()),
                "{input}"
            );
        }

        // Bytes that are not text, which is what a cover actually is.
        let image: Vec<u8> = (0..=255u8).collect();
        let encoded = encode(&image, STANDARD, '=');
        assert_eq!(decode(&encoded), Some(image));
    }

    #[test]
    fn decoding_refuses_what_is_not_base64() {
        assert_eq!(decode("not base64!"), None);
        assert_eq!(decode("Zm9v\n YmFy"), Some(b"foobar".to_vec()));
    }
}
