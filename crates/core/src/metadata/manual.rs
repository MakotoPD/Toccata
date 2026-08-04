// SPDX-License-Identifier: GPL-3.0-or-later

//! Metadata the user typed or corrected, kept on disc under the Disc ID.
//!
//! This is the first source the cascade asks. Somebody who has already fixed a
//! release by hand should never have to fix it twice, and their answer beats
//! anything a database has to say about the same disc.

use std::fs;
use std::path::PathBuf;

use super::{Lookup, MetadataError, MetadataSource, ReleaseCandidate, SourceId};
use crate::toc::Toc;

pub struct Manual {
    root: PathBuf,
}

impl Manual {
    /// `root` is a directory the application owns. The caller supplies it,
    /// because working out where that is belongs to the layer that knows which
    /// system it is running on.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load(&self, disc_id: &str) -> Option<ReleaseCandidate> {
        let path = self.path_for(disc_id)?;
        let body = fs::read_to_string(path).ok()?;

        serde_json::from_str(&body).ok()
    }

    /// Writes through a temporary file so that an interrupted save leaves the
    /// previous version intact rather than half a file.
    pub fn save(&self, disc_id: &str, release: &ReleaseCandidate) -> Result<(), MetadataError> {
        let path = self.path_for(disc_id).ok_or(unwritable())?;
        fs::create_dir_all(&self.root).map_err(|_| unwritable())?;

        let mut stored = release.clone();
        stored.source_id = SourceId::Manual;
        stored.relayed_from = None;

        let body = serde_json::to_string_pretty(&stored).map_err(|_| unwritable())?;
        let temporary = path.with_extension("json.new");

        fs::write(&temporary, body).map_err(|_| unwritable())?;
        fs::rename(&temporary, &path).map_err(|_| unwritable())?;

        Ok(())
    }

    pub fn forget(&self, disc_id: &str) -> Result<(), MetadataError> {
        let Some(path) = self.path_for(disc_id) else {
            return Ok(());
        };

        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(unwritable()),
        }
    }

    /// Disc IDs are built from a fixed alphabet, but the value arrives from
    /// outside this module, so it is checked rather than trusted with a path.
    fn path_for(&self, disc_id: &str) -> Option<PathBuf> {
        if !is_safe_name(disc_id) {
            return None;
        }

        Some(self.root.join(format!("{disc_id}.json")))
    }
}

fn is_safe_name(disc_id: &str) -> bool {
    !disc_id.is_empty()
        && disc_id.len() <= 64
        && disc_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !disc_id.contains("..")
}

fn unwritable() -> MetadataError {
    MetadataError::Unreadable {
        source_id: SourceId::Manual,
    }
}

impl MetadataSource for Manual {
    fn id(&self) -> SourceId {
        SourceId::Manual
    }

    fn lookup<'a>(&'a self, toc: &'a Toc) -> Lookup<'a> {
        Box::pin(async move {
            Ok(self
                .load(&toc.musicbrainz_disc_id())
                .into_iter()
                .collect::<Vec<_>>())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::TrackMetadata;

    fn candidate() -> ReleaseCandidate {
        ReleaseCandidate {
            source_id: SourceId::MusicBrainz,
            relayed_from: Some("discogs".to_owned()),
            id: "release-1".to_owned(),
            title: "Produkt47".to_owned(),
            artist: "OKI".to_owned(),
            date: Some("2022".to_owned()),
            country: Some("PL".to_owned()),
            label: None,
            barcode: None,
            disambiguation: None,
            genre: None,
            style: None,
            composer: None,
            comment: None,
            compilation: false,
            disc_number: 1,
            disc_total: Some(1),
            medium_track_counts: vec![1],
            cover_art: None,
            tracks: vec![TrackMetadata {
                number: 1,
                title: "I To Jest Fakt".to_owned(),
                artist: "OKI".to_owned(),
                length_ms: None,
            }],
        }
    }

    fn store() -> (Manual, tempdir::Guard) {
        let guard = tempdir::create();
        (Manual::new(guard.path()), guard)
    }

    #[test]
    fn a_saved_release_comes_back_marked_as_the_users_own() {
        let (manual, _guard) = store();

        manual.save("disc-one", &candidate()).unwrap();
        let loaded = manual.load("disc-one").expect("the release was saved");

        assert_eq!(loaded.title, "Produkt47");
        assert_eq!(loaded.tracks[0].title, "I To Jest Fakt");
        assert_eq!(loaded.source_id, SourceId::Manual);
        assert_eq!(
            loaded.relayed_from, None,
            "an edited release is nobody else's any more"
        );
    }

    #[test]
    fn an_unknown_disc_has_nothing_stored() {
        let (manual, _guard) = store();
        assert!(manual.load("disc-one").is_none());
    }

    #[test]
    fn forgetting_removes_it_and_forgetting_twice_is_harmless() {
        let (manual, _guard) = store();

        manual.save("disc-one", &candidate()).unwrap();
        manual.forget("disc-one").unwrap();
        assert!(manual.load("disc-one").is_none());
        assert!(manual.forget("disc-one").is_ok());
    }

    #[test]
    fn refuses_identifiers_that_would_escape_the_directory() {
        for name in [
            "",
            "../secrets",
            "sub/dir",
            r"sub\dir",
            "with space",
            "disc..id",
        ] {
            assert!(!is_safe_name(name), "{name:?} should be refused");
        }
    }

    #[test]
    fn accepts_the_alphabet_disc_ids_are_built_from() {
        assert!(is_safe_name("xUp1F2NkfP8s8jaeFn_Av3jNEI4-"));
        assert!(is_safe_name("GUSGi.wM90Gz9VZEk2MG_4AyYk8-"));
    }

    /// A directory that cleans itself up, so the tests do not need a crate for
    /// it and do not leave anything behind.
    mod tempdir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU32, Ordering};

        static COUNTER: AtomicU32 = AtomicU32::new(0);

        pub struct Guard(PathBuf);

        impl Guard {
            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        pub fn create() -> Guard {
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("toccata-test-{}-{unique}", std::process::id()));

            std::fs::create_dir_all(&path).expect("the temporary directory can be created");
            Guard(path)
        }
    }
}
