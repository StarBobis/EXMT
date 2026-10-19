"""Replace a Texture2D's pixels with blocks encoded by `texconv`.

The game ships **block-compressed** textures (measured on a real skin: 3 × BC1 for
diffuse, 9 × BC7 for normal / masks, 2048², 12 mips, every payload streamed into
the bundle's `.resS` node).  A Mod therefore cannot just drop a PNG in: the asset
stores raw blocks, the format number, the size and the mip count, and the sampler
reads all of them.

Encoding is delegated to Microsoft's `texconv.exe` (bundled with the Blender
add-on under `bin/`). It is the reference encoder for DirectXTex, so a BC7 target
really gets BC7 — writing our own BC7 mode-6 compressor was tried and dropped
because "correct" is not something a hand-rolled packer can promise.

This module owns the bundle half (`info`, `apply`); the format/DDS helpers live in
`ddsinfo` so the add-on can use them without UnityPy.
"""

from __future__ import annotations

from .ddsinfo import (  # noqa: F401 - re-exported for callers
    FORMATS,
    expected_bytes,
    mip_sizes,
    parse_dds,
    vertical_flip,
)


def _tree_payload(tree) -> bytes:
    """The raw image bytes of a Texture2D typetree.

    UnityPy names this field **`image data`** (with a space) — `m_ImageData` or
    `image_data` are silently accepted as new keys and dropped on save, which
    looks like "the patch did nothing".
    """
    for key in ("image data", "m_ImageData", "image_data"):
        value = tree.get(key)
        if isinstance(value, (bytes, bytearray)):
            return bytes(value)
    return b""


def info(bundle_path: str, name: str) -> dict:
    """What the asset expects: size, mip count, Unity format, payload location."""
    for row in list_textures(bundle_path):
        if row["name"] == name:
            return row
    raise ValueError("texture not found in bundle: %s" % name)


def list_textures(bundle_path: str) -> list:
    """Every Texture2D of a bundle with what it expects (one bundle open)."""
    from .sf import open_bundle

    b = open_bundle(bundle_path)
    out = []
    for obj in b.objects():
        if obj.type.name != "Texture2D":
            continue
        tree = obj.read_typetree()
        fmt = int(tree.get("m_TextureFormat") or 0)
        width = int(tree.get("m_Width") or 0)
        height = int(tree.get("m_Height") or 0)
        mips = int(tree.get("m_MipCount") or 1)
        row = {
            "name": tree.get("m_Name") or "",
            "width": width,
            "height": height,
            "mips": mips,
            "format": fmt,
            "format_name": FORMATS[fmt][0] if fmt in FORMATS else "",
            "supported": fmt in FORMATS,
            "streamed": bool((tree.get("m_StreamData") or {}).get("path")),
            "complete_image_size": int(tree.get("m_CompleteImageSize") or 0),
            "inline_bytes": len(_tree_payload(tree)),
            "readable": bool(tree.get("m_IsReadable")),
        }
        if row["supported"]:
            row["expected_bytes"] = expected_bytes(fmt, width, height, mips)
        out.append(row)
    return out


def apply(bundle, name: str, payload: bytes, meta: dict) -> dict:
    """Write pre-encoded blocks into the named Texture2D.

    The pixels live in the typetree under `image data`; a streamed asset keeps
    them in the bundle's `.resS` node and only carries an offset/size. Replacing
    them means inlining the new blocks and clearing `m_StreamData`, so the asset
    no longer points into the stream.
    """
    for obj in bundle.objects():
        if obj.type.name != "Texture2D":
            continue
        tree = obj.read_typetree()
        if tree.get("m_Name") != name:
            continue
        fmt = int(tree.get("m_TextureFormat") or 0)
        width = int(tree.get("m_Width") or 0)
        height = int(tree.get("m_Height") or 0)
        mips = int(tree.get("m_MipCount") or 1)
        want = expected_bytes(fmt, width, height, mips)
        if len(payload) != want:
            raise ValueError(
                "texture %s: payload is %d bytes but the asset expects %d "
                "(%dx%d, %d mips, format %s)" % (name, len(payload), want, width, height, mips, fmt))
        if int(meta.get("format") or fmt) != fmt:
            raise ValueError("texture %s: encoded as %s but the asset is format %s"
                             % (name, meta.get("format"), fmt))
        tree["image data"] = vertical_flip(payload, fmt, width, height, mips)
        tree["m_CompleteImageSize"] = len(payload)
        tree["m_MipCount"] = mips
        tree["m_StreamData"] = {"path": "", "offset": 0, "size": 0}
        obj.save_typetree(tree)
        return {"name": name, "bytes": len(payload), "format": fmt,
                "width": width, "height": height, "mips": mips}
    raise ValueError("texture not found in bundle: %s" % name)
