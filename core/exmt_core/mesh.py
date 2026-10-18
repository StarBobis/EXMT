"""Mesh (class 43) payload <-> numpy arrays.

The mesh lives in the SerializedFile's object data; vertex attributes are stored
as up to 4 independent streams whose layout is described by `m_Channels`.
GF2's character meshes use three streams:

    stream 0: position f32x3, normal f32x3, tangent f32x4      (stride 40)
    stream 1: color    f32x4, uv0    f16x2                     (stride 20)
    stream 2: weights  f32x4, bones  u32x4                     (stride 32)

Each stream is padded to 16 bytes inside `m_DataSize`.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

# VertexAttribute enum order (index == channel slot)
ATTRS = {
    0: ("position", 3),
    1: ("normal", 3),
    2: ("tangent", 4),
    3: ("color", 4),
    4: ("uv0", 2),
    5: ("uv1", 2),
    6: ("uv2", 2),
    7: ("uv3", 2),
    8: ("uv4", 2),
    9: ("uv5", 2),
    10: ("uv6", 2),
    11: ("uv7", 2),
    12: ("weights", 4),
    13: ("bones", 4),
}

# VertexAttributeFormat -> (numpy dtype, bytes, normaliser)
FORMATS = {
    0: ("<f4", 4, 1.0),
    1: ("<f2", 2, 1.0),
    2: ("u1", 1, 255.0),
    3: ("i1", 1, 127.0),
    4: ("<u2", 2, 65535.0),
    5: ("<i2", 2, 32767.0),
    6: ("u1", 1, 1.0),
    7: ("i1", 1, 1.0),
    8: ("<u2", 2, 1.0),
    9: ("<i2", 2, 1.0),
    10: ("<u4", 4, 1.0),
    11: ("<i4", 4, 1.0),
}


def _align16(n: int) -> int:
    return (n + 15) & ~15


@dataclass
class MeshData:
    name: str
    vertex_count: int
    channels: list                      # 14 x dict(stream, offset, format, dimension)
    arrays: dict                        # attr index -> (N, dim) float/int numpy array
    indices: np.ndarray                 # raw index values (uint16/uint32)
    index_format: int                   # 0 = UInt16, 1 = UInt32
    submeshes: list = field(default_factory=list)
    bindpose: np.ndarray | None = None  # (B, 4, 4) float32
    bone_hashes: list = field(default_factory=list)
    aabb: dict = field(default_factory=dict)
    raw_data: bytes = b""

    # -- convenience ----------------------------------------------------- #
    @property
    def stride(self) -> list:
        out = []
        for attr, (stream, offset, fmt, dim) in enumerate(self.channels):
            if dim:
                need = offset + FORMATS[fmt][1] * dim
                while len(out) <= stream:
                    out.append(0)
                out[stream] = max(out[stream], need)
        return [(_align16(s) if s else 0) for s in out]


def _layout(channels: list) -> list:
    strides = []
    for ch in channels:
        stream = ch["stream"]
        fmt = ch["format"]
        dim = ch["dimension"]
        if not dim:
            continue
        need = ch["offset"] + FORMATS[fmt][1] * dim
        while len(strides) <= stream:
            strides.append(0)
        strides[stream] = max(strides[stream], need)
    return strides


def decode_streams(data: bytes, vertex_count: int, channels: list) -> dict:
    strides = _layout(channels)
    starts = []
    cursor = 0
    for s in strides:
        cursor = _align16(cursor)          # each stream starts 16-byte aligned
        starts.append(cursor)
        cursor += s * vertex_count if s else 0

    out = {}
    for attr, ch in enumerate(channels):
        dim = ch["dimension"]
        if not dim:
            continue
        dt, size, norm = FORMATS[ch["format"]]
        stride = strides[ch["stream"]]
        offset = ch["offset"]
        raw = np.frombuffer(data, dtype=np.uint8, count=stride * vertex_count,
                            offset=starts[ch["stream"]])
        rows = raw.reshape(vertex_count, stride)[:, offset:offset + size * dim]
        vals = np.ascontiguousarray(rows).view(dt).reshape(vertex_count, dim)
        if norm != 1.0:
            vals = vals.astype(np.float32) / norm
        out[attr] = vals
    return out


def encode_streams(arrays: dict, channels: list, vertex_count: int) -> bytes:
    """Pack arrays back into the layout described by `channels`."""
    strides = _layout(channels)
    starts = []
    cursor = 0
    for s in strides:
        cursor = _align16(cursor)
        starts.append(cursor)
        cursor += s * vertex_count if s else 0
    total = cursor
    buf = np.zeros(total, dtype=np.uint8)
    for attr, ch in enumerate(channels):
        dim = ch["dimension"]
        if not dim or attr not in arrays:
            continue
        dt, size, norm = FORMATS[ch["format"]]
        vals = np.asarray(arrays[attr])
        if vals.shape[0] != vertex_count:
            raise ValueError("attribute %d has %d rows, expected %d" %
                             (attr, vals.shape[0], vertex_count))
        if norm != 1.0:
            vals = np.clip(np.round(vals * norm), 0, norm).astype(dt)
        else:
            vals = vals.astype(dt)
        stride = strides[ch["stream"]]
        offset = ch["offset"]
        view = buf[starts[ch["stream"]]:starts[ch["stream"]] + stride * vertex_count].reshape(vertex_count, stride)
        raw = np.ascontiguousarray(vals).view(np.uint8).reshape(vertex_count, size * dim)
        view[:, offset:offset + size * dim] = raw
    return buf.tobytes()


def from_tree(tree: dict) -> MeshData:
    vd = tree["m_VertexData"]
    count = vd["m_VertexCount"]
    channels = [dict(c) for c in vd["m_Channels"]]
    while len(channels) < 14:
        channels.append({"stream": 0, "offset": 0, "format": 0, "dimension": 0})
    data = vd["m_DataSize"]
    arrays = decode_streams(data, count, channels)

    ib = tree["m_IndexBuffer"]
    idx_dtype = np.uint16 if tree["m_IndexFormat"] == 0 else np.uint32
    # m_IndexBuffer is a TypelessData blob: the tree reader hands it back as a
    # list of bytes, so reinterpret it with the declared index width.
    ib_bytes = bytes(ib) if not isinstance(ib, (bytes, bytearray)) else bytes(ib)
    indices = np.frombuffer(ib_bytes, dtype=idx_dtype).copy()

    bind = tree.get("m_BindPose") or []
    mat = None
    if bind:
        mat = np.array(
            [[[m["e%d%d" % (r, c)] for c in range(4)] for r in range(4)] for m in bind],
            dtype=np.float32,
        )

    return MeshData(
        name=tree["m_Name"],
        vertex_count=count,
        channels=channels,
        arrays=arrays,
        indices=indices,
        index_format=tree["m_IndexFormat"],
        submeshes=[dict(s) for s in tree["m_SubMeshes"]],
        bindpose=mat,
        bone_hashes=list(tree.get("m_BoneNameHashes") or []),
        aabb=dict(tree.get("m_LocalAABB") or {}),
        raw_data=data,
    )


def to_tree(tree: dict, mesh: MeshData) -> dict:
    """Write arrays back into an existing typetree dict (mutates and returns it)."""
    count = mesh.vertex_count
    data = encode_streams(mesh.arrays, mesh.channels, count)
    vd = tree["m_VertexData"]
    vd["m_VertexCount"] = count
    vd["m_DataSize"] = data

    ib = mesh.indices.astype(np.uint16 if mesh.index_format == 0 else np.uint32)
    tree["m_IndexBuffer"] = ib.tobytes() if isinstance(tree["m_IndexBuffer"], (bytes, bytearray)) \
        else list(ib.tobytes())
    tree["m_IndexFormat"] = mesh.index_format

    tree["m_SubMeshes"] = mesh.submeshes
    if mesh.aabb:
        tree["m_LocalAABB"] = mesh.aabb
    if "m_StreamData" in tree and isinstance(tree["m_StreamData"], dict):
        # keep meshes inline: zero-length external stream
        tree["m_StreamData"]["path"] = ""
        tree["m_StreamData"]["offset"] = 0
        tree["m_StreamData"]["size"] = 0
    return tree


def aabb_from_vertices(positions: np.ndarray) -> dict:
    lo = positions.min(axis=0)
    hi = positions.max(axis=0)
    center = (lo + hi) * 0.5
    extent = (hi - lo) * 0.5
    return {
        "m_Center": {"x": float(center[0]), "y": float(center[1]), "z": float(center[2])},
        "m_Extent": {"x": float(extent[0]), "y": float(extent[1]), "z": float(extent[2])},
    }
