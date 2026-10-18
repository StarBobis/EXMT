"""Decode Unity Texture2D payloads (BC1/BC3/BC7) to PIL images."""

from __future__ import annotations

import io

from .sf import Bundle, texture_info, texture_payload

FORMATS = {
    10: ("bc1", 1),   # DXT1
    12: ("bc3", 1),   # DXT5
    25: ("bc7", 1),   # BC7
    17: ("rgba_half", 8),
    4: ("rgba32", 4),
}


def decode_texture(bundle_path: str, name: str):
    """Return a PIL.Image for the named texture (mip 0, upright)."""
    import numpy as np
    import texture2ddecoder
    from PIL import Image

    b = Bundle(bundle_path)
    info = texture_info(b, name)
    payload = texture_payload(b, info)
    fmt = int(info["format"])
    w, h = int(info["width"]), int(info["height"])
    if fmt not in FORMATS:
        raise ValueError("unsupported texture format %s for %s" % (fmt, name))
    kind, _ = FORMATS[fmt]

    if kind == "bc1":
        raw = texture2ddecoder.decode_bc1(payload, w, h)
    elif kind == "bc3":
        raw = texture2ddecoder.decode_bc3(payload, w, h)
    elif kind == "bc7":
        raw = texture2ddecoder.decode_bc7(payload, w, h)
    elif kind == "rgba32":
        raw = payload[: w * h * 4]
    else:
        raise ValueError("decoder for %s not implemented" % kind)

    arr = np.frombuffer(raw, dtype=np.uint8).reshape(h, w, 4)
    # decoders return BGRA, and Unity stores textures bottom-up
    arr = arr[::-1, :, [2, 1, 0, 3]]
    return Image.fromarray(arr, "RGBA")


def save_png(bundle_path: str, name: str, out_path: str, size: int | None = None) -> dict:
    from PIL import Image

    img = decode_texture(bundle_path, name)
    if size:
        img = img.resize((size, size), Image.LANCZOS)
    if img.mode == "RGBA":
        bg = Image.new("RGBA", img.size, (0, 0, 0, 0))
        bg.alpha_composite(img)
        img = bg
    img.save(out_path)
    return {"file": out_path, "size": img.size, "texture": name}
