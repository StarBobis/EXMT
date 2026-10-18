"""Triangle subdivision for `.exmtmesh` (used to grow a mesh past its original size).

Each subdivided triangle adds three midpoint vertices whose attributes are
interpolated; skin weights are merged (bone indices cannot be averaged, so the
three parents' weights are combined and reduced back to 4 influences).

There is no edge sharing: the output is "vertex-split" subdivision, which is
slightly wasteful but keeps the code trivial and the visual result identical.
"""

from __future__ import annotations

import numpy as np

from .mesh import MeshData

ATTRS = (0, 1, 2, 3, 4)  # position, normal, tangent, color, uv0


def _merge_weights(pairs) -> tuple:
    acc: dict[int, float] = {}
    for b, w in pairs:
        if w > 0:
            acc[int(b)] = acc.get(int(b), 0.0) + float(w)
    items = sorted(acc.items(), key=lambda kv: -kv[1])[:4]
    total = sum(v for _, v in items) or 1.0
    bones = [0, 0, 0, 0]
    weights = [0.0, 0.0, 0.0, 0.0]
    for i, (b, v) in enumerate(items):
        bones[i] = b
        weights[i] = v / total
    return bones, weights


def subdivide(mesh: MeshData, target_vertices: int) -> MeshData:
    arrays = {k: np.asarray(v) for k, v in mesh.arrays.items()}
    pos = arrays.get(0)
    if pos is None:
        raise ValueError("mesh has no positions")
    bones = arrays.get(13)
    weights = arrays.get(12)

    out_arrays = {k: [np.asarray(v[i], dtype=np.float32) for i in range(len(v))]
                  for k, v in arrays.items() if k in ATTRS}
    out_bones: list = [bones[i].tolist() if bones is not None else [0, 0, 0, 0]
                       for i in range(len(pos))]
    out_weights: list = [weights[i].tolist() if weights is not None else [0.0, 0.0, 0.0, 0.0]
                         for i in range(len(pos))]

    tris = np.asarray(mesh.indices, dtype=np.int64).reshape(-1, 3).tolist()

    def make_mid(i, j):
        new = {}
        for k, buf in out_arrays.items():
            a, b = np.asarray(buf[i], np.float32), np.asarray(buf[j], np.float32)
            new[k] = ((a + b) * 0.5).tolist()
        pairs = []
        for v in (i, j):
            for bi, bw in zip(out_bones[v], out_weights[v]):
                if bw > 0:
                    pairs.append((bi, bw * 0.5))
        b, w = _merge_weights(pairs)
        for k, buf in out_arrays.items():
            buf.append(new[k])
        out_bones.append(b)
        out_weights.append(w)
        return len(out_bones) - 1

    level = 0
    while len(out_bones) < target_vertices:
        need = target_vertices - len(out_bones)
        limit = min(len(tris), need // 3)
        if limit <= 0:
            break
        new_tris = []
        for idx, (a, b, c) in enumerate(tris):
            if idx >= limit:
                new_tris.append((a, b, c))
                continue
            ab = make_mid(a, b)
            bc = make_mid(b, c)
            ca = make_mid(c, a)
            new_tris += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
        partial = limit < len(tris)
        tris = new_tris
        level += 1
        if partial:
            break

    data = {k: np.asarray(v, dtype=np.float32) for k, v in out_arrays.items()}
    data[12] = np.asarray(out_weights, dtype=np.float32)
    data[13] = np.asarray(out_bones, dtype=np.uint32)
    indices = np.asarray(tris, dtype=np.uint32).reshape(-1)
    fmt = 0 if len(indices) == 0 or indices.max() < 65536 else 1

    md = MeshData(
        name=mesh.name,
        vertex_count=len(out_bones),
        channels=mesh.channels,
        arrays=data,
        indices=indices,
        index_format=fmt,
        submeshes=[{"firstByte": 0, "indexCount": int(len(indices))}],
        bindpose=mesh.bindpose,
        bone_hashes=mesh.bone_hashes,
        aabb={},
    )
    md.aabb = {}
    return md
