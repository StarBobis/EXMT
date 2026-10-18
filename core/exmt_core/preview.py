"""Preview + workspace payloads for the Models page.

Three jobs, all read-only towards the game:

* `build_texindex` walks the bundle folder and records which bundle holds which
  Texture2D.  A character's meshes and its textures live in *different* bundles
  and nothing in the mesh bundle points at the texture bundle, so a name index
  is the only way to answer "which textures belong to this skin".  Only bundles
  whose first node is small can be texture bundles, which keeps the scan at a
  few seconds instead of decompressing every mesh bundle.
* `geometry` / `textures_for` feed the three.js preview.
* `make_workspace` writes the folder the Blender add-on imports: one
  `.exmtmesh` per mesh, one PNG per texture, plus `workspace.json`.
"""

from __future__ import annotations

import base64
import io
import json
import os
import time
from concurrent.futures import ProcessPoolExecutor

import numpy as np

# A Texture2D-only bundle has a thin SerializedFile (object table + names); mesh
# bundles are megabytes of vertex data, so this bound skips them cheaply.
MAX_SF_FOR_INDEX = 512 * 1024


# --------------------------------------------------------------------------- #
# texture index
# --------------------------------------------------------------------------- #
def _peek_texture_names(path: str) -> list:
    """Texture2D names of one bundle, without decompressing the .resS payload."""
    from .bundle import decrypt, lz4_dec, parse

    try:
        with open(path, "rb") as fh:
            head = fh.read(65536)
        plain = decrypt(head)
        info = parse(plain)
        if not info.nodes:
            return []
        off, size, flags, _cab = info.nodes[0]
        if flags != 4 or size > MAX_SF_FOR_INDEX:
            return []

        end = off + size
        need = 0
        consumed = 0
        have = 0
        for (u, c, _f) in info.blocks:
            need += 1
            consumed += c
            have += u
            if have >= end:
                break
        want = info.data_off + consumed
        if want > len(plain):
            with open(path, "rb") as fh:
                plain = decrypt(fh.read(want))

        buf = bytearray()
        pos = info.data_off
        for (u, c, f) in info.blocks[:need]:
            chunk = plain[pos:pos + c]
            pos += c
            if (f & 0x3F) in (2, 3):
                d = lz4_dec(chunk, u)
                if d is None:
                    return []
                buf += d
            else:
                buf += chunk

        import UnityPy

        env = UnityPy.load(bytes(buf[off:end]))
        names = []
        for o in env.objects:
            if o.type.name != "Texture2D":
                continue
            try:
                name = o.peek_name()
            except Exception:
                name = None
            if name:
                names.append(name)
        return names
    except Exception:
        return []


def build_texindex(bundle_dir: str, out_path: str) -> dict:
    """Write `{texture: [bundle file, …]}` plus stats; returns the summary."""
    files = sorted(f for f in os.listdir(bundle_dir) if f.endswith(".bundle"))
    workers = max(2, min(16, (os.cpu_count() or 4)))
    started = time.time()
    with ProcessPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(_peek_texture_names,
                                [os.path.join(bundle_dir, f) for f in files],
                                chunksize=64))

    textures: dict[str, list[str]] = {}
    for fname, names in zip(files, results):
        for name in names:
            textures.setdefault(name, []).append(fname)
    payload = {
        "format": "exmt-texindex-1",
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "bundle_dir": bundle_dir,
        "bundle_count": len(files),
        "texture_count": len(textures),
        "textures": textures,
    }
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    tmp = out_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, separators=(",", ":"))
    os.replace(tmp, out_path)
    return {
        "index": out_path,
        "bundles": len(files),
        "textures": len(textures),
        "seconds": round(time.time() - started, 1),
    }


def load_texindex(path: str) -> dict:
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


