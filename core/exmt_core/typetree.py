"""Read a SerializedFile object into plain Python values via its type tree.

This replaces the reader-side of a third-party UnityPy dependency: given the
flat pre-order node list from `serializedfile.SerializedType` and the object's
bytes, it reconstructs the nested structure.

The algorithm is driven entirely by the type tree, in this order of checks:

  1. A primitive type name reads a fixed-width value.
  2. `string` reads an int32 length, that many bytes, then aligns to 4.
  3. `TypelessData` reads an int32 length and that many raw bytes.
  4. `pair` reads its two children as a tuple.
  5. A node whose first child is `Array` is a variable-length container: the
     int32 count lives in that child, and the element type is the Array's
     *second* child. Getting this wrong reads the count as data.
  6. Anything else is a struct: one value per child, keyed by child name.

Alignment is applied after a node when its `meta_flag` carries 0x4000, and
vectors inherit the flag from their Array child. Alignment is absolute
(4-byte), measured from the start of the object payload.

Payload slices are 4-byte aligned in practice (the object table stores offsets
that are multiples of 4), so aligning within the slice matches aligning in the
file. Callers that pass a slice from an unaligned offset would see a mismatch.
"""

from __future__ import annotations

import struct

from .serializedfile import SerializedFile, SerializedType, TypeNode, _Reader

__all__ = ["Error", "attach_children", "read_object", "read_payload", "write_payload"]

_META_FLAG_ALIGN = 0x4000

# Primitive name -> (struct code, byte width). Names are Unity's, including the
# legacy spellings that still appear in type trees.
_PRIMITIVES: dict[str, tuple[str, int]] = {
    "SInt8": ("b", 1),
    "UInt8": ("B", 1),
    "char": ("B", 1),
    "short": ("h", 2),
    "SInt16": ("h", 2),
    "unsigned short": ("H", 2),
    "UInt16": ("H", 2),
    "int": ("i", 4),
    "SInt32": ("i", 4),
    "unsigned int": ("I", 4),
    "UInt32": ("I", 4),
    "Type*": ("I", 4),
    "long long": ("q", 8),
    "SInt64": ("q", 8),
    "unsigned long long": ("Q", 8),
    "UInt64": ("Q", 8),
    "FileSize": ("Q", 8),
    "float": ("f", 4),
    "double": ("d", 8),
    "bool": ("?", 1),
}


class Error(ValueError):
    """Raised when a payload does not fit its declared type tree."""


def attach_children(nodes: list[TypeNode]) -> TypeNode | None:
    """Link a flat pre-order node list into a tree and return its root.

    A node at level L is a child of the most recent node at level L-1. Levels
    are not a depth counter to trust blindly, so the stack is rebuilt from the
    sequence rather than indexed by level.
    """
    if not nodes:
        return None
    for n in nodes:
        n.children = []
    stack: list[TypeNode] = []
    for node in nodes:
        while stack and stack[-1].level >= node.level:
            stack.pop()
        if stack:
            stack[-1].children.append(node)
        stack.append(node)
    return nodes[0]


class _ObjectReader:
    """Walks one object payload guided by its type tree."""

    def __init__(self, root: TypeNode, data: bytes, endian: str):
        self.root = root
        self.r = _Reader(data, endian)

    # -- primitives ------------------------------------------------------ #
    def _aligned_string(self) -> str:
        length = self.r.i32()
        if length < 0 or length > self.r.limit - self.r.pos:
            raise Error("string length %d at %d does not fit object" % (length, self.r.pos))
        if length == 0:
            # Unity still aligns after an empty string.
            self.r.align(4)
            return ""
        raw = self.r._take(length)
        self.r.align(4)
        return raw.decode("utf-8", "surrogateescape")

    def _typeless_data(self) -> bytes:
        length = self.r.i32()
        if length < 0 or length > self.r.limit - self.r.pos:
            raise Error("TypelessData length %d does not fit object" % length)
        return self.r._take(length)

    # -- tree walk ------------------------------------------------------- #
    def read(self):
        return self._value(self.root)

    def _value(self, node: TypeNode):
        align = bool(node.meta_flag & _META_FLAG_ALIGN)

        prim = _PRIMITIVES.get(node.type_name)
        if prim is not None:
            code, width = prim
            value = struct.unpack(self.r.endian + code, self.r._take(width))[0]
        elif node.type_name == "string":
            value = self._aligned_string()
        elif node.type_name == "TypelessData":
            value = self._typeless_data()
        elif node.type_name == "pair":
            if len(node.children) < 2:
                raise Error("pair node %r has %d children" % (node.name, len(node.children)))
            value = (self._value(node.children[0]), self._value(node.children[1]))
        elif node.children and node.children[0].type_name == "Array":
            arr = node.children[0]
            if arr.meta_flag & _META_FLAG_ALIGN:
                align = True
            if len(arr.children) < 2:
                raise Error("array node %r has %d children" % (node.name, len(arr.children)))
            size = self.r.i32()
            if size < 0:
                raise Error("negative array length %d at %r" % (size, node.name))
            remaining = self.r.limit - self.r.pos
            # Guard against a bogus count turning into a huge allocation.
            if size and size > remaining:
                raise Error(
                    "array %r wants %d elements with %d bytes left"
                    % (node.name, size, remaining)
                )
            subtype = arr.children[1]
            value = [self._value(subtype) for _ in range(size)]
        else:
            value = {}
            for child in node.children:
                value[child.name] = self._value(child)

        if align:
            self.r.align(4)
        return value


