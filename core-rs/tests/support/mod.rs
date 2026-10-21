//! Where the real GF2 sample bundle comes from.
//!
//! Tests prefer `EXMT_GF2_BUNDLE`, then the installed game, then the old
//! development cache. The bundle is read-only; no test writes into the game.

use std::path::PathBuf;

pub fn gf2_bundle() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("EXMT_GF2_BUNDLE") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
        eprintln!("skip: EXMT_GF2_BUNDLE is not a file: {}", path.display());
        return None;
    }
    let installed = PathBuf::from(
        r"D:\Program Files\GF2Exilium\GF2 Game\GF2_Exilium_Data\LocalCache\Data\AssetBundles_Windows\872f1c4f93f07d6ba65b103ae5d1a337.bundle",
    );
    if installed.is_file() {
        return Some(installed);
    }
    let legacy = PathBuf::from(
        r"C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig",
    );
    if legacy.is_file() {
        return Some(legacy);
    }
    eprintln!("skip: GF2 sample bundle not found; set EXMT_GF2_BUNDLE");
    None
}
