"""Minimal glTF 2.0 (.glb) writer for skinned character meshes.

Each `MeshData` becomes one skinned mesh node plus its own skeleton (joints are
exported flat, with `inverseBindMatrices` = the game's bind poses; a flat
skeleton is exact for glTF because every joint node carries its world rest
matrix).
"""

from __future__ import annotations

import json
import struct

import numpy as np

from .mesh import MeshData

FLOAT = 5126
USHORT = 5123
UINT = 5125


class _Builder:
    def __init__(self):
        self.bin = bytearray()
        self.views = []
        self.accessors = []

    def _align(self, n: int = 4):
        while len(self.bin) % n:
            self.bin.append(0)

    def view(self, data: bytes, target: int | None = None) -> int:
        self._align(4)
        offset = len(self.bin)
        self.bin += data
        v = {"buffer": 0, "byteOffset": offset, "byteLength": len(data)}
        if target:
            v["target"] = target
        self.views.append(v)
        return len(self.views) - 1

    def accessor(self, array: np.ndarray, comp: int, type_: str, target: int | None = None,
                 minmax: bool = False) -> int:
        arr = np.ascontiguousarray(array)
        view = self.view(arr.tobytes(), target)
        acc = {
            "bufferView": view,
            "componentType": comp,
            "count": int(arr.shape[0]),
            "type": type_,
        }
        if minmax:
            acc["min"] = [float(x) for x in np.asarray(arr).min(axis=0)]
            acc["max"] = [float(x) for x in np.asarray(arr).max(axis=0)]
        self.accessors.append(acc)
        return len(self.accessors) - 1


def _column_major(m: np.ndarray) -> list:
    return [float(x) for x in np.asarray(m, dtype=np.float32).reshape(-1)]


def build_glb(meshes: list[tuple[str, MeshData]], path: str, with_skin: bool = True) -> dict:
    b = _Builder()
    nodes: list[dict] = []
    gltf_meshes: list[dict] = []
    skins: list[dict] = []
    scene_nodes: list[int] = []

    for label, md in meshes:
        attrs = {}
        pos = np.asarray(md.arrays[0], dtype=np.float32)
        attrs["POSITION"] = b.accessor(pos, FLOAT, "VEC3", 34962, minmax=True)
        if 1 in md.arrays:
            attrs["NORMAL"] = b.accessor(np.asarray(md.arrays[1], np.float32), FLOAT, "VEC3", 34962)
        if 2 in md.arrays:
            attrs["TANGENT"] = b.accessor(np.asarray(md.arrays[2], np.float32), FLOAT, "VEC4", 34962)
        if 4 in md.arrays:
            attrs["TEXCOORD_0"] = b.accessor(np.asarray(md.arrays[4], np.float32), FLOAT, "VEC2", 34962)
        if 3 in md.arrays:
            attrs["COLOR_0"] = b.accessor(np.asarray(md.arrays[3], np.float32), FLOAT, "VEC4", 34962)

        skin_index = None
        if with_skin and 12 in md.arrays and 13 in md.arrays and md.bindpose is not None:
            w = np.asarray(md.arrays[12], np.float32)
            j = np.asarray(md.arrays[13], np.uint32)
            # renormalise + drop influences pointing past the joint list
            limit = len(md.bindpose)
            j = np.where(j < limit, j, 0)
            s = w.sum(axis=1, keepdims=True)
            s[s <= 0] = 1.0
            w = w / s
            attrs["JOINTS_0"] = b.accessor(j.astype(np.uint16), USHORT, "VEC4", 34962)
            attrs["WEIGHTS_0"] = b.accessor(w, FLOAT, "VEC4", 34962)

            n_joints = len(md.bindpose)
            joint_nodes = []
            arm_index = len(nodes)
            nodes.append({"name": f"{label}_armature", "children": []})
            ibm = np.zeros((n_joints, 16), dtype=np.float32)
            for k in range(n_joints):
                world = np.linalg.inv(np.asarray(md.bindpose[k], dtype=np.float64)).astype(np.float32)
                idx = len(nodes)
                nodes.append({"name": f"{label}_b{k}", "matrix": _column_major(world)})
                joint_nodes.append(idx)
                ibm[k] = _column_major(md.bindpose[k])
            nodes[arm_index]["children"] = joint_nodes

            ibm_view = b.view(ibm.tobytes())
            ibm_acc = len(b.accessors)
            b.accessors.append({
                "bufferView": ibm_view,
                "componentType": FLOAT,
                "count": n_joints,
                "type": "MAT4",
            })
            skins.append({
                "name": f"{label}_skin",
                "joints": joint_nodes,
                "inverseBindMatrices": ibm_acc,
                "skeleton": arm_index,
            })
            skin_index = len(skins) - 1
            scene_nodes.append(arm_index)

        # indices
        idx = np.asarray(md.indices)
        comp = USHORT if idx.max() < 65536 else UINT
        attrs_idx = b.accessor(idx.astype(np.uint16 if comp == USHORT else np.uint32),
                               comp, "SCALAR", 34963)

        prims = []
        width = 2 if md.index_format == 0 else 4
        for sub in md.submeshes or [{"firstByte": 0, "indexCount": len(idx)}]:
            first = int(sub["firstByte"]) // width
            count = int(sub["indexCount"])
            acc = attrs_idx
            if first or count != len(idx):
                chunk = idx[first:first + count].astype(np.uint32)
                acc = b.accessor(chunk.astype(np.uint16 if chunk.max() < 65536 else np.uint32),
                                 USHORT if chunk.max() < 65536 else UINT, "SCALAR", 34963)
            prims.append({"attributes": attrs, "indices": acc, "mode": 4})

        gltf_meshes.append({"name": label, "primitives": prims})
        node = {"name": label, "mesh": len(gltf_meshes) - 1}
        if skin_index is not None:
            node["skin"] = skin_index
        scene_nodes.append(len(nodes))
        nodes.append(node)

    gltf = {
        "asset": {"version": "2.0", "generator": "EXMT Modbox 0.1"},
        "scene": 0,
        "scenes": [{"nodes": scene_nodes}],
        "nodes": nodes,
        "meshes": gltf_meshes,
        "buffers": [{"byteLength": len(b.bin)}],
        "bufferViews": b.views,
        "accessors": b.accessors,
    }
    if skins:
        gltf["skins"] = skins

    js = json.dumps(gltf, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    while len(js) % 4:
        js += b" "
    bin_data = bytes(b.bin)
    while len(bin_data) % 4:
        bin_data += b"\0"

    total = 12 + 8 + len(js) + 8 + len(bin_data)
    with open(path, "wb") as fh:
        fh.write(struct.pack("<III", 0x46546C67, 2, total))
        fh.write(struct.pack("<II", len(js), 0x4E4F534A))
        fh.write(js)
        fh.write(struct.pack("<II", len(bin_data), 0x004E4942))
        fh.write(bin_data)

    return {
        "file": path,
        "bytes": total,
        "meshes": len(gltf_meshes),
        "nodes": len(nodes),
        "skins": len(skins),
    }
