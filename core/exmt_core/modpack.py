"""Pack edited meshes back into bundles and stage them as an installable Mod.

The Blender add-on exports every mesh of a workspace and hands them here; this
module repacks them into *copies* of the game bundles and writes the result into
`<workspace>/GeneratedMod/`:

    GeneratedMod/
      <bundle hash>.bundle      patched copy of one game bundle
      mod.json                  what to install where (the app reads this)

Nothing here touches the game folder — installing is the app's job, so the files
can be inspected, copied or thrown away first.  Every bundle is patched in one
pass: several edited meshes usually live in the same bundle, and writing the
bundle once keeps the other objects byte-identical.
"""

from __future__ import annotations

import copy
import json
import os
import time

import numpy as np

from . import interchange, mesh as M
from . import texpatch as T
from .sf import open_bundle

FORMAT = "exmt-mod-1"

_BONE_NAMES = None


def _bone_name(hash_: int):
    """Full bone path for a hash, using the app's name index when it is available.

    Without the index there is nothing to match by name, so the caller simply
    keeps the hash-only behaviour.
    """
    global _BONE_NAMES
    if _BONE_NAMES is None:
        paths = {}
        try:
            from . import bonenames as _bn
            candidates = []
            cfg = os.path.join(os.environ.get("APPDATA", ""), "com.exmt.modbox", "EXMTConfig.json")
            if os.path.isfile(cfg):
                with open(cfg, encoding="utf-8") as fh:
                    candidates.append(str((json.load(fh) or {}).get("bone_names") or ""))
            candidates.append(r"C:\EXMTCacheFolder\bonenames.json")
            for candidate in candidates:
                if candidate and os.path.isfile(candidate):
                    paths = _bn.load(candidate) or {}
                    break
        except Exception:
            paths = {}
        _BONE_NAMES = paths
    return _BONE_NAMES.get(hash_) or _BONE_NAMES.get(str(hash_))


def _compatible(base: dict, target: dict) -> bool:
    """May the patch file's own tree be written back as the target's tree?

    True only when the two describe the same asset: a mesh name can appear in
    several bundles, and the copies are **not** always cooked identically (measured:
    `c_AlvaSSR0101_slg_cloth3_lod0` in `0e834224…` stores UV0 as float16 and has
    `m_IsReadable = false`, while the copy in `a75eb9c2…` uses float32 and is
    readable). Writing one copy's tree into the other would rewrite fields the
    user never touched.
    """
    if not base:
        return False
    bv = base.get("m_VertexData") or {}
    tv = target.get("m_VertexData") or {}
    if bv.get("m_Channels") != tv.get("m_Channels"):
        return False
    if base.get("m_IsReadable") != target.get("m_IsReadable"):
        return False
    if list(base.get("m_BoneNameHashes") or []) != list(target.get("m_BoneNameHashes") or []):
        return False
    return True


def _fit_channels(md: M.MeshData, channels: list) -> None:
    """Make the arrays match a layout (pad or trim each attribute to its width)."""
    for attr, ch in enumerate(channels):
        dim = int(ch.get("dimension") or 0)
        arr = md.arrays.get(attr)
        if arr is None:
            continue
        if dim == 0:
            md.arrays.pop(attr, None)
        elif arr.shape[1] > dim:
            md.arrays[attr] = arr[:, :dim]
        elif arr.shape[1] < dim:
            pad = np.zeros((arr.shape[0], dim - arr.shape[1]), dtype=arr.dtype)
            md.arrays[attr] = np.concatenate([arr, pad], axis=1)
    md.channels = [dict(c) for c in channels]


