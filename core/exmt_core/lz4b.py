"""Self-contained LZ4 block codec (no third-party `lz4` package).

The UnityFS container stores each data block as a bare LZ4 block stream, and the
blocksInfo directory is compressed the same way. Unity never uses the LZ4 frame
format here: there is no header, no checksum and no content size, just the
sequence stream described by the LZ4 specification section "Block format".

A block is a sequence of runs. Each run is::

    token                      1 byte: (literal_len << 4) | (match_len - 4)
    [literal length extension] while the 4-bit field is 15, read bytes, add,
                               stop at a byte < 255
    literals                   literal_len raw bytes
    [offset]                   2 bytes little-endian, omitted for the final run
    [match length extension]   same 15/255 scheme, only if the low nibble is 15
    match                      match_len bytes copied from `output[-offset:]`

The match copy may overlap the bytes it produces (offset can be smaller than
match_len), which is how LZ4 encodes runs of a single repeated byte. Copying
byte-by-byte is therefore required, not merely safe.

Decoder notes:
  * `expected_size` is only a check. Unity always knows the uncompressed size
    from the block table, so a mismatch there means the stream or the table is
    wrong -- raise instead of returning plausible-looking garbage.
"""

from __future__ import annotations

__all__ = ["compress", "decompress", "Error"]

# LZ4 block-format limit: a match offset is a 16-bit field.
_MAX_OFFSET = 65535

# End-of-block rules from the specification. A match may not start within the
# last MFLIMIT bytes, and the final literals run must be at least LASTLITERALS
# long. Encoders that violate these still round-trip through lenient decoders
# but are rejected by conforming ones, so emit conforming blocks.
_MFLIMIT = 12
_LASTLITERALS = 5

_MINMATCH = 4
_HASH_LOG = 16
_HASH_SIZE = 1 << _HASH_LOG


class Error(ValueError):
    """Raised for malformed or unsupported LZ4 block input."""


def _read_extended_length(src: bytes, pos: int, base: int) -> tuple[int, int]:
    """Read the 255-terminated length extension used by literal/match runs."""
    if base != 15:
        return base, pos
    total = base
    n = len(src)
    while True:
        if pos >= n:
            raise Error("truncated length extension")
        byte = src[pos]
        pos += 1
        total += byte
        if byte != 255:
            return total, pos


def decompress(src: bytes, expected_size: int | None = None) -> bytes:
    """Decode one LZ4 block.

    Args:
        src: bare LZ4 block bytes (no frame header).
        expected_size: uncompressed size to enforce, when the caller knows it
            (Unity's block table always does). Omitted means "trust the stream".

    Raises:
        Error: malformed stream, bad offset, or a size mismatch.
    """
    if not isinstance(src, (bytes, bytearray, memoryview)):
        raise TypeError("src must be bytes-like")
    src = bytes(src)
    n = len(src)

    out = bytearray()
    pos = 0

    while pos < n:
        token = src[pos]
        pos += 1

        literal_len, pos = _read_extended_length(src, pos, token >> 4)
        if literal_len:
            end = pos + literal_len
            if end > n:
                raise Error("literal run past end of block")
            out += src[pos:end]
            pos = end

        # A block may end right after a literal run; that final run carries no
        # offset/match, which is the only way a sequence terminates early.
        if pos >= n:
            break

        if pos + 2 > n:
            raise Error("truncated match offset")
        offset = src[pos] | (src[pos + 1] << 8)
        pos += 2
        if offset == 0:
            raise Error("match offset of zero")
        if offset > len(out):
            raise Error(
                "match offset %d exceeds %d bytes of output" % (offset, len(out))
            )

        match_len, pos = _read_extended_length(src, pos, token & 0x0F)
        match_len += _MINMATCH

        start = len(out) - offset
        if match_len <= offset:
            # Non-overlapping: a single slice copy is exact.
            out += out[start:start + match_len]
        else:
            # Overlapping: the source is being produced as we copy, so repeat
            # the `offset`-byte window until the match is filled.
            window = bytes(out[start:])
            reps, tail = divmod(match_len, offset)
            out += window * reps + window[:tail]

    if expected_size is not None and len(out) != expected_size:
        raise Error(
            "decoded %d bytes, expected %d" % (len(out), expected_size)
        )
    return bytes(out)


def _emit_length(dst: bytearray, value: int) -> None:
    """Write the 255-terminated extension bytes for `value` (>= 15)."""
    remaining = value - 15
    while remaining >= 255:
        dst.append(255)
        remaining -= 255
    dst.append(remaining)


