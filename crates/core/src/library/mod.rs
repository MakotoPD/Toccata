// SPDX-License-Identifier: GPL-3.0-or-later

//! What has been ripped, kept in SQLite.
//!
//! The question this answers is "have I already done this disc, and how did it
//! go" — which a folder full of files cannot, because the folder may have been
//! moved, renamed or filled from somewhere else entirely. The disc identifier
//! is the key, since that is the one thing about a disc that does not change.
//!
//! Nothing here is required for a rip to work. A database that will not open
//! costs the history and nothing else.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::tag::Album;

/// One rip that happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub musicbrainz_disc_id: String,
    pub artist: String,
    pub title: String,
    /// Where the files went, as it was at the time.
    pub folder: String,
    pub drive: String,
    pub read_offset: i32,
    /// Seconds since the epoch, formatted by whoever shows it.
    pub ripped_at: i64,
    pub tracks: u32,
    /// Sectors the drive could not read. Above zero and the rip is not
    /// bit-perfect, however good it sounds.
    pub unreadable_sectors: u32,
}

/// One track of one rip, kept so a checksum can be looked up long afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackEntry {
    pub number: u8,
    pub title: String,
    pub crc32: u32,
    pub accuraterip_v1: u32,
    pub accuraterip_v2: u32,
    pub unreadable_sectors: u32,
}

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LibraryError {
    #[error("the library database could not be opened")]
    Unopenable,

    #[error("the library database refused the change")]
    Unwritable,

    #[error("the library database could not be read")]
    Unreadable,
}

/// The columns an [`Entry`] is read from. The track count is counted rather
/// than stored, so that forgetting a track can never leave the number lying.
const SELECT: &str = "SELECT r.id, r.disc_id, r.artist, r.title, r.folder, r.drive,
                             r.read_offset, r.ripped_at, r.unreadable_sectors,
                             COUNT(t.number)
                      FROM rips r
                      LEFT JOIN rip_tracks t ON t.rip = r.id";

const ORDER: &str = "ORDER BY r.ripped_at DESC, r.id DESC";

pub struct Library {
    connection: Connection,
}

