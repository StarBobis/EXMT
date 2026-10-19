"""`.exmtmesh` interchange format (numpy .npz) used by the Tauri app and the Blender add-on.

This is the **lossless** format: besides the decoded vertex attributes it carries the
mesh's full SerializedFile typetree (`meta["tree"]`), so bone hashes, bind poses,
per-bone AABBs, collision meshes, submesh descriptors and every engine-only flag
survive a Blender round trip.  glTF cannot represent those, which is why the add-on
uses this format for both directions.

Stored arrays
    positions (N,3) f32     normals (N,3) f32      tangents (N,4) f32
    colors    (N,4) f32     uv0     (N,2) f32      uv1 (N,2) f32 (optional)
    weights   (N,4) f32     bones   (N,4) u32
    indices   (M,)  u32
    submesh_first (S,) u32  submesh_count (S,) u32
    bindpose  (B,4,4) f32   bone_hashes (B,) u32
    raw_stream, raw_index   uint8 blobs of the original buffers (byte-identical round trip)
    meta      ()    unicode JSON string (includes the full typetree)
"""

from __future__ import annotations

import base64
import copy
import json

import numpy as np

from . import mesh as M

FORMAT = "exmt-mesh-2"

# fields that hold megabytes of bytes and are moved out of the JSON tree
RAW_FIELDS = {
    "m_IndexBuffer": "raw_index",
    "m_BakedConvexCollisionMesh": "raw_baked_convex",
    "m_BakedTriangleCollisionMesh": "raw_baked_tri",
}


def _extract_tree(tree: dict) -> tuple[dict, dict]:
    """Split a mesh typetree into (json-safe tree, raw byte blobs)."""
    t = copy.deepcopy(tree)
    raw = {}
    vd = t.get("m_VertexData") or {}
    data = vd.get("m_DataSize")
    if isinstance(data, (bytes, bytearray)) and data:
        raw["raw_stream"] = np.frombuffer(bytes(data), dtype=np.uint8)
        vd["m_DataSize"] = None
    for field, key in RAW_FIELDS.items():
        v = t.get(field)
        if isinstance(v, (bytes, bytearray)) and len(v):
            raw[key] = np.frombuffer(bytes(v), dtype=np.uint8)
            t[field] = None
        elif isinstance(v, list) and v and isinstance(v[0], int):
            raw[key] = np.asarray(v, dtype=np.uint8)
            t[field] = None
    return t, raw


