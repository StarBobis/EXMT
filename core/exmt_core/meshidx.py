"""Which bundle holds which mesh name: `mesh name -> [bundle hash, …]`.

A mesh is not necessarily stored once.  GF2 keeps several copies of the same
asset (68 of 643 bundles hold meshes of more than one skin: `0e834224…` carries
Alva SSR01/0101/0102), and the game picks whichever copy its loader reaches.  So
"patch the mesh" has to mean "patch every copy", otherwise an edit can land in a
file the game never reads — and two Mods that share a bundle overwrite each
other.

Scanning is cheap because only the object *name* is needed: `UnityPy`'s
`peek_name()` reads that field alone (measured: 25 bundles in 1.1 s, 643 in
~40 s single-threaded).  The result is cached in `<cache>/meshindex.json` and
rebuilt only when the game folder changes.
"""

from __future__ import annotations

import json
import os
import time
from concurrent.futures import ProcessPoolExecutor

FORMAT = "exmt-meshindex-1"


def scan_bundle(path: str) -> list:
    """[(mesh name, bundle hash)] for one bundle."""
    import UnityPy

    from .bundle import decrypt

    clean = os.path.basename(path)
    hash_ = clean[:-7] if clean.endswith(".bundle") else os.path.splitext(clean)[0]
    try:
        with open(path, "rb") as fh:
            env = UnityPy.load(decrypt(fh.read()))
        out = []
        for obj in env.objects:
            if obj.type.name != "Mesh":
                continue
            try:
                name = obj.peek_name()
            except Exception:
                continue
            if name:
                out.append((str(name), hash_))
        return out
    except Exception:
        return []


def build(bundle_dir: str, out_path: str, limit: int = 0, workers: int = 6) -> dict:
    """Scan `bundle_dir` and write `{mesh: [bundle hash, …]}` to `out_path`."""
    files = sorted(
        os.path.join(bundle_dir, name)
        for name in os.listdir(bundle_dir)
        if name.endswith(".bundle")
    )
    if limit:
        files = files[:limit]

    started = time.time()
    table: dict = {}
    done = 0
    with ProcessPoolExecutor(max_workers=max(1, workers)) as pool:
        for rows in pool.map(scan_bundle, files, chunksize=4):
            done += 1
            for name, hash_ in rows:
                bucket = table.setdefault(name, [])
                if hash_ not in bucket:
                    bucket.append(hash_)

    payload = {
        "format": FORMAT,
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "game_bundles": bundle_dir,
        "bundles": done,
        "mesh_count": len(table),
        "meshes": table,
    }
    tmp = out_path + ".tmp"
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False)
    os.replace(tmp, out_path)
    payload["seconds"] = round(time.time() - started, 1)
    payload.pop("meshes")
    return payload


def load(path: str):
    if not os.path.isfile(path):
        return None
    try:
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        return data.get("meshes") or None
    except Exception:
        return None


def bundles_of(index, mesh_name: str) -> list:
    """Every bundle that holds `mesh_name` (empty when unknown)."""
    if not index:
        return []
    return list(index.get(mesh_name) or [])
