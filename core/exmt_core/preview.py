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
import re
import shutil
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


def _geometry_payload(md) -> dict:
    """Typed arrays for three.js, base64 float32/uint32."""
    pos = md.arrays.get(0)
    if pos is None:
        raise ValueError("mesh %r has no positions" % md.name)
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


def geometry(bundle_path: str, mesh_name: str) -> dict:
    """Positions / normals / uvs / indices for the three.js preview (base64 f32)."""
    from . import mesh as M
    from .sf import open_bundle

    md = M.from_tree(open_bundle(bundle_path).mesh(mesh_name).read_typetree())
    return _geometry_payload(md)


def geoset(spec: dict) -> dict:
    """Every mesh of one skin in a single call, each bundle parsed once.

    `spec` = {bundles: {hash: path}, meshes: [{bundle, mesh}]}
    """
    from . import mesh as M
    from .sf import open_bundle

    opened: dict[str, object] = {}
    out = []
    for item in spec.get("meshes", []):
        hash_ = item["bundle"]
        if hash_ not in opened:
            opened[hash_] = open_bundle(spec["bundles"][hash_])
        tree = opened[hash_].mesh(item["mesh"]).read_typetree()
        payload = _geometry_payload(M.from_tree(tree))
        payload["bundle"] = hash_
        out.append(payload)
    return {"meshes": out}


# --------------------------------------------------------------------------- #
# textures
# --------------------------------------------------------------------------- #
def diffuse_maps(bundles: dict, pairs: list, size: int = 1024) -> dict:
    """Full-resolution diffuse maps for the meshes that have one.

    `pairs` is `[{"mesh": …, "texture": …, "bundle": …}]`; every bundle is parsed
    once. Images larger than `size` are downscaled so the preview stays light.
    """
    from PIL import Image

    from . import textures as T
    from .sf import open_bundle

    opened = {}
    out = []
    for pair in pairs:
        mesh = pair.get("mesh", "")
        name = pair.get("texture", "")
        bundle_hash = pair.get("bundle", "")
        try:
            bundle = opened.get(bundle_hash)
            if bundle is None:
                bundle = open_bundle(bundles[bundle_hash])
                opened[bundle_hash] = bundle
            img = T.decode_texture(bundle, name)
            width, height = img.size
            if max(width, height) > size:
                img = img.copy()
                img.thumbnail((size, size), Image.LANCZOS)
            buf = io.BytesIO()
            img.save(buf, "PNG")
            out.append({
                "mesh": mesh,
                "texture": name,
                "width": int(width),
                "height": int(height),
                "png": base64.b64encode(buf.getvalue()).decode("ascii"),
            })
        except Exception as exc:  # noqa: BLE001 - one broken map must not sink the rest
            out.append({"mesh": mesh, "texture": name, "error": str(exc)[:200]})
    return {"maps": out}


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


def _alias(mesh_name: str) -> str:
    """Short, stable label for a submesh: `c_AlvaSSR0102_slg_cloth1_lod0` → `cloth1_lod0`.

    The alias is what a blueprint slot shows next to its index; the full game name
    stays in `mesh` (and in the tooltip), so nothing is lost.
    """
    name = mesh_name
    m = re.match(r"^c_[A-Za-z0-9]+_(?:slg_)?(.+)$", name)
    if m:
        name = m.group(1)
    return name or mesh_name