def save(md: M.MeshData, path: str, meta: dict | None = None, tree: dict | None = None) -> None:
    arrs = {}
    names = {0: "positions", 1: "normals", 2: "tangents", 3: "colors", 4: "uv0", 5: "uv1"}
    for attr, arr in md.arrays.items():
        key = names.get(attr)
        if not key:
            continue
        arrs[key] = np.asarray(arr, dtype=np.float32)
    arrs["weights"] = np.asarray(md.arrays.get(12, np.zeros((md.vertex_count, 4))), dtype=np.float32)
    arrs["bones"] = np.asarray(md.arrays.get(13, np.zeros((md.vertex_count, 4))), dtype=np.uint32)
    arrs["indices"] = np.asarray(md.indices, dtype=np.uint32)
    width = 2 if md.index_format == 0 else 4
    arrs["submesh_first"] = np.array([s["firstByte"] // width for s in md.submeshes], dtype=np.uint32)
    arrs["submesh_count"] = np.array([s["indexCount"] for s in md.submeshes], dtype=np.uint32)
    if md.bindpose is not None:
        arrs["bindpose"] = np.asarray(md.bindpose, dtype=np.float32)
    arrs["bone_hashes"] = np.asarray(md.bone_hashes, dtype=np.uint32)

    info = {
        "format": FORMAT,
        "name": md.name,
        "vertex_count": int(md.vertex_count),
        "index_format": int(md.index_format),
        "channels": md.channels,
        "aabb": md.aabb,
        "submeshes": md.submeshes,
        "has_bindpose": md.bindpose is not None,
    }
    if meta:
        info.update(meta)
    if tree is not None:
        info["tree"], raw = _extract_tree(tree)
        arrs.update(raw)
    arrs["meta"] = np.array(json.dumps(info, ensure_ascii=False))
    with open(path, "wb") as fh:
        np.savez_compressed(fh, **arrs)


def load(path: str, template: dict | None = None) -> tuple[M.MeshData, dict]:
    z = np.load(path, allow_pickle=False)
    info = json.loads(str(z["meta"]))
    channels = list(info["channels"])
    while len(channels) < 14:
        channels.append({"stream": 0, "offset": 0, "format": 0, "dimension": 0})
    # only keep channels that actually exist in the target layout
    if template is not None:
        tch = template["m_VertexData"]["m_Channels"]
        for i in range(14):
            if i < len(tch) and tch[i]["dimension"] == 0:
                channels[i] = {"stream": 0, "offset": 0, "format": 0, "dimension": 0}

    arrays = {}
    mapping = {"positions": 0, "normals": 1, "tangents": 2, "colors": 3, "uv0": 4, "uv1": 5,
               "weights": 12, "bones": 13}
    for key, attr in mapping.items():
        if key in z and channels[attr]["dimension"]:
            arrs = np.asarray(z[key])
            dim = channels[attr]["dimension"]
            arrays[attr] = arrs[:, :dim]

    count = int(arrays[0].shape[0]) if 0 in arrays else int(info["vertex_count"])
    for attr in list(arrays):
        if arrays[attr].shape[0] != count:
            raise ValueError("attribute %d length mismatch" % attr)
        if channels[attr]["dimension"] and arrays[attr].shape[1] != channels[attr]["dimension"]:
            arrays[attr] = arrays[attr][:, :channels[attr]["dimension"]]

    indices = np.asarray(z["indices"], dtype=np.uint16 if info["index_format"] == 0 else np.uint32)
    first = np.asarray(z["submesh_first"], dtype=np.uint32)
    cnt = np.asarray(z["submesh_count"], dtype=np.uint32)
    width = 2 if int(info["index_format"]) == 0 else 4
    subs = [{"firstByte": int(first[i]) * width, "indexCount": int(cnt[i]), "topology": 0,
             "baseVertex": 0, "firstVertex": 0, "vertexCount": 0, "localAABB": {}} for i in range(len(cnt))]
    # the file only stores first/count; the target's per-submesh metadata (local
    # culling AABB, base/first vertex, topology) has to survive the round trip
    template_subs = (template or {}).get("m_SubMeshes") or []
    for i in range(min(len(subs), len(template_subs))):
        src_sub = template_subs[i] or {}
        for field in ("topology", "baseVertex", "firstVertex", "vertexCount"):
            if src_sub.get(field) is not None:
                subs[i][field] = src_sub[field]
        if src_sub.get("localAABB"):
            subs[i]["localAABB"] = src_sub["localAABB"]

    raw = {}
    for name in ("raw_stream", "raw_index", "raw_baked_convex", "raw_baked_tri"):
        if name in z:
            raw[name] = np.asarray(z[name], dtype=np.uint8).tobytes()

    return M.MeshData(
        name=info.get("name", ""),
        vertex_count=count,
        channels=channels,
        arrays=arrays,
        indices=indices,
        index_format=int(info["index_format"]),
        submeshes=subs,
        bindpose=np.asarray(z["bindpose"], dtype=np.float32) if "bindpose" in z else None,
        bone_hashes=[int(x) for x in np.asarray(z["bone_hashes"])] if "bone_hashes" in z else [],
        aabb=info.get("aabb") or {},
        raw_data=b"",
    ), {"first": first, "count": cnt, "info": info, "raw": raw}


def rebuild_submeshes(md: M.MeshData, first, count) -> None:
    """Recompute submesh descriptors for a (possibly new) index buffer.

    The per-submesh `localAABB` is what Unity culls with, so it must cover the
    vertices this submesh *references* — `pos[min:max+1]` would also pull in
    unreferenced ones and inflate the box (which then shows up as a diff against
    the game's own numbers on an untouched mesh).
    """
    pos = md.arrays.get(0)
    width = 2 if md.index_format == 0 else 4
    subs = []
    total = len(md.indices)
    f = 0
    for i in range(len(count)):
        n = int(count[i])
        if n <= 0:
            continue
        idx = md.indices[f:f + n].astype(np.uint32)
        lo = int(idx.min())
        hi = int(idx.max()) + 1
        if pos is not None and n:
            aabb = M.aabb_from_vertices(np.asarray(pos[idx], dtype=np.float32))
        elif pos is not None and hi > 0:
            aabb = M.aabb_from_vertices(np.asarray(pos[lo:hi], dtype=np.float32))
        else:
            aabb = md.aabb
        subs.append({
            "firstByte": int(f) * width,
            "indexCount": n,
            "topology": 0,
            "baseVertex": 0,
            "firstVertex": lo,
            "vertexCount": hi - lo,
            "localAABB": aabb,
        })
        f += n
    if f != total:
        raise ValueError("submeshes cover %d of %d indices" % (f, total))
    md.submeshes = subs
