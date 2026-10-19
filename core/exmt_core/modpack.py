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

from . import interchange, mesh as M
from .sf import open_bundle

FORMAT = "exmt-mod-1"


def patch_mesh(bundle, mesh_name: str, src: str) -> dict:
    """Replace one mesh's geometry inside an opened bundle (saved by the caller).

    The `.exmtmesh` carries the mesh's full typetree, so packing restores every
    field the game shipped (bone hashes, bind poses, per-bone AABBs, collision
    meshes, usage flags …) and only replaces the geometry.
    """
    o = bundle.mesh(mesh_name)
    target_tree = o.read_typetree()
    md, extra = interchange.load(src, target_tree)

    base = extra["info"].get("tree")
    if base:
        tree = copy.deepcopy(base)
        # the target bundle's own name must win
        tree["m_Name"] = target_tree.get("m_Name", tree.get("m_Name"))
    else:
        tree = target_tree

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
            "tree_restored": bool(base)}


def _clear(outdir: str) -> None:
    for name in os.listdir(outdir):
        if name.endswith(".bundle") or name in ("mod.json", "mod.json.tmp"):
            os.remove(os.path.join(outdir, name))


def build(spec: dict) -> dict:
    """`spec` = {outdir, label, workspace, bundles: {hash: pristine path},
    meshes: [{bundle, mesh, file}]} → writes bundles + `mod.json`, returns a summary."""
    outdir = spec["outdir"]
    os.makedirs(outdir, exist_ok=True)
    _clear(outdir)

    bundles = spec.get("bundles") or {}
    grouped: dict = {}
    for item in spec.get("meshes") or []:
        grouped.setdefault(item["bundle"], []).append(item)

    written = []
    missing = []
    for hash_, items in grouped.items():
        source = bundles.get(hash_)
        if not source or not os.path.isfile(source):
            missing.append({"hash": hash_, "meshes": [i["mesh"] for i in items]})
            continue
        bundle = open_bundle(source)
        patched = []
        for item in items:
            if not os.path.isfile(item["file"]):
                missing.append({"hash": hash_, "mesh": item["mesh"], "file": item["file"]})
                continue
            info = patch_mesh(bundle, item["mesh"], item["file"])
            patched.append(info)
        if not patched:
            continue
        out = os.path.join(outdir, hash_ + ".bundle")
        data = bundle.save(out)
        written.append({
            "hash": hash_,
            "file": os.path.basename(out),
            "bytes": len(data),
            "source_bytes": os.path.getsize(source),
            "meshes": [p["mesh"] for p in patched],
            "verts": sum(p["verts"] for p in patched),
            "source": source,
        })

    manifest = {
        "format": FORMAT,
        "label": spec.get("label", ""),
        "workspace": spec.get("workspace", ""),
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "game_bundles": spec.get("game_bundles", ""),
        "mesh_count": sum(len(b["meshes"]) for b in written),
        "bundles": written,
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
        "missing": len(missing),
        "bytes": sum(b["bytes"] for b in written),
        "files": [b["file"] for b in written],
    }