def read_object(sf: SerializedFile, path_id: int):
    """Decode one object by path ID into nested dicts, lists and scalars."""
    obj = sf.objects.get(path_id)
    if obj is None:
        raise KeyError("no object with path_id %d" % path_id)
    return read_payload(sf, obj)


def read_payload(sf: SerializedFile, obj) -> object:
    """Decode one ObjectInfo's payload using its type tree."""
    stype: SerializedType = sf.type_of(obj)
    if not stype.nodes:
        raise Error("type %d has no type tree" % obj.type_id)
    root = attach_children(stype.nodes)
    return _ObjectReader(root, sf.object_bytes(obj), sf.endian).read()


class _ObjectWriter:
    """Serializes values back through a type tree, mirroring _ObjectReader.

    The contract this exists to satisfy: writing a value that was just read must
    reproduce the original bytes exactly. That only holds if every alignment
    decision here mirrors the reader, so both sides key off the same
    `meta_flag & 0x4000` test and the same Array-child rule.
    """

    def __init__(self, root: TypeNode, endian: str):
        self.root = root
        self.endian = endian
        self.buf = bytearray()

    def align(self, n: int = 4) -> None:
        pad = (-len(self.buf)) % n
        if pad:
            self.buf += b"\x00" * pad

    def write(self, value) -> bytes:
        self._value(self.root, value)
        return bytes(self.buf)

    def _value(self, node: TypeNode, value) -> None:
        align = bool(node.meta_flag & _META_FLAG_ALIGN)

        prim = _PRIMITIVES.get(node.type_name)
        if prim is not None:
            code, _width = prim
            if node.type_name == "bool":
                self.buf += struct.pack(self.endian + "?", bool(value))
            else:
                self.buf += struct.pack(self.endian + code, value)
        elif node.type_name == "string":
            raw = value.encode("utf-8", "surrogateescape")
            self.buf += struct.pack(self.endian + "i", len(raw))
            self.buf += raw
            self.align(4)
        elif node.type_name == "TypelessData":
            raw = bytes(value)
            self.buf += struct.pack(self.endian + "i", len(raw))
            self.buf += raw
        elif node.type_name == "pair":
            if len(node.children) < 2:
                raise Error("pair node %r has %d children" % (node.name, len(node.children)))
            first, second = value
            self._value(node.children[0], first)
            self._value(node.children[1], second)
        elif node.children and node.children[0].type_name == "Array":
            arr = node.children[0]
            if arr.meta_flag & _META_FLAG_ALIGN:
                align = True
            if len(arr.children) < 2:
                raise Error("array node %r has %d children" % (node.name, len(arr.children)))
            items = list(value)
            self.buf += struct.pack(self.endian + "i", len(items))
            subtype = arr.children[1]
            for item in items:
                self._value(subtype, item)
        else:
            if not isinstance(value, dict):
                raise Error(
                    "node %r expects a struct, got %s" % (node.name, type(value).__name__)
                )
            for child in node.children:
                if child.name not in value:
                    raise Error("missing field %r for node %r" % (child.name, node.name))
                self._value(child, value[child.name])

        if align:
            self.align(4)


def write_payload(sf: SerializedFile, obj, value) -> bytes:
    """Serialize `value` using the object's type tree."""
    stype: SerializedType = sf.type_of(obj)
    if not stype.nodes:
        raise Error("type %d has no type tree" % obj.type_id)
    root = attach_children(stype.nodes)
    return _ObjectWriter(root, sf.endian).write(value)
