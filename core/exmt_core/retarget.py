"""Retarget a skinned mesh onto another skeleton by matching bone rest positions.

Used when swapping a garment that was rigged for a different bone list (e.g. the
dorm outfit with 43 bones) onto the target character's rig (126 bones).

Both meshes are authored in the same bind space, so only the weights need to be
remapped; positions/UVs/indices are kept as-is.
"""

from __future__ import annotations

import numpy as np

from .mesh import MeshData


def bind_positions(bindpose: np.ndarray) -> np.ndarray:
    """World-space rest position of every bone (bindpose is world->bone)."""
    inv = np.linalg.inv(bindpose)
    return inv[:, :3, 3]


def match_bones(src_bind: np.ndarray, dst_bind: np.ndarray,
                src_hashes=None, dst_hashes=None) -> tuple:
    """Map src bone -> dst bone.

    Bone *name hashes* are exact identity, so they win; whatever is left is
    matched by nearest rest position.  Returns (mapping, exact_mask).
    """
    src = bind_positions(src_bind)
    dst = bind_positions(dst_bind)
    mapping = np.full(len(src), -1, dtype=np.int32)
    exact = np.zeros(len(src), dtype=bool)
    used = set()

    if src_hashes is not None and dst_hashes is not None:
        index = {}
        for j, h in enumerate(dst_hashes):
            index.setdefault(int(h), []).append(j)
        for i, h in enumerate(src_hashes):
            cand = index.get(int(h))
            if not cand:
                continue
            free = [j for j in cand if j not in used]
            if free:
                mapping[i] = free[0]
                used.add(free[0])
                exact[i] = True

    # remaining bones: greedy nearest position among unused targets
    rest = [i for i in range(len(src)) if mapping[i] < 0]
    pairs = []
    for i in rest:
        d = np.linalg.norm(dst - src[i], axis=1)
        order = np.argsort(d)
        pairs.append((float(order.size and d[order[0]] or 0.0), i, order))
    pairs.sort()
    for _, i, order in pairs:
        for cand in order:
            j = int(cand)
            if j not in used:
                mapping[i] = j
                used.add(j)
                break
    return mapping, exact


def retarget(src: MeshData, dst: MeshData) -> MeshData:
    if src.bindpose is None or dst.bindpose is None:
        raise ValueError("both meshes need bind poses")
    mapping, exact = match_bones(src.bindpose, dst.bindpose,
                                 src.bone_hashes or None, dst.bone_hashes or None)

    bones = np.asarray(src.arrays[13], dtype=np.int64).copy()
    weights = np.asarray(src.arrays[12], dtype=np.float32).copy()
    remapped = mapping[bones]

    # merge duplicate influences, keep the 4 strongest, renormalise
    out_b = np.zeros((src.vertex_count, 4), dtype=np.uint32)
    out_w = np.zeros((src.vertex_count, 4), dtype=np.float32)
    for v in range(src.vertex_count):
        acc = {}
        for k in range(4):
            w = float(weights[v, k])
            if w <= 0.0:
                continue
            b = int(remapped[v, k])
            acc[b] = acc.get(b, 0.0) + w
        items = sorted(acc.items(), key=lambda kv: -kv[1])[:4]
        total = sum(w for _, w in items) or 1.0
        for k, (b, w) in enumerate(items):
            out_b[v, k] = b
            out_w[v, k] = w / total

    arrays = dict(src.arrays)
    arrays[13] = out_b
    arrays[12] = out_w

    return MeshData(
        name=dst.name,
        vertex_count=src.vertex_count,
        channels=dst.channels,
        arrays=arrays,
        indices=src.indices,
        index_format=dst.index_format,
        submeshes=list(src.submeshes),
        bindpose=dst.bindpose,
        bone_hashes=list(dst.bone_hashes),
        aabb=src.aabb,
        raw_data=b"",
    )
