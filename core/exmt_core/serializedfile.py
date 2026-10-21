"""SerializedFile reader -- the layer under the UnityFS container.

A UnityFS bundle wraps one SerializedFile (the node with flag 4). That file is a
directory plus a blob: a big-endian header, a table of per-class type trees, a
table of objects with their `(start, size)`, script types, and externals. Object
payloads live at `data_offset`, in the byte order the header declares, which is
independent of the header's own big-endian encoding.

Format notes established against Unity 2019.4.29f1 (GF2) and cross-checked with
an independent reader:

  * The header is big-endian even when the payload is little-endian. `endian`
    describes the payload only, and mixing the two up silently yields plausible
    garbage rather than an error.
  * `data_offset` is 32-bit through v21 and 64-bit from v22, where an explicit
    endianness field plus 3 reserved bytes follow it.
  * Type-tree string offsets carry 0x80000000 as a "common string" flag. The low
    bits still index the type's own string buffer, so mask before reading.
  * Object path IDs are signed 64-bit; large assets genuinely use negative IDs,
    so reading them unsigned breaks lookups.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field

from .commonstrings import COMMON_STRINGS

__all__ = [
    "ObjectInfo",
    "SerializedFile",
    "SerializedType",
    "TypeNode",
    "Error",
]


class Error(ValueError):
    """Raised when a SerializedFile does not match any supported layout."""


# Type-tree node flags. The type flag marks a class/struct field.
_TYPE_FLAG_IS_ARRAY = 0x1
_TYPE_FLAG_ALIGN = 0x4000
_TYPE_FLAG_ANY_CHILD = 0x8000


class _Reader:
    """Cursor over a bytes buffer with an explicit byte order."""

    __slots__ = ("data", "pos", "endian", "limit")

    def __init__(self, data: bytes, endian: str = ">", pos: int = 0, limit: int | None = None):
        self.data = data
        self.pos = pos
        self.endian = endian
        self.limit = len(data) if limit is None else limit

    def _take(self, n: int) -> bytes:
        end = self.pos + n
        if end > self.limit:
            raise Error("read past end at %d (+%d, limit %d)" % (self.pos, n, self.limit))
        chunk = self.data[self.pos:end]
        self.pos = end
        return chunk

    def u8(self) -> int:
        return self._take(1)[0]

    def u16(self) -> int:
        return struct.unpack(self.endian + "H", self._take(2))[0]

    def u32(self) -> int:
        return struct.unpack(self.endian + "I", self._take(4))[0]

    def i32(self) -> int:
        return struct.unpack(self.endian + "i", self._take(4))[0]

    def i16(self) -> int:
        return struct.unpack(self.endian + "h", self._take(2))[0]

    def u64(self) -> int:
        return struct.unpack(self.endian + "Q", self._take(8))[0]

    def i64(self) -> int:
        return struct.unpack(self.endian + "q", self._take(8))[0]

    def align(self, n: int) -> None:
        """Advance to the next multiple of `n` relative to the file start."""
        pad = (-self.pos) % n
        if pad:
            self._take(pad)

    def cstr(self) -> str:
        """Read a NUL-terminated string; Unity writes UTF-8 here."""
        end = self.data.find(b"\0", self.pos, self.limit)
        if end < 0:
            raise Error("unterminated string at %d" % self.pos)
        raw = self.data[self.pos:end]
        self.pos = end + 1
        return raw.decode("utf-8", "replace")


@dataclass
class TypeNode:
    """One entry of a type tree. Children follow the parent at level + 1.

    `nodes` in a SerializedType is the flat pre-order list; `children` is filled
    in by `attach_children` and is what the value reader walks.
    """

    version: int
    level: int
    type_flags: int
    type_name: str
    name: str
    byte_size: int
    index: int
    meta_flag: int
    ref_type_hash: int = 0
    children: list["TypeNode"] = field(default_factory=list)

    @property
    def is_array(self) -> bool:
        return bool(self.type_flags & _TYPE_FLAG_IS_ARRAY)

    @property
    def is_align(self) -> bool:
        return bool(self.meta_flag & _TYPE_FLAG_ALIGN)

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"<node {self.type_name} {self.name} lvl={self.level} size={self.byte_size}>"


@dataclass
class SerializedType:
    class_id: int
    is_stripped: bool
    script_type_index: int
    old_type_hash: bytes
    nodes: list[TypeNode] = field(default_factory=list)
    strings: bytes = b""
    ref_type_hash: int = 0
    type_dependencies: list[int] = field(default_factory=list)

    @property
    def root(self) -> TypeNode | None:
        return self.nodes[0] if self.nodes else None


@dataclass
class ObjectInfo:
    path_id: int
    byte_start: int
    byte_size: int
    type_id: int

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"<obj {self.path_id} type={self.type_id} {self.byte_start}+{self.byte_size}>"


class _TypeTree:
    """Parser for the embedded type tree of one SerializedType."""

    def __init__(self, r: _Reader, version: int):
        self.version = version
        self.node_count = r.u32()
        self.string_size = r.u32() if version >= 19 else 0
        # A sane ceiling: Unity's largest built-in class trees are a few hundred
        # nodes, and an absurd count here means we are misaligned.
        if self.node_count > 100_000 or self.string_size > 16_000_000:
            raise Error(
                "implausible type tree (nodes=%d, strings=%d) at offset %d"
                % (self.node_count, self.string_size, r.pos)
            )
        self.nodes_start = r.pos
        node_bytes = self.node_count * (32 if version >= 19 else 24)
        if self.nodes_start + node_bytes + self.string_size > r.limit:
            raise Error("type tree runs past end of file")
        r.pos = self.nodes_start + node_bytes
        self.strings = r._take(self.string_size)
        if version >= 22:
            self.content_hash = r._take(16)
            self.extra = r.u32()

    def read_string(self, value: int) -> str:
        """Resolve a type-tree string reference.

        The reference is a 31-bit payload plus a flag: flag clear means the
        payload is a byte offset into this type's own string buffer, flag set
        means it indexes Unity's built-in name table instead. Treating the flag
        as a mask on a buffer offset -- the obvious mistake -- makes built-in
        names silently read as whatever bytes sit at that low offset.
        """
        if not (value & 0x80000000):
            offset = value
            if offset >= len(self.strings):
                raise Error(
                    "type string offset %d outside %d-byte buffer"
                    % (offset, len(self.strings))
                )
            end = self.strings.find(b"\0", offset)
            if end < 0:
                end = len(self.strings)
            return self.strings[offset:end].decode("utf-8", "replace")
        index = value & 0x7FFFFFFF
        return COMMON_STRINGS.get(index, str(index))


class SerializedFile:
    """A parsed SerializedFile. Object payloads are read lazily by offset."""

    def __init__(self, data: bytes):
        self.data = data
        # Only the fixed-size header fields are big-endian. Everything after the
        # declared endianness -- unity_version, platform, the type table, the
        # object table and every payload -- follows that declaration. Reading the
        # whole metadata block as BE yields a plausible-looking but wrong
        # type_count (0x02000000 instead of 2 here), so the switch below matters.
        r = _Reader(data, ">")
        self.metadata_size = r.u32()
        self.file_size = r.u32()
        self.version = r.u32()
        if self.version < 17 or self.version > 22:
            raise Error("unsupported SerializedFile version %d" % self.version)
        if self.version >= 22:
            self.data_offset = r.i64()
            raw_endian = r.u32()
            r.pos += 3  # reserved
        else:
            self.data_offset = r.u32()
            raw_endian = r.u32()
        self.big_endian = raw_endian != 0
        self.endian = ">" if self.big_endian else "<"
        r.endian = self.endian

        self.unity_version = r.cstr()
        self.platform = r.i32()
        self.has_type_tree = r.u8() != 0

        self.big_id_enabled = 0
        if 7 <= self.version < 14:
            self.big_id_enabled = r.i32()

        self.types: list[SerializedType] = []
        type_count = r.u32()
        if type_count > 100_000:
            raise Error("implausible type count %d" % type_count)
        for _ in range(type_count):
            self.types.append(self._read_type(r))

        self.objects: dict[int, ObjectInfo] = {}
        object_count = r.u32()
        if object_count > 5_000_000:
            raise Error("implausible object count %d" % object_count)
        for _ in range(object_count):
            path_id = r.i64()
            if self.version >= 22:
                byte_start = r.i64()
            else:
                byte_start = r.u32()
            byte_size = r.u32()
            type_id = r.i32()
            self.objects[path_id] = ObjectInfo(path_id, byte_start, byte_size, type_id)

        self.script_types = []
        if self.version >= 11:
            for _ in range(r.u32()):
                file_id = r.i32()
                if self.version < 14:
                    identifier = r.i64()
                else:
                    identifier = r.i64()
                self.script_types.append((file_id, identifier))

        self.externals = []
        if self.version >= 6:
            for _ in range(r.u32()):
                path = r.cstr()
                name = r.cstr()
                guid = r._take(16)
                kind = r.i32()
                self.externals.append((path, name, guid, kind))

        # v20+ adds a ref-type table; it does not affect object payload offsets,
        # so it is consumed defensively rather than interpreted.
        if self.version >= 20:
            ref_count = r.u32()
            if ref_count < 1_000_000:
                for _ in range(ref_count):
                    r.i32()
                    r._take(16)

        self._verify()

    def _read_type(self, r: _Reader) -> SerializedType:
        class_id = r.i32()
        is_stripped = r.u8() != 0
        script_type_index = r.i16() if self.version >= 17 else -1
        old_type_hash = r._take(16)
        t = SerializedType(
            class_id=class_id,
            is_stripped=is_stripped,
            script_type_index=script_type_index,
            old_type_hash=old_type_hash,
        )
        if self.has_type_tree:
            t.nodes, t.strings = self._read_type_tree(r)
        # v21+ appends a dependency list to every non-reference type. It is an
        # int array (count, then values) and sits *after* the type tree, so
        # omitting it shifts every later type by four bytes on a real file.
        if self.version >= 21 and not (script_type_index >= 0):
            dep_count = r.i32()
            if not 0 <= dep_count <= 100_000:
                raise Error("implausible type dependency count %d" % dep_count)
            t.type_dependencies = [r.i32() for _ in range(dep_count)]
        return t

    def _read_type_tree(self, r: _Reader) -> tuple[list[TypeNode], bytes]:
        tree = _TypeTree(r, self.version)
        nodes: list[TypeNode] = []
        node_r = _Reader(self.data, self.endian, tree.nodes_start, tree.nodes_start + tree.node_count * (32 if self.version >= 19 else 24))
        for _ in range(tree.node_count):
            version = node_r.u16()
            level = node_r.u8()
            type_flags = node_r.u8()
            type_off = node_r.u32()
            name_off = node_r.u32()
            byte_size = node_r.i32()
            index = node_r.i32()
            meta_flag = node_r.i32()
            ref_hash = node_r.u64() if self.version >= 19 else 0
            nodes.append(
                TypeNode(
                    version=version,
                    level=level,
                    type_flags=type_flags,
                    type_name=tree.read_string(type_off),
                    name=tree.read_string(name_off),
                    byte_size=byte_size,
                    index=index,
                    meta_flag=meta_flag,
                    ref_type_hash=ref_hash,
                )
            )
        return nodes, tree.strings

    def _verify(self) -> None:
        """Reject files whose tables cannot be mutually consistent."""
        if self.file_size != len(self.data):
            raise Error(
                "declared file_size %d != actual %d" % (self.file_size, len(self.data))
            )
        if not (0 < self.data_offset <= len(self.data)):
            raise Error("data_offset %d outside file" % self.data_offset)
        for obj in self.objects.values():
            if obj.type_id < 0 or obj.type_id >= len(self.types):
                raise Error("object %d has type_id %d, %d types" % (obj.path_id, obj.type_id, len(self.types)))
            end = self.data_offset + obj.byte_start + obj.byte_size
            if end > len(self.data):
                raise Error("object %d runs past end of file" % obj.path_id)

    # -- accessors ------------------------------------------------------- #
    @property
    def class_ids(self) -> list[int]:
        """Class ID of each type, in table order."""
        return [t.class_id for t in self.types]

    def object_bytes(self, obj: ObjectInfo) -> bytes:
        begin = self.data_offset + obj.byte_start
        return self.data[begin:begin + obj.byte_size]

    def read_bytes(self, obj: ObjectInfo) -> _Reader:
        return _Reader(self.object_bytes(obj), self.endian)

    def type_of(self, obj: ObjectInfo) -> SerializedType:
        return self.types[obj.type_id]


def open_serialized_file(data: bytes) -> SerializedFile:
    return SerializedFile(data)
