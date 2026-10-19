"""Which bundle holds which AnimationClip: `clip name -> [bundle hash, …]`.

Same shape as `meshidx`, but a clip is far bigger than a name: one character's
combat clip is ~200 KB of streamed keys, so decompressing all 9619 bundles of the
game folder (66 GB measured) just to read names costs ten minutes.  Two passes:

1. **metadata only** — a SerializedFile's type table sits right after its header,
   so the class ids say whether the bundle holds an `AnimationClip` (class id 74)
   without reading a single object body.  Measured: 86 KB of metadata for a 60 MB
   node holding 1475 objects, i.e. one block per bundle.
2. fully read only the bundles that passed and take the names with `peek_name()`.

Do **not** hand UnityPy a truncated slice to do pass 1.  `check_file_type` rejects
a stream whose declared `file_size` is larger than the bytes it got, and
`SerializedFile.__init__` eagerly parses the file's AssetBundle object (whose data
lives far beyond the metadata), so a prefix either reports zero objects or raises
`EOFError: read_length out of bounds` — silently, through `parse_file`'s
`except`.  Reading the type table directly is short, exact, and has neither
failure mode.
"""

from __future__ import annotations

import json
import os
import struct
import time
from concurrent.futures import ProcessPoolExecutor

FORMAT = "exmt-animindex-1"
CLIP = "AnimationClip"
CLIP_ID = 74  # Unity ClassIDType.AnimationClip


def _stream_prefix(path: str, info, want: int) -> bytes:
    """Decompress just enough blocks of the bundle stream to cover `want` bytes."""
    from .bundle import decrypt, lz4_dec, read_stream

    if want <= 0:
        return b""
    need = info.data_off
    have = 0
    for (u, c, _f) in info.blocks:
        need += c
        have += u
        if have >= want:
            break
    with open(path, "rb") as fh:
        plain = decrypt(fh.read(need))
    if have < want:
        return read_stream(plain, info)
    buf = bytearray()
    pos = info.data_off
    for (u, c, f) in info.blocks:
        chunk = plain[pos:pos + c]
        pos += c
        data = lz4_dec(chunk, u) if (f & 0x3F) in (2, 3) else chunk
        if data is None:
            break
        buf += data
        if len(buf) >= want:
            break
    return bytes(buf)


def class_ids(meta: bytes, version: int | None = None) -> set:
    """Class ids in a SerializedFile metadata blob (header + type table).

    Only the type table is walked, so nothing beyond `metadata_size` is needed.
    Layout per Unity's `SerializedFile::ReadHeader`/`SerializedType::Read`; the
    type-tree blob is skipped by arithmetic (node count x node size + string
    buffer), see `TypeTreeNode.parse_blob`.
    """
    _metadata_size, _file_size, version = struct.unpack_from(
        ">3I", meta, 0) if version is None else (None, None, version)
    if not 9 <= version <= 21:      # v22+ moved the header fields around
        return set()
    endian = ">" if meta[16] else "<"
    pos = meta.index(b"\0", 20) + 1                 # unity version string
    if version >= 8:
        pos += 4                                    # target platform
    enable_tt = False
    if version >= 13:
        enable_tt = bool(meta[pos])
        pos += 1
    count = struct.unpack_from(endian + "i", meta, pos)[0]
    pos += 4
    node_size = 32 if version >= 19 else 24
    ids = set()
    for _ in range(count):
        class_id = struct.unpack_from(endian + "i", meta, pos)[0]
        pos += 4
        ids.add(class_id)
        if version >= 16:
            pos += 1                                # is_stripped_type
        if version >= 17:
            pos += 2                                # script_type_index
        if version >= 13:
            if (version < 16 and class_id < 0) or (version >= 16 and class_id == 114):
                pos += 16                           # script_id hash
            pos += 16                               # old_type_hash
        if enable_tt and (version >= 12 or version == 10):
            nodes, string_bytes = struct.unpack_from(endian + "2i", meta, pos)
            pos += 8 + node_size * nodes + string_bytes
        if version >= 21:
            deps = struct.unpack_from(endian + "i", meta, pos)[0]
            pos += 4 + 4 * deps                     # type_dependencies
    return ids


