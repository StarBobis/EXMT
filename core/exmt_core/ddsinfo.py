"""DDS / texture-format helpers with **no dependencies**.

The Blender add-on's Python has numpy but no UnityPy and no lz4, so it cannot
import the rest of `exmt_core` (whose package `__init__` pulls in `bundle` → lz4).
This module is deliberately standalone: `struct` only. The add-on loads it by
path to read what `texconv` wrote; the core imports it normally.
"""

from __future__ import annotations

import struct

# Unity TextureFormat → (texconv format name, size, kind)
#   kind "block": 4×4 block compression, `size` = bytes per block
#   kind "pixel": uncompressed, `size` = bytes per pixel
FORMATS = {
    1: ("R8_UNORM", 1, "pixel"),            # Alpha8
    3: ("B8G8R8X8_UNORM", 4, "pixel"),      # RGB24
    4: ("R8G8B8A8_UNORM", 4, "pixel"),      # RGBA32
    5: ("B8G8R8A8_UNORM", 4, "pixel"),      # ARGB32
    10: ("BC1_UNORM", 8, "block"),          # DXT1
    12: ("BC3_UNORM", 16, "block"),         # DXT5
    17: ("R16G16B16A16_FLOAT", 8, "pixel"),  # RGBAHalf
    20: ("R32G32B32A32_FLOAT", 16, "pixel"),  # RGBAFloat
    24: ("BC6H_UF16", 16, "block"),
    25: ("BC7_UNORM", 16, "block"),
    26: ("BC4_UNORM", 8, "block"),
    27: ("BC5_UNORM", 16, "block"),
    63: ("R8_UNORM", 1, "pixel"),           # R8
}

# DXGI format → Unity TextureFormat, for sanity-checking what texconv produced
DXGI_TO_UNITY = {
    2: 20,    # R32G32B32A32_FLOAT
    10: 17,   # R16G16B16A16_FLOAT
    28: 4,    # R8G8B8A8_UNORM
    49: 3,    # B8G8R8X8_UNORM
    61: 63,   # R8_UNORM
    71: 10,   # BC1_UNORM
    77: 12,   # BC3_UNORM
    80: 26,   # BC4_UNORM
    83: 27,   # BC5_UNORM
    87: 5,    # B8G8R8A8_UNORM
    95: 24,   # BC6H_UF16
    98: 25,   # BC7_UNORM
}

DDS_MAGIC = b"DDS "
DDS_HEADER = 128
DDS_DX10 = 20


def mip_sizes(width: int, height: int, count: int) -> list:
    return [(max(1, width >> i), max(1, height >> i)) for i in range(max(1, count))]


def expected_bytes(fmt: int, width: int, height: int, mips: int) -> int:
    """How many bytes Unity expects for this format/size/mip count."""
    if fmt not in FORMATS:
        raise ValueError("unsupported texture format %s" % fmt)
    _name, size, kind = FORMATS[fmt]
    total = 0
    for w, h in mip_sizes(width, height, mips):
        if kind == "pixel":
            total += w * h * size
        else:
            total += ((w + 3) // 4) * ((h + 3) // 4) * size
    return total


def parse_dds(path: str) -> dict:
    """`{payload, width, height, mips, unity_format, dxgi_format}` from a texconv DDS."""
    with open(path, "rb") as fh:
        data = fh.read()
    if data[:4] != DDS_MAGIC:
        raise ValueError("texconv did not write a DDS: %s" % path)
    header_size = struct.unpack("<I", data[4:8])[0]
    if header_size != 124:
        raise ValueError("unexpected DDS header size %s" % header_size)
    height, width = struct.unpack("<II", data[12:20])
    mips = struct.unpack("<I", data[28:32])[0] or 1
    fourcc = data[84:88]
    offset = DDS_HEADER
    dxgi = None
    if fourcc == b"DX10":
        dxgi = struct.unpack("<I", data[DDS_HEADER:DDS_HEADER + 4])[0]
        offset += DDS_DX10
        unity = DXGI_TO_UNITY.get(dxgi)
    else:
        legacy = {b"DXT1": 10, b"DXT5": 12, b"ATI1": 26, b"BC5U": 27}
        unity = legacy.get(fourcc)
    return {
        "payload": data[offset:],
        "width": width,
        "height": height,
        "mips": mips,
        "unity_format": unity,
        "dxgi_format": dxgi,
        "header_bytes": offset,
    }


def vertical_flip(payload: bytes, fmt: int, width: int, height: int, mips: int) -> bytes:
    """Flip every mip level vertically, block-row by block-row.

    Unity stores Texture2D rows **bottom-up** (that is why the preview flips a
    decoded texture to look upright), while `texconv` emits the DDS top-down for
    a PNG input. Measured on a real 2048² BC7 normal map: without this flip the
    decoded result is the mirror of the replacement (10.1 dB), with it 59.2 dB.
    """
    _name, size, kind = FORMATS[fmt]
    out = bytearray()
    offset = 0
    for w, h in mip_sizes(width, height, mips):
        if kind == "pixel":
            row = w * size
            level = payload[offset:offset + row * h]
            out += b"".join(level[i * row:(i + 1) * row] for i in reversed(range(h)))
            offset += row * h
        else:
            bw, bh = (w + 3) // 4, (h + 3) // 4
            row = bw * size
            level = payload[offset:offset + row * bh]
            out += b"".join(level[i * row:(i + 1) * row] for i in reversed(range(bh)))
            offset += row * bh
    if offset != len(payload):
        raise ValueError("payload is %d bytes, mip chain accounts for %d"
                         % (len(payload), offset))
    return bytes(out)