def _remap_bones(md: M.MeshData, from_hashes, to_hashes) -> int:
    """Re-index the bone slots through the hash table (drops unmatched slots).

    A game mesh can be cooked in more than one generation and the copies disagree
    (measured 2026-10-19, `c_VepleySSR01_slg_cloth_lod0`): one generation keeps a
    `Hip_M` link between `Root_M` and the legs, the other deleted it, so every hash
    below that point changes while the bone keeps its short name.  Hash alone then
    drops 15 influences and 1954 of 12734 vertices lose every driver; falling back
    to the short name recovers 12 of the 15 (kept weight 69.6% -> 85.8%, orphan
    vertices 1954 -> 0).
    """
    lookup = {int(h): i for i, h in enumerate(to_hashes)}
    slots = md.arrays.get(13)
    if slots is None:
        return 0

    by_short = None
    for i, h in enumerate(to_hashes):
        name = _bone_name(int(h))
        if name:
            if by_short is None:
                by_short = {}
            by_short.setdefault(name.rsplit("/", 1)[-1], i)

    dropped = 0
    for row in range(slots.shape[0]):
        for col in range(slots.shape[1]):
            old = int(slots[row, col])
            hash_ = int(from_hashes[old]) if 0 <= old < len(from_hashes) else None
            new = lookup.get(hash_) if hash_ is not None else None
            if new is None and hash_ is not None and by_short:
                # same bone, re-parented between game versions
                name = _bone_name(hash_)
                if name:
                    new = by_short.get(name.rsplit("/", 1)[-1])
            if new is None:
                if float(md.arrays[12][row, col]) > 0:
                    dropped += 1
                slots[row, col] = 0
                if 12 in md.arrays:
                    md.arrays[12][row, col] = 0.0
            else:
                slots[row, col] = new
    return dropped


def patch_mesh(bundle, mesh_name: str, src: str) -> dict:
    """Replace one mesh's geometry inside an opened bundle (saved by the caller).

    The `.exmtmesh` carries the mesh's full typetree, so packing restores every
    field the game shipped (bone hashes, bind poses, per-bone AABBs, collision
    meshes, usage flags …) and only replaces the geometry. When the target bundle
    stores the asset differently, **the target's tree wins** and only the geometry
    (plus the weights/bones it carries) is written into it.
    """
    o = bundle.mesh(mesh_name)
    target_tree = o.read_typetree()
    md, extra = interchange.load(src, target_tree)

    base = extra["info"].get("tree")
    same_asset = _compatible(base, target_tree)
    if same_asset:
        tree = copy.deepcopy(base)
        # the target bundle's own name must win
        tree["m_Name"] = target_tree.get("m_Name", tree.get("m_Name"))
        dropped = 0
    else:
        tree = copy.deepcopy(target_tree)
        dropped = _remap_bones(
            md,
            list((base or {}).get("m_BoneNameHashes") or md.bone_hashes or []),
            list(target_tree.get("m_BoneNameHashes") or []),
        )
        _fit_channels(md, (target_tree.get("m_VertexData") or {}).get("m_Channels") or md.channels)

    # restore binary blobs that we do not regenerate (collision meshes)
    raw = extra.get("raw") or {}
    for field, key in (("m_BakedConvexCollisionMesh", "raw_baked_convex"),
                       ("m_BakedTriangleCollisionMesh", "raw_baked_tri")):
        if tree.get(field) is None:
            if key in raw:
                tree[field] = raw[key]
            elif target_tree.get(field) is not None:
                tree[field] = target_tree[field]

    interchange.rebuild_submeshes(md, extra["first"], extra["count"])
    md.aabb = M.aabb_from_vertices(md.arrays[0].astype("float32"))
    M.to_tree(tree, md)
    o.save_typetree(tree)
    return {"mesh": md.name, "verts": md.vertex_count, "indices": int(len(md.indices)),
            "tree_restored": bool(same_asset), "dropped_bones": dropped}


def _clear(outdir: str) -> None:
    for name in os.listdir(outdir):
        if name.endswith(".bundle") or name in ("mod.json", "mod.json.tmp"):
            os.remove(os.path.join(outdir, name))


