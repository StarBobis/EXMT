"""Which bundle holds which mesh / texture name: `name -> [bundle hash, …]`.

A mesh is not necessarily stored once.  GF2 keeps several copies of the same
asset (68 of 643 bundles hold meshes of more than one skin: `0e834224…` carries
Alva SSR01/0101/0102), and the game picks whichever copy its loader reaches.  So
"patch the mesh" has to mean "patch every copy", otherwise an edit can land in a
file the game never reads — and two Mods that share a bundle overwrite each
other.

Textures are duplicated too, and *more* aggressively: measured over the 643
backed-up bundles, **72 of 3824 texture names appear in more than one bundle**
(`RampMap_Linear_RGBAHalf` in 542, `Handmade_base_d` in 39, `cw_ColphneSR01_WL_d`
in 5).  Replacing such a texture in one bundle only would leave the copy the game
actually samples untouched, so the same one-pass scan records both maps.

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

FORMAT = "exmt-meshindex-2"

#: object types the index covers, in the order they are collected
KINDS = (("Mesh", "meshes"), ("Texture2D", "textures"))


def scan_bundle(path: str) -> dict:
    """`{"meshes": [(name, hash), …], "textures": [(name, hash), …]}` for one bundle."""
    import UnityPy

    from .bundle import decrypt

    clean = os.path.basename(path)
    hash_ = clean[:-7] if clean.endswith(".bundle") else os.path.splitext(clean)[0]
    out = {kind: [] for _type, kind in KINDS}
    try:
        with open(path, "rb") as fh:
            env = UnityPy.load(decrypt(fh.read()))
        wanted = {type_name: kind for type_name, kind in KINDS}
        for obj in env.objects:
            kind = wanted.get(obj.type.name)
            if kind is None:
                continue
            try:
                name = obj.peek_name()
            except Exception:
                continue
            if name:
                out[kind].append((str(name), hash_))
    except Exception:
        pass
    return out


def build(bundle_dir: str, out_path: str, limit: int = 0, workers: int = 6) -> dict:
    """Scan `bundle_dir`; write `{meshes: {...}, textures: {...}}` to `out_path`."""
    files = sorted(
        os.path.join(bundle_dir, name)
        for name in os.listdir(bundle_dir)
        if name.endswith(".bundle")
    )
    if limit:
        files = files[:limit]

    started = time.time()
    tables = {kind: {} for _type, kind in KINDS}
    done = 0
    with ProcessPoolExecutor(max_workers=max(1, workers)) as pool:
        for rows in pool.map(scan_bundle, files, chunksize=4):
            done += 1
            for _type, kind in KINDS:
                table = tables[kind]
                for name, hash_ in rows.get(kind, []):
                    bucket = table.setdefault(name, [])
                    if hash_ not in bucket:
                        bucket.append(hash_)

    payload = {
        "format": FORMAT,
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "game_bundles": bundle_dir,
        "bundles": done,
        "mesh_count": len(tables["meshes"]),
        "texture_count": len(tables["textures"]),
        "meshes": tables["meshes"],
        "textures": tables["textures"],
    }
    tmp = out_path + ".tmp"
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False)
    os.replace(tmp, out_path)
    payload["seconds"] = round(time.time() - started, 1)
    payload.pop("meshes")
    payload.pop("textures")
    return payload


def load(path: str):
    """`{"meshes": {...}, "textures": {...}}` or None (older files: meshes only)."""
    if not os.path.isfile(path):
        return None
    try:
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
    except Exception:
        return None
    if not data.get("meshes"):
        return None
    return {"meshes": data.get("meshes") or {}, "textures": data.get("textures") or {}}


def bundles_of(index, mesh_name: str) -> list:
    """Every bundle that holds `mesh_name` (empty when unknown)."""
    if not index:
        return []
    if isinstance(index, dict) and "meshes" in index:
        index = index["meshes"]
    return list(index.get(mesh_name) or [])