def metadata_class_ids(path: str) -> set:
    """Class ids of a bundle's first SerializedFile node (empty when it has none)."""
    from .bundle import decrypt, parse

    with open(path, "rb") as fh:
        info = parse(decrypt(fh.read(131072)))
    for off, size, flags, _cab in info.nodes:
        if flags != 4 or size < 64:
            continue
        head = _stream_prefix(path, info, off + 20)
        if len(head) < off + 20:
            continue
        metadata_size, _file_size, version, data_offset = struct.unpack_from(">4I", head, off)
        want = off + min(size, max(metadata_size, data_offset))
        meta = _stream_prefix(path, info, want)
        if len(meta) < want:
            continue
        try:
            ids = class_ids(meta[off:want])
        except Exception:
            continue
        if ids:
            return ids
    return set()


def scan_bundle(path: str) -> list:
    """`[(clip name, bundle hash)]` for one bundle (empty when it has no clip)."""
    import UnityPy

    clean = os.path.basename(path)
    hash_ = clean[:-7] if clean.endswith(".bundle") else os.path.splitext(clean)[0]
    try:
        if CLIP_ID not in metadata_class_ids(path):
            return []
    except Exception:
        return []
    out = []
    try:
        from .bundle import decrypt

        with open(path, "rb") as fh:
            full = UnityPy.load(decrypt(fh.read()))
        for obj in full.objects:
            if obj.type.name != CLIP:
                continue
            try:
                name = obj.peek_name()
            except Exception:
                continue
            if name:
                out.append((str(name), hash_))
    except Exception:
        pass
    return out


def build(bundle_dir: str, out_path: str, workers: int = 6, limit: int | None = None) -> dict:
    """Scan `bundle_dir`; write `{clips: {name: [hash, …]}}` to `out_path`."""
    files = sorted(
        os.path.join(bundle_dir, name)
        for name in os.listdir(bundle_dir)
        if name.endswith(".bundle")
    )
    if limit:
        files = files[:limit]
    started = time.time()
    table: dict = {}
    scanned = 0
    with ProcessPoolExecutor(max_workers=max(1, workers)) as pool:
        for rows in pool.map(scan_bundle, files, chunksize=4):
            scanned += 1
            for name, hash_ in rows:
                bucket = table.setdefault(name, [])
                if hash_ not in bucket:
                    bucket.append(hash_)
    payload = {
        "format": FORMAT,
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "bundle_dir": bundle_dir,
        "bundles": scanned,
        "clip_count": len(table),
        "anim_bundles": sorted({h for row in table.values() for h in row}),
        "clips": table,
    }
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    tmp = out_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, separators=(",", ":"))
    os.replace(tmp, out_path)
    return {
        "index": out_path,
        "bundles": scanned,
        "clips": len(table),
        "clip_bundles": len(payload["anim_bundles"]),
        "seconds": round(time.time() - started, 1),
    }


def load(path: str):
    """`{clip name: [bundle hash, …]}` or None when the file is missing/stale."""
    if not path or not os.path.isfile(path):
        return None
    try:
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
    except Exception:
        return None
    if data.get("format") != FORMAT or not data.get("clips"):
        return None
    return data["clips"]


def bundles_of(index, clip_name: str) -> list:
    """Every bundle that holds `clip_name` (empty when unknown)."""
    if not index:
        return []
    if isinstance(index, dict) and "clips" in index:
        index = index["clips"]
    return list(index.get(clip_name) or [])


def for_skin(index, skin_key: str) -> list:
    """`[{name, bundles}]` for one skin prefix, in name order.

    `skin_key` is the asset prefix the meshes and textures share (`c_AlvaSSR01`),
    so the clip prefix is that plus `_` — clips are named
    `c_<Skin>_slg_<action>` exactly like meshes.
    """
    if not index:
        return []
    if isinstance(index, dict) and "clips" in index:
        index = index["clips"]
    prefix = "%s_" % skin_key
    rows = [
        {"name": name, "bundles": list(bundles)}
        for name, bundles in index.items()
        if name.startswith(prefix)
    ]
    rows.sort(key=lambda row: row["name"])
    return rows
