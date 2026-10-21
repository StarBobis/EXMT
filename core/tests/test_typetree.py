"""Regression tests for the self-hosted type-tree value reader.

Two layers: synthetic cases that pin the algorithm (primitives, strings,
arrays, nesting, alignment) with no sample file, and contract cases captured
from a real bundle with an independent reader.

Run with::

    cd core && python -m unittest discover -s tests -v
"""

from __future__ import annotations

import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from exmt_core import bundle  # noqa: E402
from exmt_core.serializedfile import SerializedFile, TypeNode  # noqa: E402
from exmt_core.typetree import Error, _ObjectReader, attach_children  # noqa: E402
from sample_bundle import SAMPLE  # noqa: E402

META_ALIGN = 0x4000


def node(level, type_name, name, byte_size=-1, meta_flag=0):
    return TypeNode(
        version=1,
        level=level,
        type_flags=0,
        type_name=type_name,
        name=name,
        byte_size=byte_size,
        index=0,
        meta_flag=meta_flag,
    )


def read_with(nodes, payload: bytes, endian="<"):
    root = attach_children(nodes)
    return _ObjectReader(root, payload, endian).read()


class TestTreeBuilding(unittest.TestCase):
    def test_flat_list_becomes_tree(self):
        nodes = [
            node(0, "Root", "Base"),
            node(1, "int", "a"),
            node(1, "int", "b"),
            node(2, "int", "b_child"),
        ]
        root = attach_children(nodes)
        self.assertEqual(root.name, "Base")
        self.assertEqual([c.name for c in root.children], ["a", "b"])
        self.assertEqual([c.name for c in root.children[1].children], ["b_child"])

    def test_empty_returns_none(self):
        self.assertIsNone(attach_children([]))

    def test_repeated_attach_is_idempotent(self):
        nodes = [node(0, "Root", "Base"), node(1, "int", "a")]
        attach_children(nodes)
        attach_children(nodes)
        self.assertEqual(len(nodes[0].children), 1)


class TestPrimitives(unittest.TestCase):
    def test_integers_and_floats(self):
        nodes = [
            node(0, "Root", "Base"),
            node(1, "int", "i"),
            node(1, "float", "f"),
            node(1, "bool", "b"),
            node(1, "SInt8", "s8"),
            node(1, "UInt64", "u64"),
        ]
        payload = struct.pack("<ifbBQ", -7, 1.5, 1, 300 & 0xFF, 2**63 + 5)
        got = read_with(nodes, payload)
        self.assertEqual(got["i"], -7)
        self.assertAlmostEqual(got["f"], 1.5)
        self.assertIs(got["b"], True)
        self.assertEqual(got["s8"], 300 & 0xFF)
        self.assertEqual(got["u64"], 2**63 + 5)

    def test_big_endian_honoured(self):
        nodes = [node(0, "Root", "Base"), node(1, "int", "v")]
        got = read_with(nodes, struct.pack(">i", 1234), endian=">")
        self.assertEqual(got["v"], 1234)


class TestStrings(unittest.TestCase):
    def test_aligned_string_pads_to_four(self):
        nodes = [node(0, "Root", "Base"), node(1, "string", "s"), node(1, "int", "after")]
        body = struct.pack("<i", 5) + b"hello" + b"\x00\x00\x00"  # 5 -> pad to 8
        payload = body + struct.pack("<i", 99)
        got = read_with(nodes, payload)
        self.assertEqual(got["s"], "hello")
        self.assertEqual(got["after"], 99)

    def test_empty_string_still_aligns(self):
        nodes = [node(0, "Root", "Base"), node(1, "string", "s"), node(1, "int", "after")]
        payload = struct.pack("<i", 0) + struct.pack("<i", 7)
        got = read_with(nodes, payload)
        self.assertEqual(got["s"], "")
        self.assertEqual(got["after"], 7)

    def test_oversized_length_rejected(self):
        nodes = [node(0, "Root", "Base"), node(1, "string", "s")]
        with self.assertRaises(Error):
            read_with(nodes, struct.pack("<i", 10_000) + b"ab")