# --------------------------------------------------------------------------- #
# meshes + preview geometry
# --------------------------------------------------------------------------- #
def list_meshes(bundle_path: str) -> list:
    """Every mesh in one bundle: name, vertex and triangle counts."""
    from . import mesh as M
    from .sf import open_bundle

    b = open_bundle(bundle_path)
    out = []
    for o in b.meshes():
        tree = o.read_typetree()
        md = M.from_tree(tree)
        out.append({
            "name": md.name,
            "verts": int(md.vertex_count),
            "tris": int(len(md.indices) // 3),
            "bones": int(len(md.bone_hashes)),
        })
    return out


def _b64(array: np.ndarray) -> str:
    return base64.b64encode(np.ascontiguousarray(array).tobytes()).decode("ascii")


def geometry(bundle_path: str, mesh_name: str) -> dict:
    """Positions / normals / uvs / indices for the three.js preview (base64 f32)."""
    from . import mesh as M
    from .sf import open_bundle

    b = open_bundle(bundle_path)
    tree = b.mesh(mesh_name).read_typetree()
    md = M.from_tree(tree)

    pos = md.arrays.get(0)
    if pos is None:
        raise ValueError("mesh %r has no positions" % mesh_name)
    pos = pos[:, :3].astype(np.float32)
    normal = md.arrays.get(1)
    uv = md.arrays.get(4)
    indices = md.indices.astype(np.uint32)

    # Unity is left-handed: mirror Z once here so three.js (right-handed) sees
    # the model the way the game does, and flip the winding to match.
    pos = pos * np.array([1.0, 1.0, -1.0], dtype=np.float32)
    if normal is not None:
        normal = normal[:, :3].astype(np.float32) * np.array([1.0, 1.0, -1.0], dtype=np.float32)
    if len(indices) >= 3:
        idx = indices[:(len(indices) // 3) * 3].reshape(-1, 3)[:, ::-1].reshape(-1)
    else:
        idx = indices

    lo = pos.min(axis=0) if len(pos) else np.zeros(3)
    hi = pos.max(axis=0) if len(pos) else np.zeros(3)
    return {
        "name": md.name,
        "count": int(md.vertex_count),
        "positions": _b64(pos),
        "normals": _b64(normal) if normal is not None else None,
        "uvs": _b64(uv[:, :2].astype(np.float32)) if uv is not None else None,
        "indices": _b64(idx),
        "bounds": {"min": [float(v) for v in lo], "max": [float(v) for v in hi]},
    }


# --------------------------------------------------------------------------- #
# textures
# --------------------------------------------------------------------------- #
def thumbs(bundle_path: str, names: list, size: int = 192) -> list:
    """Downscaled PNGs (base64) for the texture grid, one bundle load."""
    from PIL import Image

    from . import textures as T

    bundle = None
    out = []
    for name in names:
        try:
            if bundle is None:
                from .sf import open_bundle

                bundle = open_bundle(bundle_path)
            img = T.decode_texture(bundle, name)
            width, height = img.size
            img.thumbnail((size, size), Image.LANCZOS)
            buf = io.BytesIO()
            img.save(buf, "PNG")
            out.append({
                "name": name,
                "width": int(width),
                "height": int(height),
                "png": base64.b64encode(buf.getvalue()).decode("ascii"),
            })
        except Exception as exc:  # noqa: BLE001 - one broken texture must not sink the grid
            out.append({"name": name, "error": str(exc)[:200]})
    return out


# --------------------------------------------------------------------------- #
# workspace
# --------------------------------------------------------------------------- #
def _safe(name: str) -> str:
    keep = [c if (c.isalnum() or c in "-_.") else "_" for c in name]
    return "".join(keep)[:120] or "item"


def make_workspace(outdir: str, spec: dict) -> dict:
    """Write meshes + textures + workspace.json for the Blender add-on.

    `spec` = {id, name, display, bundles: {hash: path}, meshes: [{bundle, mesh}],
              textures: [{bundle, name}]}
    """
    from . import interchange, mesh as M
    from . import textures as T
    from .sf import open_bundle

    os.makedirs(outdir, exist_ok=True)
    mesh_dir = os.path.join(outdir, "meshes")
    tex_dir = os.path.join(outdir, "textures")
    os.makedirs(mesh_dir, exist_ok=True)
    os.makedirs(tex_dir, exist_ok=True)

    bundles: dict[str, object] = {}
    written_meshes = []
    for item in spec.get("meshes", []):
        hash_ = item["bundle"]
        if hash_ not in bundles:
            bundles[hash_] = open_bundle(spec["bundles"][hash_])
        bundle = bundles[hash_]
        tree = bundle.mesh(item["mesh"]).read_typetree()
        md = M.from_tree(tree)
        fname = _safe(md.name) + ".exmtmesh"
        path = os.path.join(mesh_dir, fname)
        interchange.save(md, path, {"bundle": hash_}, tree=tree)
        written_meshes.append({
            "file": "meshes/" + fname,
            "mesh": md.name,
            "bundle": hash_,
            "verts": int(md.vertex_count),
            "bones": int(len(md.bone_hashes)),
        })

    written_textures = []
    for item in spec.get("textures", []):
        hash_ = item["bundle"]
        if hash_ not in bundles:
            bundles[hash_] = open_bundle(spec["bundles"][hash_])
        bundle = bundles[hash_]
        img = T.decode_texture(bundle, item["name"])
        if max(img.size) > 2048:
            img.thumbnail((2048, 2048))
        fname = _safe(item["name"]) + ".png"
        path = os.path.join(tex_dir, fname)
        img.save(path)
        written_textures.append({
            "file": "textures/" + fname,
            "name": item["name"],
            "bundle": hash_,
            "width": int(img.size[0]),
            "height": int(img.size[1]),
        })

    manifest = {
        "format": "exmt-workspace-1",
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "entry": {k: spec.get(k, "") for k in ("id", "name", "display")},
        "game_bundles": spec.get("game_bundles", ""),
        "meshes": written_meshes,
        "textures": written_textures,
    }
    with open(os.path.join(outdir, "workspace.json"), "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, ensure_ascii=False, indent=1)

    return {
        "dir": outdir,
        "meshes": len(written_meshes),
        "textures": len(written_textures),
        "bytes": sum(os.path.getsize(os.path.join(outdir, m["file"])) for m in written_meshes),
    }
