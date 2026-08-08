//! Library half of `roves-content-packer` — see `src/main.rs` for the CLI
//! (`pack`/`extract`) and CUSTOMIZATIONS.md for the full design. Split out so
//! `ports/servoshell` can link this directly (for [`extract::ensure_file_available`],
//! the in-process, on-demand path used by its own `file:` protocol handler)
//! instead of shelling out to the binary for that part.

pub mod extract;
pub mod manifest;
pub mod pack;
pub mod size;