impl Library {
    /// Opens the database, creating it and its tables if this is the first run.
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| LibraryError::Unopenable)?;
        }

        let connection = Connection::open(path).map_err(|_| LibraryError::Unopenable)?;
        Self::prepare(&connection)?;

        Ok(Self { connection })
    }

    /// A database in memory, which is what the tests run against.
    pub fn in_memory() -> Result<Self, LibraryError> {
        let connection = Connection::open_in_memory().map_err(|_| LibraryError::Unopenable)?;
        Self::prepare(&connection)?;

        Ok(Self { connection })
    }

    fn prepare(connection: &Connection) -> Result<(), LibraryError> {
        // `IF NOT EXISTS` rather than a migration table: there is one version
        // of this schema so far, and inventing a migration system for it would
        // be building for a future nobody has described yet.
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;

                 CREATE TABLE IF NOT EXISTS rips (
                     id                 INTEGER PRIMARY KEY,
                     disc_id            TEXT    NOT NULL,
                     artist             TEXT    NOT NULL,
                     title              TEXT    NOT NULL,
                     folder             TEXT    NOT NULL,
                     drive              TEXT    NOT NULL,
                     read_offset        INTEGER NOT NULL,
                     ripped_at          INTEGER NOT NULL,
                     unreadable_sectors INTEGER NOT NULL
                 );

                 CREATE INDEX IF NOT EXISTS rips_by_disc ON rips (disc_id);

                 CREATE TABLE IF NOT EXISTS rip_tracks (
                     rip                INTEGER NOT NULL REFERENCES rips (id) ON DELETE CASCADE,
                     number             INTEGER NOT NULL,
                     title              TEXT    NOT NULL,
                     crc32              INTEGER NOT NULL,
                     accuraterip_v1     INTEGER NOT NULL,
                     accuraterip_v2     INTEGER NOT NULL,
                     unreadable_sectors INTEGER NOT NULL,
                     PRIMARY KEY (rip, number)
                 );",
            )
            .map_err(|_| LibraryError::Unopenable)
    }

    /// Files one finished rip, with its tracks, and hands back its identifier.
    pub fn record(
        &mut self,
        disc_id: &str,
        album: &Album,
        folder: &str,
        drive: &str,
        read_offset: i32,
    ) -> Result<i64, LibraryError> {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64);

        // One transaction, so a rip is either in the history with all of its
        // tracks or not in it at all.
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| LibraryError::Unwritable)?;

        transaction
            .execute(
                "INSERT INTO rips
                    (disc_id, artist, title, folder, drive, read_offset, ripped_at,
                     unreadable_sectors)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    disc_id,
                    album.artist,
                    album.title,
                    folder,
                    drive,
                    read_offset,
                    at,
                    album.unreadable_sectors(),
                ],
            )
            .map_err(|_| LibraryError::Unwritable)?;

        let id = transaction.last_insert_rowid();

        for track in &album.tracks {
            transaction
                .execute(
                    "INSERT INTO rip_tracks
                        (rip, number, title, crc32, accuraterip_v1, accuraterip_v2,
                         unreadable_sectors)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        track.number,
                        track.title,
                        track.checksums.crc32,
                        track.checksums.accuraterip_v1,
                        track.checksums.accuraterip_v2,
                        track.unreadable_sectors,
                    ],
                )
                .map_err(|_| LibraryError::Unwritable)?;
        }

        transaction.commit().map_err(|_| LibraryError::Unwritable)?;

        Ok(id)
    }

    /// The most recent rips, newest first.
    pub fn history(&self, limit: u32) -> Result<Vec<Entry>, LibraryError> {
        self.entries(
            &format!("{SELECT} GROUP BY r.id {ORDER} LIMIT ?1"),
            params![limit],
        )
    }

    /// Every time this particular disc has been ripped before.
    pub fn by_disc(&self, disc_id: &str) -> Result<Vec<Entry>, LibraryError> {
        self.entries(
            &format!("{SELECT} WHERE r.disc_id = ?1 GROUP BY r.id {ORDER}"),
            params![disc_id],
        )
    }

    fn entries(
        &self,
        sql: &str,
        arguments: impl rusqlite::Params,
    ) -> Result<Vec<Entry>, LibraryError> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|_| LibraryError::Unreadable)?;

        let rows = statement
            .query_map(arguments, |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    musicbrainz_disc_id: row.get(1)?,
                    artist: row.get(2)?,
                    title: row.get(3)?,
                    folder: row.get(4)?,
                    drive: row.get(5)?,
                    read_offset: row.get(6)?,
                    ripped_at: row.get(7)?,
                    unreadable_sectors: row.get(8)?,
                    tracks: row.get(9)?,
                })
            })
            .map_err(|_| LibraryError::Unreadable)?;

        rows.collect::<Result<_, _>>()
            .map_err(|_| LibraryError::Unreadable)
    }

    /// The tracks of one rip, in order.
    pub fn tracks(&self, rip: i64) -> Result<Vec<TrackEntry>, LibraryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT number, title, crc32, accuraterip_v1, accuraterip_v2, unreadable_sectors
                 FROM rip_tracks WHERE rip = ?1 ORDER BY number",
            )
            .map_err(|_| LibraryError::Unreadable)?;

        let rows = statement
            .query_map(params![rip], |row| {
                Ok(TrackEntry {
                    number: row.get(0)?,
                    title: row.get(1)?,
                    crc32: row.get(2)?,
                    accuraterip_v1: row.get(3)?,
                    accuraterip_v2: row.get(4)?,
                    unreadable_sectors: row.get(5)?,
                })
            })
            .map_err(|_| LibraryError::Unreadable)?;

        rows.collect::<Result<_, _>>()
            .map_err(|_| LibraryError::Unreadable)
    }

    /// Removes one rip from the history. The files are left alone: this is a
    /// record of what happened, not the music itself.
    pub fn forget(&self, id: i64) -> Result<(), LibraryError> {
        self.connection
            .execute("DELETE FROM rips WHERE id = ?1", params![id])
            .map(|_| ())
            .map_err(|_| LibraryError::Unwritable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::RippedTrack;
    use crate::verify::Checksums;

    fn checksums(crc32: u32) -> Checksums {
        Checksums {
            crc32,
            ctdb_crc32: crc32,
            accuraterip_v1: crc32.wrapping_mul(2),
            accuraterip_v2: crc32.wrapping_mul(3),
        }
    }

    fn album(title: &str, unreadable: u32) -> Album {
        Album {
            title: title.to_owned(),
            artist: "Oki".to_owned(),
            track_total: 7,
            tracks: vec![
                RippedTrack {
                    number: 1,
                    file: "01.flac".to_owned(),
                    title: "Znasz Mnie?".to_owned(),
                    artist: "Oki".to_owned(),
                    length: 13173,
                    pre_emphasis: false,
                    unreadable_sectors: 0,
                    checksums: checksums(0x1a8e_cbaf),
                },
                RippedTrack {
                    number: 2,
                    file: "02.flac".to_owned(),
                    title: "Goat/Simp".to_owned(),
                    artist: "Oki".to_owned(),
                    length: 11344,
                    pre_emphasis: false,
                    unreadable_sectors: unreadable,
                    checksums: checksums(0x0497_0de2),
                },
            ],
            ..Album::default()
        }
    }

    #[test]
    fn a_rip_comes_back_with_its_tracks() {
        let mut library = Library::in_memory().expect("the database opens");

        let id = library
            .record("disc-one", &album("Reklamacja'47", 0), "D:/Music", "E:", 6)
            .expect("the rip is recorded");

        let history = library.history(10).expect("the history reads");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].title, "Reklamacja'47");
        assert_eq!(
            history[0].tracks, 2,
            "the track count is counted, not stored"
        );
        assert_eq!(history[0].read_offset, 6);

        let tracks = library.tracks(id).expect("the tracks read");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].title, "Znasz Mnie?");
        assert_eq!(tracks[0].crc32, 0x1a8e_cbaf);
        assert_eq!(tracks[1].accuraterip_v2, 0x0497_0de2u32.wrapping_mul(3));
    }

    // The question the history exists to answer.
    #[test]
    fn the_same_disc_ripped_twice_shows_up_twice() {
        let mut library = Library::in_memory().unwrap();

        library
            .record("disc-one", &album("First", 0), "a", "E:", 6)
            .unwrap();
        library
            .record("disc-two", &album("Other", 0), "b", "E:", 6)
            .unwrap();
        library
            .record("disc-one", &album("Again", 0), "c", "E:", 0)
            .unwrap();

        assert_eq!(library.by_disc("disc-one").unwrap().len(), 2);
        assert_eq!(library.by_disc("disc-two").unwrap().len(), 1);
        assert!(library.by_disc("never seen").unwrap().is_empty());
    }

    #[test]
    fn an_imperfect_rip_says_so_from_the_history_alone() {
        let mut library = Library::in_memory().unwrap();
        library
            .record("disc", &album("Scratched", 1234), "a", "E:", 6)
            .unwrap();

        let entry = &library.history(10).unwrap()[0];
        assert_eq!(entry.unreadable_sectors, 1234);
    }

    // Forgetting a rip must take its tracks with it, or the database fills up
    // with rows nothing points at.
    #[test]
    fn forgetting_a_rip_takes_its_tracks_too() {
        let mut library = Library::in_memory().unwrap();
        let id = library
            .record("disc", &album("Gone", 0), "a", "E:", 6)
            .unwrap();

        library.forget(id).expect("the rip is forgotten");

        assert!(library.history(10).unwrap().is_empty());
        assert!(library.tracks(id).unwrap().is_empty());
    }

    #[test]
    fn a_database_on_disk_survives_being_closed() {
        let path = std::env::temp_dir().join(format!("toccata-library-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let mut library = Library::open(&path).expect("the database opens");
            library
                .record("disc", &album("Kept", 0), "a", "E:", 6)
                .unwrap();
        }

        let library = Library::open(&path).expect("the database reopens");
        assert_eq!(library.history(10).unwrap().len(), 1);

        let _ = std::fs::remove_file(&path);
    }
}