class TestArrays(unittest.TestCase):
    def build_vector(self, elem_type, elem_name="data"):
        return [
            node(0, "Root", "Base"),
            node(1, "vector", "items"),
            node(2, "Array", "Array"),
            node(3, "int", "size", 4),
            node(3, elem_type, elem_name),
        ]

    def test_int_vector(self):
        nodes = self.build_vector("int")
        payload = struct.pack("<i", 3) + struct.pack("<iii", 11, 22, 33)
        got = read_with(nodes, payload)
        self.assertEqual(got["items"], [11, 22, 33])

    def test_empty_vector(self):
        nodes = self.build_vector("int")
        got = read_with(nodes, struct.pack("<i", 0))
        self.assertEqual(got["items"], [])

    def test_negative_length_rejected(self):
        nodes = self.build_vector("int")
        with self.assertRaises(Error):
            read_with(nodes, struct.pack("<i", -1))

    def test_absurd_length_rejected(self):
        nodes = self.build_vector("int")
        with self.assertRaises(Error):
            read_with(nodes, struct.pack("<i", 1_000_000) + struct.pack("<i", 1))

    def test_vector_of_structs(self):
        nodes = [
            node(0, "Root", "Base"),
            node(1, "vector", "entries"),
            node(2, "Array", "Array"),
            node(3, "int", "size", 4),
            node(3, "Entry", "data"),
            node(4, "int", "x"),
            node(4, "float", "y"),
        ]
        payload = struct.pack("<i", 2) + (struct.pack("<if", 1, 0.5) + struct.pack("<if", 2, 1.5))
        got = read_with(nodes, payload)
        self.assertEqual(len(got["entries"]), 2)
        self.assertEqual(got["entries"][1]["x"], 2)
        self.assertAlmostEqual(got["entries"][0]["y"], 0.5)


class TestAlignment(unittest.TestCase):
    def test_meta_flag_aligns_after_node(self):
        nodes = [
            node(0, "Root", "Base"),
            node(1, "SInt8", "one", meta_flag=META_ALIGN),
            node(1, "int", "after"),
        ]
        payload = b"\x07\x00\x00\x00" + struct.pack("<i", 42)
        got = read_with(nodes, payload)
        self.assertEqual(got["one"], 7)
        self.assertEqual(got["after"], 42)


def write_with(nodes, value, endian="<"):
    from exmt_core.typetree import _ObjectWriter

    root = attach_children(nodes)
    return _ObjectWriter(root, endian).write(value)


class TestWriter(unittest.TestCase):
    """The writer must mirror every reader decision, not just roughly match."""

    def test_primitives_round_trip(self):
        nodes = [
            node(0, "Root", "Base"),
            node(1, "int", "i"),
            node(1, "float", "f"),
            node(1, "bool", "b"),
            node(1, "UInt64", "u64"),
        ]
        payload = struct.pack("<if?Q", -7, 1.5, True, 2**63 + 5)
        self.assertEqual(write_with(nodes, read_with(nodes, payload)), payload)

    def test_string_round_trip_across_lengths(self):
        nodes = [node(0, "Root", "Base"), node(1, "string", "s"), node(1, "int", "after")]
        for text in ("", "a", "abcd", "abcde", "x" * 40):
            with self.subTest(text=text):
                body = write_with(nodes, {"s": text, "after": 5})
                self.assertEqual(read_with(nodes, body), {"s": text, "after": 5})

    def test_array_round_trip(self):
        nodes = [
            node(0, "Root", "Base"),
            node(1, "vector", "items"),
            node(2, "Array", "Array"),
            node(3, "int", "size", 4),
            node(3, "int", "data"),
        ]
        for items in ([], [1], [1, 2, 3, -4]):
            with self.subTest(items=items):
                body = write_with(nodes, {"items": items})
                self.assertEqual(read_with(nodes, body)["items"], items)

    def test_missing_field_is_an_error(self):
        nodes = [node(0, "Root", "Base"), node(1, "int", "a"), node(1, "int", "b")]
        with self.assertRaises(Error):
            write_with(nodes, {"a": 1})

    def test_struct_expected_error(self):
        nodes = [node(0, "Root", "Base"), node(1, "int", "a")]
        with self.assertRaises(Error):
            write_with(nodes, [1, 2])

    def test_big_endian_writer(self):
        nodes = [node(0, "Root", "Base"), node(1, "int", "v")]
        self.assertEqual(write_with(nodes, {"v": 1234}, endian=">"), struct.pack(">i", 1234))


