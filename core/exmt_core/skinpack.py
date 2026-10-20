"""Decode one skin in a single process and write the studio's cache folder.

The studio needs four things per skin: the submesh list, their geometry, the
texture thumbnails and the diffuse map per submesh. Each used to be its own
`exmt_core` call — 16 process starts, ~1 s of Python/UnityPy import each, so
"open another skin" cost 20–25 s even when every bundle was warm.

`skinpack <spec.json>` does all of it in **one** process, opening each bundle at
most once, and writes the exact files the studio caches under
`<cache>/skins/<key>/`:

    meshes.json    [{name, verts, tris, bones, bundle}]
    geometry.json  {meshes: [<payload>]}   (base64 f32 for three.js)
    thumbs.json    [{name, width, height, png}]
    diffuse.json   {meshName: png}
    meta.json      {version, key, built_at, cache_built_at}

`cache_built_at` ties the pack to one analysis: after `analyze` runs again the
manifest's `built_at` changes and every old pack is treated as stale — no
mtime signatures to reconcile.
"""

from __future__ import annotations

import json
import os
import time

FORMAT = "exmt-skinpack-1"


def _write(path: str, payload) -> None:
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, separators=(",", ":"))
    os.replace(tmp, path)


def build(spec: dict) -> dict:
    """`spec` = {key, dir, bundle_dir, meshes, textures, maps, cache_built_at}."""
    from . import mesh as M
    from . import preview as P
    from .sf import open_bundle

    key = spec["key"]
    out_dir = spec["dir"]
    bundle_dir = spec["bundle_dir"]
    os.makedirs(out_dir, exist_ok=True)
    started = time.time()

    def path_of(hash_: str) -> str:
        return os.path.join(bundle_dir, hash_ + ".bundle")

    # --- submesh list + geometry: one parse per mesh, bundles opened once ----
    opened: dict[str, object] = {}
    meshes: list = []
    geometry: list = []
    for item in spec.get("meshes", []):
        candidates = item.get("bundles") or [item.get("bundle")]
        name = item["mesh"]
        error = ""
        for hash_ in [h for h in candidates if h]:
            try:
                bundle = opened.get(hash_)
                if bundle is None:
                    bundle = opened[hash_] = open_bundle(path_of(hash_))
                md = M.from_tree(bundle.mesh(name).read_typetree())
                payload = P._geometry_payload(md)
                payload["bundle"] = hash_
                payload["name"] = md.name
                geometry.append(payload)
                meshes.append({
                    "name": md.name,
                    "verts": int(md.vertex_count),
                    "tris": int(len(md.indices) // 3),
                    "bones": int(len(md.bone_hashes)),
                    "bundle": hash_,
                })
                error = ""
                break
            except Exception as exc:  # noqa: BLE001 - try the next copy of the mesh
                error = str(exc)[:200]
        else:
            # every copy failed (or there was no bundle): keep the row, minus geometry
            meshes.append({"name": name, "bundle": (candidates or [""])[0], "error": error})

    # --- texture thumbnails: still one open per bundle, but in-process ------
    thumb_size = int(spec.get("thumb_size") or 160)
    thumbs: list = []
    for group in spec.get("textures", []):
        hash_ = group["bundle"]
        for row in P.thumbs(path_of(hash_), group.get("names") or [], thumb_size):
            row["bundle"] = hash_
            thumbs.append(row)
    thumbs.sort(key=lambda row: row.get("name", ""))

    # --- diffuse maps (downscaled) -----------------------------------------
    maps: dict = {}
    try:
        bundles = {item["bundle"]: path_of(item["bundle"]) for item in spec.get("maps", [])}
        if bundles:
            data = P.diffuse_maps(bundles, spec.get("maps", []),
                                  int(spec.get("map_size") or 1024))
            for row in data.get("maps", []):
                if row.get("png"):
                    maps[row["mesh"]] = row["png"]
    except Exception:  # noqa: BLE001 - a missing map only loses the preview
        maps = {}

    _write(os.path.join(out_dir, "meshes.json"), meshes)
    _write(os.path.join(out_dir, "geometry.json"), {"meshes": geometry})
    _write(os.path.join(out_dir, "thumbs.json"), thumbs)
    _write(os.path.join(out_dir, "diffuse.json"), maps)
    _write(os.path.join(out_dir, "meta.json"), {
        "format": FORMAT,
        "version": 2,
        "key": key,
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "cache_built_at": spec.get("cache_built_at") or "",
    })

    return {
        "format": FORMAT,
        "key": key,
        "dir": out_dir,
        "meshes": len(meshes),
        "geometry": len(geometry),
        "textures": len(thumbs),
        "maps": len(maps),
        "seconds": round(time.time() - started, 2),
    }
