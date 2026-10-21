"""UnityFS container read/write for GF2 bundles.

Format facts (verified on 1797 hot-update bundles):
  * header is big-endian, variable-length cstrings, then aligned to 16 bytes;
  * blocksInfo is LZ4-compressed, and the per-block table uses
    (uncompressedSize, compressedSize, flags) big-endian;
  * data blocks are 128 KiB uncompressed, LZ4 (flags 2/3) or stored (flags 0);
  * only the first 32 KiB of the file is XOR-encrypted; everything after that is
    plaintext.  Encrypting/decrypting the whole file is a classic trap: it leaves
    the header valid but turns every data block into garbage.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field

from . import lz4b

ENC_PREFIX = 0x8000
BLOCK_SIZE = 0x20000


# --------------------------------------------------------------------------- #
# encryption (NEP2: periodic 16-byte XOR, header-derived)
# --------------------------------------------------------------------------- #
def keystream(head4: bytes) -> bytes:
    x = bytes(a ^ b for a, b in zip(head4, b"Unit"))
    pad = b"\xff" * 4 if x[3] & 0x80 else b"\x00" * 4
    x2 = bytes([x[0] ^ 0x02]) + x[1:]
    return x + pad + x2 + pad


def decrypt(raw: bytes) -> bytes:
    ks = keystream(raw[:4])
    out = bytearray(raw)
    for i in range(min(ENC_PREFIX, len(raw))):
        out[i] ^= ks[i % 16]
    return bytes(out)


def encrypt(plain: bytes, head4: bytes) -> bytes:
    ks = keystream(head4)
    out = bytearray(plain)
    for i in range(min(ENC_PREFIX, len(plain))):
        out[i] ^= ks[i % 16]
    return bytes(out)


# --------------------------------------------------------------------------- #
# LZ4 helpers (Unity blocks are plain LZ4 block streams)
# --------------------------------------------------------------------------- #
def lz4_dec(src: bytes, expected: int):
    """Decode one block, returning None on malformed input.

    The callers treat a None result as "this block is not LZ4 or is corrupt",
    which is how a wrong decryption key is distinguished from a wrong block
    table, so decode errors must not escape here.
    """
    try:
        return lz4b.decompress(src, expected)
    except (lz4b.Error, TypeError):
        return None


def lz4_comp(data: bytes) -> bytes:
    return lz4b.compress(data)


# --------------------------------------------------------------------------- #
# container model
# --------------------------------------------------------------------------- #
@dataclass
class Node:
    path: str
    data: bytes
    flags: int = 0  # 4 = SerializedFile, 0 = .resS


@dataclass
class BundleInfo:
    version: int
    engine: str
    revision: str
    size: int
    flags: int
    cbis: int
    ubis: int
    bi_off: int
    data_off: int
    blocks: list = field(default_factory=list)   # (u, c, flags)
    nodes: list = field(default_factory=list)    # (offset, size, flags, path)


def parse(plain: bytes) -> BundleInfo:
    if plain[:8] != b"UnityFS\0":
        raise ValueError("not a UnityFS bundle")
    pos = 8
    version = struct.unpack_from(">I", plain, pos)[0]
    pos += 4
    e = plain.index(b"\0", pos)
    engine = plain[pos:e].decode("latin1")
    pos = e + 1
    e = plain.index(b"\0", pos)
    revision = plain[pos:e].decode("latin1")
    pos = e + 1
    size = struct.unpack_from(">q", plain, pos)[0]
    pos += 8
    cbis, ubis, flags = struct.unpack_from(">III", plain, pos)
    pos += 12
    bi_off = (pos + 15) & ~15
    bi = lz4_dec(plain[bi_off:bi_off + cbis], ubis)
    if bi is None:
        raise ValueError("blocksInfo decompression failed")
    q = 16
    bc = struct.unpack_from(">i", bi, q)[0]
    q += 4
    blocks = [struct.unpack_from(">IIH", bi, q + 10 * i) for i in range(bc)]
    q += 10 * bc
    nc = struct.unpack_from(">i", bi, q)[0]
    q += 4
    nodes = []
    for _ in range(nc):
        off, sz, fl = struct.unpack_from(">qqI", bi, q)
        q += 20
        z = bi.index(b"\0", q)
        nodes.append((off, sz, fl, bi[q:z].decode("latin1")))
        q = z + 1
    return BundleInfo(version=version, engine=engine, revision=revision, size=size,
                      flags=flags, cbis=cbis, ubis=ubis, bi_off=bi_off,
                      data_off=bi_off + cbis, blocks=blocks, nodes=nodes)


def read_stream(plain: bytes, info: BundleInfo | None = None) -> bytes:
    """Decompress every data block and concatenate them."""
    info = info or parse(plain)
    pos = info.data_off
    out = bytearray()
    for (u, c, f) in info.blocks:
        chunk = plain[pos:pos + c]
        pos += c
        if f & 0x3F in (2, 3):
            d = lz4_dec(chunk, u)
            if d is None:
                raise ValueError("block decompression failed at %d" % pos)
            out += d
        else:
            out += chunk
    return bytes(out)


def build(nodes: list[Node], template: BundleInfo, compress: bool = True) -> bytes:
    """Rebuild a bundle from node payloads, reusing header fields of `template`."""
    stream = b"".join(n.data for n in nodes)
    blocks = []
    chunks = []
    for i in range(0, len(stream), BLOCK_SIZE):
        raw = stream[i:i + BLOCK_SIZE]
        if compress and len(raw) > 32:
            comp = lz4_comp(raw)
            if len(comp) < len(raw):
                blocks.append((len(raw), len(comp), 3))
                chunks.append(comp)
                continue
        blocks.append((len(raw), len(raw), 0))
        chunks.append(raw)

    offsets = []
    off = 0
    for n in nodes:
        offsets.append(off)
        off += len(n.data)

    bi = bytearray(16)
    bi += struct.pack(">i", len(blocks))
    for u, c, f in blocks:
        bi += struct.pack(">IIH", u, c, f)
    bi += struct.pack(">i", len(nodes))
    for (n, o) in zip(nodes, offsets):
        bi += struct.pack(">qqI", o, len(n.data), n.flags) + n.path.encode("latin1") + b"\0"
    bi = bytes(bi)
    bic = lz4_comp(bi)

    body = bic + b"".join(chunks)
    total = 64 + len(body)
    head = (b"UnityFS\0" + struct.pack(">I", template.version) +
            template.engine.encode("latin1") + b"\0" +
            template.revision.encode("latin1") + b"\0" +
            struct.pack(">q", total) +
            struct.pack(">III", len(bic), len(bi), template.flags))
    if len(head) % 16:
        head += b"\0" * (16 - len(head) % 16)
    out = head + body
    if len(out) != total:
        raise AssertionError("bundle size mismatch")
    return out


def nodes_of(plain: bytes, info: BundleInfo | None = None) -> list[Node]:
    """Split the decompressed stream into the bundle's nodes."""
    info = info or parse(plain)
    stream = read_stream(plain, info)
    out = []
    for (off, size, flags, path) in info.nodes:
        out.append(Node(path=path, data=stream[off:off + size], flags=flags))
    return out