def make_workspace(outdir: str, spec: dict) -> dict:
    """Write meshes + textures + skeleton + workspace.json for the Blender add-on.

    `spec` = {id, name, display, bundles: {hash: path}, meshes: [{bundle, mesh}],
              textures: [{bundle, name}], bone_names: <bonenames.json path>}

    每个网格自带的骨骼只是它自己用的那几根（Alva 12 个网格分别 3…70 根，并集
    226 根），所以这里先把所有网格的骨骼按 CRC32 哈希合成一副骨架写进
    `skeleton.json`，再让每个 `.exmtmesh` 带上"局部索引 → 骨架索引"的映射。
    Blender 端因此只建一副完整骨架，并且能把每个网格正确绑上去。
    `bone_names` 指向骨骼名称索引（Avatar/prefab 的 crc32(路径) → 路径），有它
    就能给出真名字与父子关系。
    """
    from . import interchange, mesh as M, skeleton as S
    from . import textures as T
    from .sf import open_bundle

    os.makedirs(outdir, exist_ok=True)
    mesh_dir = os.path.join(outdir, "meshes")
    tex_dir = os.path.join(outdir, "textures")
    anim_dir = os.path.join(outdir, "animations")
    # 工作空间只放这一次勾选的内容：先清掉旧文件，避免上一次的残留混进来
    for stale in (mesh_dir, tex_dir, anim_dir):
        if os.path.isdir(stale):
            shutil.rmtree(stale)
    os.makedirs(mesh_dir, exist_ok=True)
    os.makedirs(tex_dir, exist_ok=True)

    bundles: dict[str, object] = {}
    loaded = []
    for item in spec.get("meshes", []):
        hash_ = item["bundle"]
        if hash_ not in bundles:
            bundles[hash_] = open_bundle(spec["bundles"][hash_])
        bundle = bundles[hash_]
        tree = bundle.mesh(item["mesh"]).read_typetree()
        loaded.append((hash_, tree, M.from_tree(tree)))

    # one skeleton for the whole skin: union of every mesh's bone table
    paths = spec.get("bone_paths") or None
    if not paths and spec.get("bone_names"):
        from . import bonenames

        paths = bonenames.load(spec["bone_names"]) or None
    skel = S.merge([(md.name, md.bone_hashes, md.bindpose) for _h, _t, md in loaded],
                   paths=paths)
    skel["id"] = spec.get("id", "") or spec.get("name", "")

    written_meshes = []
    for index, (hash_, tree, md) in enumerate(loaded):
        fname = _safe(md.name) + ".exmtmesh"
        path = os.path.join(mesh_dir, fname)
        view = S.for_file(skel, md.name)
        interchange.save(md, path, {"bundle": hash_, "skeleton": view}, tree=tree)
        written_meshes.append({
            # the submesh index is assigned **here**, once, in the order the app
            # sent the meshes: it is what a blueprint slot refers to, so the
            # Blender side never has to guess an object's identity
            "index": index,
            "alias": _alias(md.name),
            "file": "meshes/" + fname,
            "mesh": md.name,
            "bundle": hash_,
            "verts": int(md.vertex_count),
            "bones": len(view["local"]),
        })

    skeleton_file = "skeleton.json"
    with open(os.path.join(outdir, skeleton_file), "w", encoding="utf-8") as fh:
        json.dump({"format": S.FORMAT, "id": skel["id"], "count": skel["count"],
                   "named": skel["named"], "conflicts": skel["conflicts"],
                   "bones": skel["bones"], "meshes": skel["local"]},
                  fh, ensure_ascii=False, indent=1)

    written_textures = []
    failed = []
    for item in spec.get("textures", []):
        hash_ = item["bundle"]
        try:
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
        except Exception as exc:  # noqa: BLE001 - one broken texture must not sink the export
            failed.append({"name": item.get("name", ""), "error": str(exc)[:200]})

    # 动作：只做参考（Mod 不打包动作），但仍然随工作空间一起导出，Blender 端直接播
    written_anims = []
    failed_anims = []
    if spec.get("animations"):
        from . import anim as A

        os.makedirs(anim_dir, exist_ok=True)
        tables: dict[str, tuple] = {}
        for item in spec.get("animations", []):
            hash_ = item["bundle"]
            name = item["clip"]
            try:
                if hash_ not in tables:
                    env, table = A.open_clips(spec["bundles"][hash_])
                    # the prefab in this bundle carries what the clip lacks: the
                    # bone hierarchy (`path` hash -> parent hash) and better names
                    parents, prefab_paths = A.hierarchy(env)
                    tables[hash_] = (table, parents, prefab_paths)
                table, parents, prefab_paths = tables[hash_]
                clip = A.clip_of(table, name)
                if clip is None:
                    raise KeyError("clip not found in bundle")
                fname = _safe(name) + ".exmtanim"
                merged = dict(paths or {})
                merged.update(prefab_paths)
                info = A.export(clip, os.path.join(anim_dir, fname), paths=merged,
                                parents=parents)
                written_anims.append({
                    "file": "animations/" + fname,
                    "clip": name,
                    "bundle": hash_,
                    "seconds": round(clip.length, 3),
                    "samples": info["samples"],
                    "loop": clip.loop,
                })
            except Exception as exc:  # noqa: BLE001 - one broken clip must not sink the export
                failed_anims.append({"clip": name, "error": str(exc)[:200]})

    manifest = {
        "format": "exmt-workspace-1",
        "workspace_id": spec.get("workspace_id") or spec.get("id", ""),
        "label": spec.get("label", ""),
        "created_at": spec.get("created_at") or time.strftime("%Y-%m-%dT%H:%M:%S"),
        "entry": {k: spec.get(k, "") for k in ("id", "name", "display")},
        "game_bundles": spec.get("game_bundles", ""),
        # the pristine bundle each mesh came from: packing a Mod must start from
        # the original file, never from a bundle a previous Mod already replaced
        "bundles": spec.get("bundles") or {},
        # every bundle that stores a copy of each exported mesh / texture (one asset
        # can live in several bundles, and the game may read any of them; textures
        # are duplicated more often than meshes: 72 of 3824 names)
        "mesh_bundles": spec.get("mesh_bundles") or {},
        "texture_bundles": spec.get("texture_bundles") or {},
        "skeleton": {
            "file": skeleton_file,
            "bones": skel["count"],
            "named": skel["named"],
            "conflicts": len(skel["conflicts"]),
        },
        "meshes": written_meshes,
        "textures": written_textures,
        "failed_textures": failed,
        # reference-only: the game plays its own clips, these are for watching the
        # skinning in Blender, so they never take part in packing a Mod
        "animations": written_anims,
        "failed_animations": failed_anims,
    }
    with open(os.path.join(outdir, "workspace.json"), "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, ensure_ascii=False, indent=1)
    # keep the spec next to the workspace: the add-on reads it to find the pristine
    # bundle each mesh came from, so packing works without the app being involved
    spec_copy = os.path.join(outdir, "spec.json")
    if not os.path.exists(spec_copy):
        with open(spec_copy, "w", encoding="utf-8") as fh:
            json.dump(spec, fh, ensure_ascii=False, indent=1)

    return {
        "dir": outdir,
        "meshes": len(written_meshes),
        "textures": len(written_textures),
        "failed_textures": len(failed),
        "animations": len(written_anims),
        "failed_animations": len(failed_anims),
        "bones": skel["count"],
        "named_bones": skel["named"],
        "bytes": sum(os.path.getsize(os.path.join(outdir, m["file"])) for m in written_meshes),
    }
