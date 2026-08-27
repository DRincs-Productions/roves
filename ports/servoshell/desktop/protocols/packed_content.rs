/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! On-demand pack extraction, shared between `file.rs` and `game.rs` — both
//! protocol handlers serve the exact same bundled content, just under
//! different URL schemes (a raw `file:` path vs. `game:`'s virtual root; see
//! `game.rs`'s own module doc comment for why the latter exists), so the
//! logic for "extract whichever `.pack` archive a not-yet-extracted file
//! lives in, the first time it's touched" only needs to exist once. Extracted
//! out of `file.rs` when `game.rs` was added rather than duplicated — see
//! CUSTOMIZATIONS.md's "Virtual content root (game: protocol)" entry.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use roves_content_packer::manifest::Manifest;

pub struct PackedContent {
    /// Where `manifest.json` + `.pack` files live (read-only, shipped as-is).
    pub content_dir: PathBuf,
    /// Where boot files were already extracted, and where lazy ones land —
    /// the directory the initial `file:` URL's path lives in.
    pub cache_dir: PathBuf,
    pub manifest: Manifest,
    /// Serializes on-demand extraction: two near-simultaneous loads for
    /// files in the same not-yet-extracted pack must not both start
    /// unpacking it at once. Extraction is rare/one-time per pack per
    /// session, so coarse (one lock for all packs, not per-pack) is fine.
    extracting: Mutex<()>,
}

impl PackedContent {
    /// `cache_dir`: the directory a `.roves-content-source` marker (written by
    /// `roves-content-packer extract`, see CUSTOMIZATIONS.md) is looked for in —
    /// present only for a packed-content launch; absent for one opened via raw CLI
    /// args for local dev/testing, or a build with `--content-compress=none`.
    pub fn resolve(cache_dir: &Path) -> Option<Self> {
        let marker = cache_dir.join(roves_content_packer::extract::CONTENT_SOURCE_MARKER);
        let content_dir = std::fs::read_to_string(&marker).ok()?;
        let content_dir = PathBuf::from(content_dir.trim());
        let manifest = roves_content_packer::extract::load_manifest(&content_dir).ok()?;
        Some(Self {
            content_dir,
            cache_dir: cache_dir.to_path_buf(),
            manifest,
            extracting: Mutex::new(()),
        })
    }

    /// If `file_path` doesn't exist yet and falls under this managed packed
    /// content, extracts whichever pack contains it — a synchronous,
    /// blocking call, same as the plain file I/O around it; this only ever
    /// happens the first time a given pack's contents are touched, including
    /// across relaunches (see `ensure_file_available`'s own marker file).
    pub fn ensure_available(&self, file_path: &Path) {
        if file_path.exists() {
            return;
        }
        let Ok(rel) = file_path.strip_prefix(&self.cache_dir) else {
            return;
        };
        let rel_path = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        let _guard = self.extracting.lock().unwrap();
        // Re-check now that we hold the lock: another load() may have just
        // finished extracting the exact pack this path lives in.
        if file_path.exists() {
            return;
        }
        if let Err(e) = roves_content_packer::extract::ensure_file_available(
            &self.content_dir,
            &self.cache_dir,
            &self.manifest,
            &rel_path,
        ) {
            log::warn!("on-demand extraction of {rel_path:?} failed: {e}");
        }
    }
}