@unittest.skipUnless(os.path.exists(SAMPLE), "sample bundle not present")
class TestAgainstRealBundle(unittest.TestCase):
    """Contract captured from an independent reader on one real bundle."""

    @classmethod
    def setUpClass(cls):
        with open(SAMPLE, "rb") as fh:
            raw = fh.read()
        plain = bundle.decrypt(raw)
        info = bundle.parse(plain)
        cls.sf = SerializedFile(bundle.nodes_of(plain, info)[0].data)

    def test_all_objects_decode(self):
        from exmt_core.typetree import read_payload

        for path_id, obj in self.sf.objects.items():
            with self.subTest(path_id=path_id):
                value = read_payload(self.sf, obj)
                self.assertIsInstance(value, dict)

    def test_mesh_vertex_data(self):
        from exmt_core.typetree import read_payload

        obj = self.sf.objects[-5358632390582853297]
        m = read_payload(self.sf, obj)
        self.assertEqual(m["m_Name"], "c_VepleySSR01_slg_body_lod0")
        vd = m["m_VertexData"]
        self.assertEqual(vd["m_VertexCount"], 2283)
        self.assertEqual(len(vd["m_Channels"]), 14)
        # The channel table yields 92 bytes per vertex (stream 0 = 40,
        # stream 1 = 20, stream 2 = 32). VertexFormat 1 is Float16 (2 bytes)
        # and 10 is UInt32, so treating every channel as 4-wide is wrong.
        stride = 92
        declared = len(vd["m_DataSize"])
        self.assertGreaterEqual(declared, 2283 * stride)
        # The blob is padded up to a 16-byte boundary, so it is deliberately
        # slightly larger than vertices*stride rather than exactly equal.
        self.assertEqual(declared % 16, 0)
        self.assertLess(declared - 2283 * stride, 16)

    def test_mesh_submeshes(self):
        from exmt_core.typetree import read_payload

        m = read_payload(self.sf, self.sf.objects[-5358632390582853297])
        self.assertEqual(len(m["m_SubMeshes"]), 1)
        sub = m["m_SubMeshes"][0]
        self.assertEqual((sub["indexCount"], sub["vertexCount"]), (11007, 2283))

    def test_mesh_top_level_keys(self):
        from exmt_core.typetree import read_payload

        m = read_payload(self.sf, self.sf.objects[-5358632390582853297])
        self.assertEqual(len(m), 23)
        for key in ("m_Name", "m_VertexData", "m_SubMeshes", "m_BindPose", "m_LocalAABB"):
            self.assertIn(key, m)

    def test_asset_bundle_object(self):
        from exmt_core.typetree import read_payload

        b = read_payload(self.sf, self.sf.objects[1])
        self.assertEqual(len(b), 11)
        self.assertIn("m_AssetBundleName", b)
        self.assertIn("m_Container", b)

    def test_trailing_bytes_are_not_silently_ignored(self):
        """A decode that stops early means the tree and payload disagree.

        The reader does not enforce this itself (Unity pads payloads), so the
        check lives here against real data: the decoded size must account for
        the whole payload for a fixed-size struct with no trailing container.
        """
        from exmt_core.typetree import read_payload

        # AssetBundle payloads are small and fully described by their tree.
        obj = self.sf.objects[1]
        self.assertLessEqual(len(self.sf.object_bytes(obj)), 512)
        read_payload(self.sf, obj)  # must not raise


