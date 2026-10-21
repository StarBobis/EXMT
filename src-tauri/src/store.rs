//! Backup and restore the game's original asset bundles.
//!
//! The models page always reads from the backup, never from the live game
//! folder, so a previously installed mod cannot leak into the preview.
//! Restoring copies the backup back over the game folder.
//!
//! This is the Rust replacement for `core/exmt_core/store.py`; the JSON shapes
//! and the skip-by-size rule match, because the frontend stores them in
//! `done.json` and shows the counters.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct HashList {
    hashes: Vec<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Stats {
    pub copied: usize,
    pub skipped: usize,
    pub missing: usize,
    pub bytes: u64,
    pub dir: String,
    pub seconds: f64,
}

/// Run one of `backup`, `restore`, `backup-size`.
pub fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("backup") => {
            let src = args.get(1).ok_or("backup: missing source directory")?;
            let dst = args.get(2).ok_or("backup: missing destination directory")?;
            let list = args.get(3).ok_or("backup: missing hash list")?;
            let prefer = match args.get(4) {
                Some(path) => Some(read_prefer(Path::new(path))?),
                None => None,
            };
            let stats = backup_bundles(
                Path::new(src),
                Path::new(dst),
                &read_hashes(Path::new(list))?,
                prefer.as_ref(),
            )?;
            serde_json::to_string(&stats).map_err(|e| e.to_string())
        }
        Some("restore") => {
            let backup = args.get(1).ok_or("restore: missing backup directory")?;
            let game = args.get(2).ok_or("restore: missing game directory")?;
            let list = args.get(3).ok_or("restore: missing hash list")?;
            let stats = restore_bundles(
                Path::new(backup),
                Path::new(game),
                &read_hashes(Path::new(list))?,
            )?;
            serde_json::to_string(&stats).map_err(|e| e.to_string())
        }
        Some("backup-size") => {
            let dir = args.get(1).ok_or("backup-size: missing directory")?;
            serde_json::to_string(&backup_size(Path::new(dir))).map_err(|e| e.to_string())
        }
        other => Err(format!("unknown store command {other:?}")),
    }
}

pub fn backup_bundles(
    src: &Path,
    dst: &Path,
    hashes: &[String],
    prefer: Option<&HashMap<String, String>>,
) -> Result<Stats, String> {
    if !src.is_dir() {
        return Err(format!(
            "game bundle directory not found: {}",
            src.display()
        ));
    }
    let started = Instant::now();
    let mut stats = copy_bundles(src, dst, hashes, prefer)?;
    stats.dir = dst.display().to_string();
    stats.seconds = round2(started.elapsed().as_secs_f64());
    Ok(stats)
}

pub fn restore_bundles(backup: &Path, game: &Path, hashes: &[String]) -> Result<Stats, String> {
    if !backup.is_dir() {
        return Err(format!("backup directory not found: {}", backup.display()));
    }
    if !game.is_dir() {
        return Err(format!(
            "game bundle directory not found: {}",
            game.display()
        ));
    }
    let started = Instant::now();
    let mut stats = copy_bundles(backup, game, hashes, None)?;
    stats.dir = game.display().to_string();
    stats.seconds = round2(started.elapsed().as_secs_f64());
    Ok(stats)
}

pub fn backup_size(dir: &Path) -> serde_json::Value {
    let mut files = 0usize;
    let mut bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    files += 1;
                    bytes += meta.len();
                }
            }
        }
    }
    serde_json::json!({ "files": files, "bytes": bytes, "dir": dir.display().to_string() })
}

fn copy_bundles(
    src: &Path,
    dst: &Path,
    hashes: &[String],
    prefer: Option<&HashMap<String, String>>,
) -> Result<Stats, String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("create {}: {e}", dst.display()))?;
    let mut stats = Stats {
        copied: 0,
        skipped: 0,
        missing: 0,
        bytes: 0,
        dir: String::new(),
        seconds: 0.0,
    };
    for name in hashes {
        let source = prefer
            .and_then(|map| map.get(name))
            .map(PathBuf::from)
            .unwrap_or_else(|| src.join(format!("{name}.bundle")));
        let target = dst.join(format!("{name}.bundle"));
        if !source.is_file() {
            stats.missing += 1;
            continue;
        }
        if let (Ok(a), Ok(b)) = (std::fs::metadata(&target), std::fs::metadata(&source)) {
            if a.len() == b.len() {
                stats.skipped += 1;
                continue;
            }
        }
        std::fs::copy(&source, &target)
            .map_err(|e| format!("copy {} -> {}: {e}", source.display(), target.display()))?;
        stats.copied += 1;
        stats.bytes += std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    }
    Ok(stats)
}

fn read_hashes(path: &Path) -> Result<Vec<String>, String> {
    let text = read_json_text(path)?;
    let list: HashList =
        serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(list.hashes)
}

fn read_prefer(path: &Path) -> Result<HashMap<String, String>, String> {
    let text = read_json_text(path)?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// The frontend writes UTF-8 without BOM, but older files and hand-edited lists
/// may carry one; Python opened these with `utf-8-sig`.
fn read_json_text(path: &Path) -> Result<String, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(text
        .strip_prefix('\u{feff}')
        .map(str::to_string)
        .unwrap_or(text))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exmt-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn copies_skips_and_counts_missing() {
        let root = temp("copy");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.bundle"), b"aaaa").unwrap();
        std::fs::write(src.join("b.bundle"), b"bbbb").unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("b.bundle"), b"bbbb").unwrap(); // same size -> skipped

        let stats =
            backup_bundles(&src, &dst, &["a".into(), "b".into(), "c".into()], None).unwrap();
        assert_eq!(stats.copied, 1);
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.missing, 1);
        assert_eq!(stats.bytes, 4);
        assert_eq!(std::fs::read(dst.join("a.bundle")).unwrap(), b"aaaa");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prefer_wins_over_the_game_copy() {
        let root = temp("prefer");
        let src = root.join("src");
        let pristine = root.join("pristine.bundle");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.bundle"), b"modded").unwrap();
        std::fs::write(&pristine, b"original").unwrap();
        let mut prefer = HashMap::new();
        prefer.insert("a".to_string(), pristine.display().to_string());

        backup_bundles(&src, &dst, &["a".into()], Some(&prefer)).unwrap();
        assert_eq!(std::fs::read(dst.join("a.bundle")).unwrap(), b"original");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn restore_copies_back_and_rejects_missing_dirs() {
        let root = temp("restore");
        let backup = root.join("backup");
        let game = root.join("game");
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(backup.join("a.bundle"), b"orig").unwrap();
        let stats = restore_bundles(&backup, &game, &["a".into()]).unwrap();
        assert_eq!(stats.copied, 1);
        assert_eq!(std::fs::read(game.join("a.bundle")).unwrap(), b"orig");
        assert!(restore_bundles(&root.join("nope"), &game, &[]).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn backup_size_counts_every_file() {
        let root = temp("size");
        std::fs::write(root.join("a"), b"123").unwrap();
        std::fs::write(root.join("b"), b"45").unwrap();
        let value = backup_size(&root);
        assert_eq!(value["files"], 2);
        assert_eq!(value["bytes"], 5);
        let _ = std::fs::remove_dir_all(root);
    }
}
