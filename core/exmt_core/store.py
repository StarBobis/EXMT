"""Backup and restore the game's original asset bundles.

The models page always reads from the backup, never from the live game folder,
so a previously installed mod cannot leak into the preview. Restoring copies the
backup back over the game folder.
"""

from __future__ import annotations

import os
import shutil
import time


def _copy(src: str, dst: str, hashes: list, prefer: dict | None = None) -> dict:
    os.makedirs(dst, exist_ok=True)
    copied = skipped = missing = 0
    total = 0
    for name in hashes:
        source = (prefer or {}).get(name) or os.path.join(src, name + ".bundle")
        target = os.path.join(dst, name + ".bundle")
        if not os.path.isfile(source):
            missing += 1
            continue
        if os.path.isfile(target) and os.path.getsize(target) == os.path.getsize(source):
            skipped += 1
            continue
        shutil.copy2(source, target)
        copied += 1
        total += os.path.getsize(target)
    return {"copied": copied, "skipped": skipped, "missing": missing, "bytes": total}


def backup_bundles(src_dir: str, dst_dir: str, hashes: list, prefer: dict | None = None) -> dict:
    """Copy the listed bundles from the game into our own cache folder.

    `prefer` maps a hash to a known-pristine copy (e.g. a backup made by an older
    version of the app), which wins over the live game file.
    """
    if not os.path.isdir(src_dir):
        raise SystemExit("game bundle directory not found: %s" % src_dir)
    started = time.time()
    stats = _copy(src_dir, dst_dir, hashes, prefer)
    stats.update({"dir": dst_dir, "seconds": round(time.time() - started, 2)})
    return stats


def restore_bundles(backup_dir: str, game_dir: str, hashes: list) -> dict:
    """Copy the backed-up original bundles back over the game folder."""
    if not os.path.isdir(backup_dir):
        raise SystemExit("backup directory not found: %s" % backup_dir)
    if not os.path.isdir(game_dir):
        raise SystemExit("game bundle directory not found: %s" % game_dir)
    started = time.time()
    stats = _copy(backup_dir, game_dir, hashes)
    stats.update({"dir": game_dir, "seconds": round(time.time() - started, 2)})
    return stats


def backup_size(backup_dir: str) -> dict:
    """How much we already keep in the backup folder."""
    if not os.path.isdir(backup_dir):
        return {"files": 0, "bytes": 0, "dir": backup_dir}
    files = 0
    total = 0
    for name in os.listdir(backup_dir):
        path = os.path.join(backup_dir, name)
        if os.path.isfile(path):
            files += 1
            total += os.path.getsize(path)
    return {"files": files, "bytes": total, "dir": backup_dir}
