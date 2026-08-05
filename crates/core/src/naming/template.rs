// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning a naming pattern into a path.
//!
//! A pattern is plain text with `{token}` placeholders and `/` where a folder
//! ends. Every component is sanitised on its own, so a title containing a
//! slash becomes part of the file name rather than a new directory. Components
//! that end up empty are dropped: a release with no album artist should not
//! produce a nameless folder.

use std::collections::HashMap;

use super::component;

/// The default, which is what most libraries already look like.
pub const DEFAULT: &str = "{albumartist}/{album}/{track} - {title}";

/// Every placeholder a pattern may use, for the interface to offer.
pub const TOKENS: [&str; 12] = [
    "albumartist",
    "album",
    "artist",
    "title",
    "track",
    "tracktotal",
    "disc",
    "disctotal",
    "year",
    "genre",
    "label",
    "catalog",
];

/// What ends a path component in a pattern. Both are accepted so a pattern
/// copied from another ripper works as written.
const SEPARATORS: [char; 2] = ['/', '\\'];

/// Values a pattern is filled in from. Anything missing renders as empty,
/// which is what lets a component disappear rather than say "unknown".
pub type Values = HashMap<String, String>;

/// Renders a pattern into path components, ready to be joined by the caller.
///
/// `fallback` stands in when the whole pattern renders to nothing, so a rip
/// always has somewhere to go.
pub fn render(pattern: &str, values: &Values, fallback: &str) -> Vec<String> {
    let filled = substitute(pattern, values);

    let components: Vec<String> = filled
        .split(['/', '\\'])
        .map(|part| component(part, ""))
        .filter(|part| !part.is_empty())
        .collect();

    if components.is_empty() {
        vec![component(fallback, "disc")]
    } else {
        components
    }
}

/// Replaces `{token}` with its value, leaving unknown tokens as themselves so
/// a typo is visible rather than silently swallowed.
fn substitute(pattern: &str, values: &Values) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;

    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);

        let Some(end) = rest[start..].find('}') else {
            // An unclosed brace is just text.
            out.push_str(&rest[start..]);
            return out;
        };

        let name = &rest[start + 1..start + end];
        match values.get(name) {
            // Separators inside a value are neutralised here rather than after
            // the split, or a title like "AC/DC" would invent a directory.
            Some(value) => out.push_str(&value.replace(SEPARATORS, "_")),
            None if TOKENS.contains(&name) => {}
            None => out.push_str(&rest[start..=start + end]),
        }

        rest = &rest[start + end + 1..];
    }

    out.push_str(rest);
    out
}

/// Numbers are padded so that a directory listing sorts the way the disc plays.
pub fn pad(number: u32, total: Option<u32>) -> String {
    let width = total.map_or(2, |total| total.to_string().len().max(2));
    format!("{number:0width$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Values {
        [
            ("albumartist", "Beastie Boys"),
            ("album", "Hello Nasty"),
            ("artist", "Beastie Boys"),
            ("title", "Intergalactic"),
            ("track", "07"),
            ("tracktotal", "22"),
            ("disc", "1"),
            ("disctotal", "2"),
            ("year", "1998"),
            ("genre", "Hip Hop"),
            ("label", "Grand Royal"),
            ("catalog", "495 7232"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
    }

    #[test]
    fn renders_the_default_into_folders_and_a_file() {
        assert_eq!(
            render(DEFAULT, &values(), "disc"),
            ["Beastie Boys", "Hello Nasty", "07 - Intergalactic"]
        );
    }

    #[test]
    fn accepts_either_separator() {
        assert_eq!(
            render(r"{album}\{title}", &values(), "disc"),
            ["Hello Nasty", "Intergalactic"]
        );
    }

    // A title with a slash in it must not turn into a directory.
    #[test]
    fn a_value_never_becomes_a_folder() {
        let mut values = values();
        values.insert("title".to_owned(), "AC/DC Tribute".to_owned());

        assert_eq!(
            render("{album}/{title}", &values, "disc"),
            ["Hello Nasty", "AC_DC Tribute"]
        );
    }

    #[test]
    fn a_component_that_renders_empty_disappears() {
        let mut values = values();
        values.insert("albumartist".to_owned(), String::new());

        assert_eq!(
            render(DEFAULT, &values, "disc"),
            ["Hello Nasty", "07 - Intergalactic"],
            "no nameless folder is left behind"
        );
    }

    #[test]
    fn a_pattern_that_renders_to_nothing_falls_back() {
        let empty = Values::new();
        assert_eq!(render("{album}/{title}", &empty, "xUp1F2-"), ["xUp1F2-"]);
    }

    #[test]
    fn a_token_nobody_knows_stays_visible() {
        assert_eq!(
            render("{album}/{nonsense}", &values(), "disc"),
            ["Hello Nasty", "{nonsense}"],
            "a typo should be obvious, not silently dropped"
        );
    }

    #[test]
    fn an_unclosed_brace_is_only_text() {
        assert_eq!(
            render("{album}/{title", &values(), "disc"),
            ["Hello Nasty", "{title"]
        );
    }

    #[test]
    fn literal_text_around_tokens_survives() {
        assert_eq!(
            render(
                "{year} - {album}/CD {disc}/{track}. {title}",
                &values(),
                "disc"
            ),
            ["1998 - Hello Nasty", "CD 1", "07. Intergalactic"]
        );
    }

    #[test]
    fn track_numbers_are_padded_to_sort() {
        assert_eq!(pad(7, Some(22)), "07");
        assert_eq!(pad(7, Some(9)), "07", "two digits even on a short disc");
        assert_eq!(pad(7, Some(140)), "007");
        assert_eq!(pad(12, None), "12");
    }
}