def _common_prefix(src: bytes, a: int, b: int, limit: int) -> int:
    """Length of the common prefix of src[a:] and src[b:], capped at `limit`.

    Compares whole slices rather than byte by byte: Python loop overhead, not
    the comparison itself, dominates on 64 KiB asset blocks.
    """
    if limit <= 0:
        return 0
    if src[a:a + limit] == src[b:b + limit]:
        return limit
    # Different somewhere: narrow it down with a coarse then fine scan. Probe in
    # growing strides first because short matches (the common case) exit early.
    lo = 0
    step = 8
    while step < limit:
        if src[a + lo:a + lo + step] != src[b + lo:b + lo + step]:
            break
        lo += step
        step <<= 1
    hi = min(limit, lo + step)
    while lo < hi:
        mid = (lo + hi + 1) >> 1
        if src[a:a + mid] == src[b:b + mid]:
            lo = mid
        else:
            hi = mid - 1
    return lo


def compress(src: bytes) -> bytes:
    """Encode `src` as one conforming LZ4 block.

    Greedy single-candidate encoder in the shape of LZ4's fast mode: a fixed
    hash table, one remembered position per slot, and a skip past each match.

    This deliberately does not try to match LZ4HC. Measured on GF2 asset blocks,
    a short-chain search bought 0.3% of size for 73% more time, and the game's
    own blocks stay ~5% smaller either way; keeping one simple search is worth
    more than that margin.
    """
    if not isinstance(src, (bytes, bytearray, memoryview)):
        raise TypeError("src must be bytes-like")
    src = bytes(src)
    n = len(src)
    if n == 0:
        return b""

    dst = bytearray()
    # Slot holds (position + 1); 0 means empty. One flat list beats a dict here
    # because every position does exactly one lookup and one store.
    table = [0] * _HASH_SIZE

    anchor = 0
    pos = 0
    limit = n - _MFLIMIT
    hi_shift = 32 - _HASH_LOG
    # Sliding 32-bit window over the current 4 bytes. Recomputing this with four
    # index operations per position costs more than the match search itself, so
    # it is maintained incrementally instead. Short inputs never enter the loop,
    # so the window must tolerate n < 4 here.
    window = 0
    if n >= _MINMATCH:
        window = src[0] | (src[1] << 8) | (src[2] << 16) | (src[3] << 24)

    while pos < limit:
        h = ((window * 2654435761) & 0xFFFFFFFF) >> hi_shift
        entry = table[h]
        table[h] = pos + 1

        match = -1
        if entry:
            cand = entry - 1
            if pos - cand <= _MAX_OFFSET and src[cand:cand + _MINMATCH] == src[pos:pos + _MINMATCH]:
                match = cand
        if match < 0:
            pos += 1
            if pos < limit:
                window = ((window >> 8) | (src[pos + 3] << 24)) & 0xFFFFFFFF
            continue

        max_len = n - _LASTLITERALS - pos
        if max_len < _MINMATCH:
            break
        match_len = _common_prefix(src, match, pos, max_len)

        literal_len = pos - anchor
        code = ((literal_len if literal_len < 15 else 15) << 4) | (
            (match_len - _MINMATCH) if (match_len - _MINMATCH) < 15 else 15
        )
        dst.append(code)
        if literal_len >= 15:
            _emit_length(dst, literal_len)
        dst += src[anchor:pos]
        offset = pos - match
        dst.append(offset & 0xFF)
        dst.append(offset >> 8)
        if match_len - _MINMATCH >= 15:
            _emit_length(dst, match_len - _MINMATCH)

        # Register a few positions inside the match so later searches can refer
        # back into it, then skip the rest. Sampling instead of inserting every
        # byte is what keeps this linear rather than quadratic.
        end = pos + match_len
        step = 1 if match_len <= 32 else 8
        for k in range(pos + 1, min(end, limit), step):
            table[((src[k] | (src[k+1] << 8) | (src[k+2] << 16) | (src[k+3] << 24)) * 2654435761 & 0xFFFFFFFF) >> hi_shift] = k + 1

        pos = end
        anchor = pos
        if pos < limit:
            window = (src[pos] | (src[pos + 1] << 8) | (src[pos + 2] << 16) | (src[pos + 3] << 24)) & 0xFFFFFFFF

    literal_len = n - anchor
    dst.append(min(literal_len, 15) << 4)
    if literal_len >= 15:
        _emit_length(dst, literal_len)
    dst += src[anchor:n]

    return bytes(dst)


def roundtrip_ok(data: bytes) -> bool:
    """True when `data` survives compress/decompress unchanged."""
    return decompress(compress(data), len(data)) == data
