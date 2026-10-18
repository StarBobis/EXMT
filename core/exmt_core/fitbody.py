"""Fit a stock SFW base body onto a character mesh slot.

The shipped base body (`data/basebody.exmtmesh`) is stored in Unity space with
its feet at y=0. Fitting then only needs: scale by height, centre in X/Z, drop
the head (the character's own head renders on top) and transfer weights from the
target mesh by nearest-vertex lookup.
"""

from __future__ import annotations

import os

import numpy as np

from . import interchange
from .mesh import MeshData

DATA = os.path.join(os.path.dirname(__file__), "data", "basebody.exmtmesh")


def _nearest_weights(pos: np.ndarray, target: MeshData, k: int = 4) -> tuple:
    tpos = np.asarray(target.arrays[0], dtype=np.float64)
    tb = np.asarray(target.arrays[13], dtype=np.int64)
    tw = np.asarray(target.arrays[12], dtype=np.float64)
    bones = np.zeros((len(pos), 4), dtype=np.uint32)
    weights = np.zeros((len(pos), 4), dtype=np.float32)
    chunk = 4096
    for start in range(0, len(pos), chunk):
        part = pos[start:start + chunk]
        d = ((part[:, None, :] - tpos[None, :, :]) ** 2).sum(axis=2)
        nearest = np.argpartition(d, k, axis=1)[:, :k]
        for row, cols in enumerate(nearest):
            dist = np.sqrt(d[row, cols])
            w = 1.0 / np.maximum(dist, 1e-4)
            w /= w.sum()
            acc: dict[int, float] = {}
            for i, ci in enumerate(cols):
                for slot in range(4):
                    bw = tw[ci, slot]
                    if bw <= 0:
                        continue
                    b = int(tb[ci, slot])
                    acc[b] = acc.get(b, 0.0) + bw * w[i]
            items = sorted(acc.items(), key=lambda kv: -kv[1])[:4]
            total = sum(v for _, v in items) or 1.0
            for i, (b, v) in enumerate(items):
                bones[start + row, i] = b
                weights[start + row, i] = v / total
    return bones, weights


def fit_base_body(target: MeshData, cut_head: bool = True) -> MeshData:
    """Build a base-body mesh that can replace `target` (same skeleton)."""
    base, _ = interchange.load(DATA)
    pos = np.asarray(base.arrays[0], dtype=np.float64).copy()
    nrm = np.asarray(base.arrays[1], dtype=np.float64).copy()
    uv = np.asarray(base.arrays[4], dtype=np.float32)
    idx = np.asarray(base.indices, dtype=np.uint32).reshape(-1, 3)

    tpos = np.asarray(target.arrays[0], dtype=np.float64)
    tlo, thi = tpos.min(axis=0), tpos.max(axis=0)

    lo, hi = pos.min(axis=0), pos.max(axis=0)
    scale = (thi[1] - tlo[1]) / max(hi[1] - lo[1], 1e-6)
    pos *= scale
    lo, hi = pos.min(axis=0), pos.max(axis=0)
    pos[:, 0] += (tlo[0] + thi[0]) / 2 - (lo[0] + hi[0]) / 2
    pos[:, 2] += (tlo[2] + thi[2]) / 2 - (lo[2] + hi[2]) / 2
    pos[:, 1] += tlo[1] - lo[1]

    lo, hi = pos.min(axis=0), pos.max(axis=0)
    if cut_head:
        neck = hi[1] * 0.855
        keep = np.all(pos[idx][:, :, 1] <= neck, axis=1)
        idx = idx[keep]

    bones, weights = _nearest_weights(pos, target)

    md = MeshData(
        name=target.name,
        vertex_count=len(pos),
        channels=target.channels,
        arrays={
            0: pos.astype(np.float32),
            1: nrm.astype(np.float32),
            2: np.tile(np.array([[1.0, 0.0, 0.0, 1.0]], dtype=np.float32), (len(pos), 1)),
            3: np.ones((len(pos), 4), dtype=np.float32),
            4: uv,
            12: weights,
            13: bones,
        },
        indices=idx.reshape(-1).astype(np.uint32),
        index_format=target.index_format,
        submeshes=[{"firstByte": 0, "indexCount": len(idx) * 3}],
        bindpose=target.bindpose,
        bone_hashes=target.bone_hashes,
        aabb={},
    )
    interchange.rebuild_submeshes(md, np.array([0], dtype=np.uint32),
                                  np.array([len(idx) * 3], dtype=np.uint32))
    return md