def build(spec: dict) -> dict:
    """`spec` = {outdir, label, workspace, bundles: {hash: pristine path},
    meshes: [{bundle, mesh, file}], textures: [{bundle, name, file, format, width, height, mips}]}
    → writes bundles + `mod.json`, returns a summary.

    `textures[].file` is the **already encoded** block payload (the Blender add-on
    runs texconv, the app reuses whatever the Mod shipped), so this side never
    needs an image codec.
    """
    outdir = spec["outdir"]
    os.makedirs(outdir, exist_ok=True)
    _clear(outdir)

    bundles = spec.get("bundles") or {}
    grouped: dict = {}
    for item in spec.get("meshes") or []:
        grouped.setdefault(item["bundle"], []).append(item)
    tex_grouped: dict = {}
    for item in spec.get("textures") or []:
        tex_grouped.setdefault(item["bundle"], []).append(item)

    written = []
    missing = []
    for hash_ in list(dict.fromkeys(list(grouped) + list(tex_grouped))):
        source = bundles.get(hash_)
        if not source or not os.path.isfile(source):
            missing.append({"hash": hash_,
                            "meshes": [i["mesh"] for i in grouped.get(hash_, [])],
                            "textures": [i["name"] for i in tex_grouped.get(hash_, [])]})
            continue
        bundle = open_bundle(source)
        patched = []
        patched_tex = []
        for item in grouped.get(hash_, []):
            if not os.path.isfile(item["file"]):
                missing.append({"hash": hash_, "mesh": item["mesh"], "file": item["file"]})
                continue
            info = patch_mesh(bundle, item["mesh"], item["file"])
            patched.append(info)
        for item in tex_grouped.get(hash_, []):
            if not os.path.isfile(item["file"]):
                missing.append({"hash": hash_, "texture": item["name"], "file": item["file"]})
                continue
            with open(item["file"], "rb") as fh:
                payload = fh.read()
            info = T.apply(bundle, item["name"], payload, item)
            patched_tex.append(info)
        if not patched and not patched_tex:
            continue
        out = os.path.join(outdir, hash_ + ".bundle")
        data = bundle.save(out)
        written.append({
            "hash": hash_,
            "file": os.path.basename(out),
            "bytes": len(data),
            "source_bytes": os.path.getsize(source),
            "meshes": [p["mesh"] for p in patched],
            "textures": [p["name"] for p in patched_tex],
            "verts": sum(p["verts"] for p in patched),
            "source": source,
        })

    # keep the geometry patches next to the packed bundle: merging two Mods means
    # applying both patch sets to a fresh copy of the same bundle, so the patch
    # files (not the already-packed bundle) are the part that must survive
    patches: dict = {}
    for item in spec.get("meshes") or []:
        entry = patches.setdefault(item["file"], {"mesh": item["mesh"], "bundles": []})
        if item["bundle"] not in entry["bundles"]:
            entry["bundles"].append(item["bundle"])
    patch_rows = []
    for file, entry in patches.items():
        rel = None
        try:
            rel = os.path.relpath(file, outdir).replace("\\", "/")
        except ValueError:
            rel = None
        inside = rel is not None and not rel.startswith("../")
        patch_rows.append({
            "mesh": entry["mesh"],
            "file": rel if inside else file,
            "inside": bool(inside),
            "bundles": entry["bundles"],
        })

    # same idea for textures: keep the encoded payload so a merge can replay it
    tex_rows = []
    tex_by_file: dict = {}
    for item in spec.get("textures") or []:
        entry = tex_by_file.setdefault(item["file"], {
            "name": item["name"], "format": int(item.get("format") or 0),
            "width": int(item.get("width") or 0), "height": int(item.get("height") or 0),
            "mips": int(item.get("mips") or 1), "bundles": [],
        })
        if item["bundle"] not in entry["bundles"]:
            entry["bundles"].append(item["bundle"])
    for file, entry in tex_by_file.items():
        try:
            rel = os.path.relpath(file, outdir).replace("\\", "/")
        except ValueError:
            rel = None
        inside = rel is not None and not rel.startswith("../")
        tex_rows.append({
            "name": entry["name"],
            "file": rel if inside else file,
            "inside": bool(inside),
            "format": entry["format"],
            "width": entry["width"],
            "height": entry["height"],
            "mips": entry["mips"],
            "bundles": entry["bundles"],
        })

    manifest = {
        "format": FORMAT,
        "label": spec.get("label", ""),
        "workspace": spec.get("workspace", ""),
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "game_bundles": spec.get("game_bundles", ""),
        "mesh_count": sum(len(b["meshes"]) for b in written),
        "bundles": written,
        "patches": patch_rows,
        "textures": tex_rows,
        "missing": missing,
    }
    target = os.path.join(outdir, "mod.json")
    tmp = target + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, ensure_ascii=False, indent=1)
    os.replace(tmp, target)

    return {
        "dir": outdir,
        "bundles": len(written),
        "meshes": manifest["mesh_count"],
        "textures": len(tex_rows),
        "missing": len(missing),
        "bytes": sum(b["bytes"] for b in written),
        "files": [b["file"] for b in written],
    }
