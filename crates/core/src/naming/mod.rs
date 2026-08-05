// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning titles into names a filesystem will accept.
//!
//! The rules applied are the Windows ones, everywhere. Linux and macOS would
//! take `:` and `?` happily, but a music library ends up on a shared drive or a
//! NAS often enough that a name which only works on the machine that made it is
//! not worth the trouble.

pub mod template;

/// Longest a single path component may be. Filesystems allow 255, and leaving
/// room means a long title does not push the extension off the end.
const MAX_LENGTH: usize = 120;

/// Names MS-DOS claimed for devices. Still refused by Windows today, with or
/// without an extension, in any case.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Makes one path component out of a title. Never returns an empty string:
/// `fallback` stands in when nothing usable is left.
pub fn component(title: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut spaced = false;

    for character in title.chars() {
        let replacement = match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            control if control.is_control() => ' ',
            other => other,
        };

        // Runs of whitespace collapse, so a title full of replaced characters
        // does not turn into a wall of spaces.
        if replacement.is_whitespace() {
            if !spaced && !out.is_empty() {
                out.push(' ');
            }
            spaced = true;
            continue;
        }

        spaced = false;
        out.push(replacement);
    }

    // Windows silently strips trailing dots and spaces, which turns two
    // different titles into the same file.
    let trimmed = out.trim_end_matches([' ', '.']).trim_start();
    let mut name: String = trimmed.chars().take(MAX_LENGTH).collect();
    name = name.trim_end_matches([' ', '.']).to_owned();

    if name.is_empty() {
        return fallback.to_owned();
    }

    let stem = name.split('.').next().unwrap_or(&name).to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        name.insert(0, '_');
    }

    name
}

/// File name for one track, without an extension.
pub fn track_file(number: u8, title: &str) -> String {
    let title = component(title, "");

    if title.is_empty() {
        format!("{number:02}")
    } else {
        format!("{number:02} - {title}")
    }
}

/// Directory for one release. The disc identifier stands in when the release
/// has no name yet, so a rip never lands somewhere unrelated.
pub fn release_folder(artist: &str, title: &str, disc_id: &str) -> String {
    let artist = component(artist, "");
    let title = component(title, "");

    match (artist.is_empty(), title.is_empty()) {
        (false, false) => component(&format!("{artist} - {title}"), disc_id),
        (true, false) => title,
        (false, true) => artist,
        (true, true) => component(disc_id, "disc"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_title_that_is_already_usable() {
        assert_eq!(component("Hello Nasty", "x"), "Hello Nasty");
        assert_eq!(
            component("Trzy MC's i jeden DJ", "x"),
            "Trzy MC's i jeden DJ"
        );
    }

    #[test]
    fn replaces_what_windows_refuses() {
        assert_eq!(component("AC/DC", "x"), "AC_DC");
        assert_eq!(component(r#"Who? What: Why*"#, "x"), "Who_ What_ Why_");
        assert_eq!(component(r"back\slash", "x"), "back_slash");
        assert_eq!(component("quote\"mark", "x"), "quote_mark");
    }

    #[test]
    fn strips_what_windows_would_strip_itself() {
        // Two titles that Windows would collapse into one file.
        assert_eq!(component("Track.", "x"), "Track");
        assert_eq!(component("Track ", "x"), "Track");
        assert_ne!(component("Track..", "x"), "Track.");
    }

    #[test]
    fn collapses_whitespace_and_control_characters() {
        assert_eq!(component("a\t\tb", "x"), "a b");
        assert_eq!(component("  padded  ", "x"), "padded");
        assert_eq!(component("line\nbreak", "x"), "line break");
    }

    #[test]
    fn escapes_the_names_dos_claimed() {
        assert_eq!(component("CON", "x"), "_CON");
        assert_eq!(component("nul", "x"), "_nul");
        assert_eq!(component("com4.txt", "x"), "_com4.txt");
        assert_eq!(component("CONCERT", "x"), "CONCERT", "only the exact name");
    }

    #[test]
    fn falls_back_when_nothing_usable_is_left() {
        assert_eq!(component("", "fallback"), "fallback");
        assert_eq!(component("   ", "fallback"), "fallback");
        assert_eq!(component("...", "fallback"), "fallback");
    }

    #[test]
    fn shortens_a_title_that_would_not_fit() {
        let long = "a".repeat(400);
        assert_eq!(component(&long, "x").chars().count(), MAX_LENGTH);
    }

    #[test]
    fn numbers_tracks_so_they_sort() {
        assert_eq!(track_file(1, "Intergalactic"), "01 - Intergalactic");
        assert_eq!(track_file(17, "Electrify"), "17 - Electrify");
        assert_eq!(
            track_file(3, ""),
            "03",
            "an unnamed track is still numbered"
        );
    }

    #[test]
    fn names_the_folder_after_the_release() {
        assert_eq!(
            release_folder("Beastie Boys", "Hello Nasty", "disc"),
            "Beastie Boys - Hello Nasty"
        );
        assert_eq!(release_folder("", "Hello Nasty", "disc"), "Hello Nasty");
        assert_eq!(release_folder("Beastie Boys", "", "disc"), "Beastie Boys");
        assert_eq!(release_folder("", "", "xUp1F2_-"), "xUp1F2_-");
    }
}
