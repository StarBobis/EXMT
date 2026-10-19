"""Recover real bone names: `crc32(full transform path)` -> path.

GF2 meshes only ship CRC32 hashes for their bones, but the packs do contain the
real hierarchies:

* `Avatar` assets (`c_<Char>_slg_skinAvatar`) carry `m_TOS`: a list of
  `(hash, "root/Root_M/Spine1_M/…")` pairs — hash and path agree, verified 30/30;
* character and weapon prefabs carry the `Transform` tree, whose paths are the
  same strings once you strip the ancestors the prefab adds.

Both are scanned once into `<cache>/bonenames.json`, which the workspace export
uses to name (and parent) the merged skeleton.  Everything here is read-only and
best-effort: a character whose prefab is not in the local cache keeps hash names.
"""

from __future__ import annotations

import io
import json
import os
import time
import zlib
from concurrent.futures import ProcessPoolExecutor

FORMAT = "exmt-bonenames-1"

#: the serialized-file metadata (type list + object table) fits in this prefix,
#: which is all pass 1 needs — the bulk of a bundle is never decompressed
PREFIX = 768 * 1024


def _crc(text: str) -> int:
    return zlib.crc32(text.encode()) & 0xFFFFFFFF


def _truncated_env(path: str):
    """UnityPy env built from the first `PREFIX` bytes of the bundle's node 0."""
    import UnityPy

    from .bundle import decrypt, lz4_dec, parse

    with open(path, "rb") as fh:
        head = fh.read(131072)
    info = parse(decrypt(head))
    if not info.nodes:
        return None
    off, size, flags, _cab = info.nodes[0]
    if flags != 4:
        return None
    want = min(size, PREFIX)
    need = info.data_off
    have = 0
    for (u, c, _f) in info.blocks:
        need += c
        have += u
        if have >= want:
            break
    with open(path, "rb") as fh:
        plain = decrypt(fh.read(need))
    buf = bytearray()
    pos = info.data_off
    for (u, c, f) in info.blocks:
        chunk = plain[pos:pos + c]
        pos += c
        if (f & 0x3F) in (2, 3):
            data = lz4_dec(chunk, u)
            if data is None:
                break
            buf += data
        else:
            buf += chunk
        if len(buf) >= off + want:
            break
    end = off + min(size, len(buf) - off)
    if end <= off:
        return None
    return UnityPy.load(io.BytesIO(bytes(buf[off:end])))


def _add_path(out: dict, path: str, source: str, priority: int) -> None:
    """Register a path and every ancestor-relative suffix of it."""
    parts = path.split("/")
    for start in range(len(parts)):
        suffix = "/".join(parts[start:])
        if not suffix:
            continue
        h = _crc(suffix)
        prev = out.get(h)
        if prev is None or priority > prev[1]:
            out[h] = (suffix, priority, source)


def scan_bundle(path: str) -> list:
    """`[(hash, path, source)]` harvested from one bundle (empty when irrelevant)."""
    out: dict = {}
    try:
        env = _truncated_env(path)
        if env is None:
            return []
        objects = list(env.objects)
        types = {o.type.name for o in objects}
        source = os.path.basename(path)
        if "Avatar" in types:
            for o in objects:
                if o.type.name != "Avatar":
                    continue
                try:
                    tree = o.read_typetree()
                except Exception:
                    continue
                for entry in tree.get("m_TOS") or []:
                    try:
                        h = int(entry[0])
                        text = str(entry[1])
                    except Exception:
                        continue
                    if text:
                        out[h] = (text, 2, source)
        if "Transform" in types and "GameObject" in types:
            names = {}
            father = {}
            owner = {}
            for o in objects:
                kind = o.type.name
                if kind == "GameObject":
                    try:
                        names[o.path_id] = o.read_typetree().get("m_Name") or ""
                    except Exception:
                        pass
                elif kind == "Transform":
                    try:
                        t = o.read_typetree()
                    except Exception:
                        continue
                    father[o.path_id] = t.get("m_Father", {}).get("m_PathID", 0)
                    owner[o.path_id] = t.get("m_GameObject", {}).get("m_PathID", 0)
            for tid in father:
                parts = []
                cur = tid
                guard = 0
                while cur and cur in father and guard < 64:
                    parts.append(names.get(owner.get(cur), ""))
                    cur = father[cur]
                    guard += 1
                if not parts:
                    continue
                full = "/".join(reversed([p for p in parts if p]))
                if full:
                    _add_path(out, full, source, 1)
    except Exception:
        return []
    return [(h, v[0], v[1], v[2]) for h, v in out.items()]


def build(bundle_dir: str, out_path: str, limit: int | None = None,
          workers: int | None = None) -> dict:
    """Scan `bundle_dir`, write `{hash: path}` to `out_path`, return the summary."""
    files = sorted(f for f in os.listdir(bundle_dir) if f.endswith(".bundle"))
    if limit:
        files = files[:limit]
    pool_size = workers or max(2, min(14, os.cpu_count() or 4))
    started = time.time()
    paths: dict = {}
    conflicts = []
    with ProcessPoolExecutor(max_workers=pool_size) as pool:
        for rows in pool.map(scan_bundle,
                             [os.path.join(bundle_dir, f) for f in files],
                             chunksize=8):
            for h, text, priority, source in rows:
                prev = paths.get(h)
                if prev is None:
                    paths[h] = {"path": text, "priority": priority, "source": source}
                elif prev["path"] != text and priority > prev["priority"]:
                    paths[h] = {"path": text, "priority": priority, "source": source}
                elif prev["path"] != text:
                    conflicts.append({"hash": h, "kept": prev["path"], "other": text})

    payload = {
        "format": FORMAT,
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "bundle_dir": bundle_dir,
        "bundles": len(files),
        "names": len(paths),
        "paths": {str(h): v["path"] for h, v in paths.items()},
    }
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    tmp = out_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, separators=(",", ":"))
    os.replace(tmp, out_path)
    return {
        "index": out_path,
        "bundles": len(files),
        "names": len(paths),
        "conflicts": len(conflicts),
        "seconds": round(time.time() - started, 1),
    }


def load(path: str) -> dict:
    """`{int hash: path}` for `skeleton.merge(..., paths=…)`."""
    if not path or not os.path.isfile(path):
        return {}
    with open(path, encoding="utf-8") as fh:
        payload = json.load(fh)
    return {int(k): v for k, v in (payload.get("paths") or {}).items()}