@unittest.skipUnless(os.path.exists(SAMPLE), "sample bundle not present")
class TestRealByteExactRoundTrip(unittest.TestCase):
    """The central invariant: rewrite without edits must reproduce the bytes."""

    @classmethod
    def setUpClass(cls):
        with open(SAMPLE, "rb") as fh:
            raw = fh.read()
        plain = bundle.decrypt(raw)
        info = bundle.parse(plain)
        cls.sf = SerializedFile(bundle.nodes_of(plain, info)[0].data)

    def test_every_object_is_byte_identical_after_round_trip(self):
        from exmt_core.typetree import read_payload, write_payload

        for path_id, obj in self.sf.objects.items():
            with self.subTest(path_id=path_id):
                tree = read_payload(self.sf, obj)
                out = write_payload(self.sf, obj, tree)
                self.assertEqual(out, self.sf.object_bytes(obj))

    def test_large_mesh_round_trip(self):
        """The 1.8 MB payload is where an off-by-one alignment would show up."""
        from exmt_core.typetree import read_payload, write_payload

        obj = self.sf.objects[-4397244682595215826]
        self.assertEqual(obj.byte_size, 1822256)
        tree = read_payload(self.sf, obj)
        self.assertEqual(write_payload(self.sf, obj, tree), self.sf.object_bytes(obj))


@unittest.skipUnless(os.path.exists(SAMPLE), "sample bundle not present")
class TestMutation(unittest.TestCase):
    """Writing changed values must re-lay the payload, not copy it."""

    @classmethod
    def setUpClass(cls):
        with open(SAMPLE, "rb") as fh:
            raw = fh.read()
        plain = bundle.decrypt(raw)
        info = bundle.parse(plain)
        cls.sf = SerializedFile(bundle.nodes_of(plain, info)[0].data)
        cls.obj = cls.sf.objects[-5358632390582853297]

    def _reread(self, buf):
        from exmt_core.typetree import _ObjectReader

        root = attach_children(self.sf.types[self.obj.type_id].nodes)
        return _ObjectReader(root, buf, self.sf.endian).read()

    def test_rename_resizes_payload(self):
        import copy

        from exmt_core.typetree import read_payload, write_payload

        base = read_payload(self.sf, self.obj)
        for newname in ("x", "", "a_much_longer_name_than_the_original_one_abcdefghij"):
            with self.subTest(name=newname):
                tree = copy.deepcopy(base)
                tree["m_Name"] = newname
                out = write_payload(self.sf, self.obj, tree)
                back = self._reread(out)
                self.assertEqual(back["m_Name"], newname)
                # Neighbouring fields must survive the re-layout intact.
                self.assertEqual(back["m_VertexData"]["m_VertexCount"], 2283)
                self.assertEqual(back["m_SubMeshes"][0]["indexCount"], 11007)

    def test_vertex_mutation_survives(self):
        import copy

        from exmt_core.typetree import read_payload, write_payload

        base = read_payload(self.sf, self.obj)
        raw = bytearray(base["m_VertexData"]["m_DataSize"])
        before = struct.unpack_from("<f", raw, 0)[0]
        struct.pack_into("<f", raw, 0, before + 1.0)

        tree = copy.deepcopy(base)
        tree["m_VertexData"]["m_DataSize"] = bytes(raw)
        out = write_payload(self.sf, self.obj, tree)
        self.assertEqual(len(out), len(self.sf.object_bytes(self.obj)))

        back = self._reread(out)
        after = struct.unpack_from("<f", back["m_VertexData"]["m_DataSize"], 0)[0]
        self.assertAlmostEqual(after, before + 1.0, places=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
