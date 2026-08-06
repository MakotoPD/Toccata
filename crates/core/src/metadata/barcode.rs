// SPDX-License-Identifier: GPL-3.0-or-later

//! The disc's own catalogue number, looked up on Discogs.
//!
//! This is the underrated path. A pressing carries its barcode in the
//! subchannel, and Discogs catalogues physical pressings better than anything
//! else, so between them they reach discs that a table of contents lookup
//! walks straight past — local editions, reissues, anything a Disc ID has
//! never been submitted for.
//!
//! Most discs carry no catalogue number at all, in which case this source has
//! nothing to say and the cascade moves on.

use super::{Disc, Lookup, MetadataSource, SourceId, discogs::Discogs};

#[derive(Default)]
pub struct Barcode {
    discogs: Discogs,
}

impl Barcode {
    /// Shares the key the rest of the application uses for Discogs.
    pub fn set_token(&self, token: Option<&str>) {
        self.discogs.set_token(token);
    }
}

impl MetadataSource for Barcode {
    fn id(&self) -> SourceId {
        SourceId::Discogs
    }

    fn lookup<'a>(&'a self, disc: &'a Disc) -> Lookup<'a> {
        Box::pin(async move {
            let Some(barcode) = disc.mcn.as_deref() else {
                return Ok(Vec::new());
            };

            // A search hit carries no track list, so the pressing is fetched
            // in full. Anything else would put an album on screen with no
            // songs in it.
            let hits = self.discogs.search("", "", barcode).await?;

            let Some(first) = hits.first() else {
                return Ok(Vec::new());
            };

            match self.discogs.release(&first.id).await? {
                Some(release) => Ok(vec![release]),
                None => Ok(hits),
            }
        })
    }
}
