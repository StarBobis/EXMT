"""Minimal BC1/DXT1 encoder (with mip chain) for replacing GF2 diffuse textures.

The game's diffuse maps are DXT1 2048² with a full 12-level mip chain
(2,796,216 bytes). External garment textures come as PNG/JPEG at arbitrary
sizes, so they must be resized, mip-mapped and block-compressed to fit.

Quality is "min/max endpoints, 4-colour mode" — fine for a mod, not a reference
encoder.
"""

from __future__ import annotations

import io

import numpy as np


def _to565(c: np.ndarray) -> int:
    r, g, b = int(c[0]), int(c[1]), int(c[2])
    return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)


def _from565(v: int) -> np.ndarray:
    r = (v >> 11) & 0x1F
    g = (v >> 5) & 0x3F
    b = v & 0x1F
    return np.array([(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)], dtype=np.int32)


def _encode_block(block: np.ndarray) -> bytes:
    """block: (4,4,3) uint8 -> 8 bytes."""
    px = block.reshape(-1, 3).astype(np.int32)
    cmin = px.min(axis=0)
    cmax = px.max(axis=0)
    c0 = _to565(cmax)
    c1 = _to565(cmin)
    if c0 == c1:
        return bytes(np.array([c0, c1, 0, 0], dtype="<u2").tobytes())
    if c0 < c1:
        c0, c1 = c1, c0
    p0 = _from565(c0).astype(np.float32)
    p1 = _from565(c1).astype(np.float32)
    palette = np.stack([p0, p1, (2 * p0 + p1) / 3.0, (p0 + 2 * p1) / 3.0])
    d = ((px[:, None, :] - palette[None, :, :]) ** 2).sum(axis=2)
    idx = d.argmin(axis=1).astype(np.uint32)
    bits = 0
    for i, v in enumerate(idx):
        bits |= int(v) << (2 * i)
    return bytes(np.array([c0, c1, bits & 0xFFFF, (bits >> 16) & 0xFFFF], dtype="<u2").tobytes())


def _pad(block: np.ndarray) -> np.ndarray:
    h, w = block.shape[:2]
    if h == 4 and w == 4:
        return block
    out = np.zeros((4, 4, 3), dtype=np.uint8)
    out[:h, :w] = block
    if h < 4:
        out[h:, :w] = block[h - 1:h, :w]
    if w < 4:
        out[:h, w:] = out[:h, w - 1:w]
    if h < 4 and w < 4:
        out[h:, w:] = block[h - 1, w - 1]
    return out


def encode_dxt1(rgb: np.ndarray) -> bytes:
    """rgb: (H,W,3) uint8, H and W are multiples of 4 (or padded here)."""
    h, w = rgb.shape[:2]
    ph = (4 - h % 4) % 4
    pw = (4 - w % 4) % 4
    if ph or pw:
        rgb = np.pad(rgb, ((0, ph), (0, pw), (0, 0)), mode="edge")
        h, w = rgb.shape[:2]
    out = bytearray()
    for by in range(0, h, 4):
        for bx in range(0, w, 4):
            out += _encode_block(rgb[by:by + 4, bx:bx + 4])
    return bytes(out)


def _downsample(rgb: np.ndarray) -> np.ndarray:
    h, w = rgb.shape[:2]
    if h <= 1 and w <= 1:
        return rgb
    h2, w2 = max(1, h // 2), max(1, w // 2)
    src = rgb[: h2 * 2, : w2 * 2].astype(np.uint16)
    out = src.reshape(h2, 2, w2, 2, 3).mean(axis=(1, 3))
    return np.clip(np.round(out), 0, 255).astype(np.uint8)


def encode_with_mips(rgb: np.ndarray, levels: int) -> bytes:
    out = bytearray()
    cur = rgb
    for i in range(levels):
        out += encode_dxt1(np.ascontiguousarray(cur))
        if i != levels - 1:
            cur = _downsample(cur)
    return bytes(out)


def load_image(path: str) -> np.ndarray:
    from PIL import Image
    im = Image.open(path).convert("RGB")
    return np.asarray(im, dtype=np.uint8)


def encode_file(path: str, size: int = 2048, levels: int = 12) -> bytes:
    from PIL import Image
    im = Image.open(path).convert("RGB").resize((size, size), Image.LANCZOS)
    return encode_with_mips(np.asarray(im, dtype=np.uint8), levels)
