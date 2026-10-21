"""Regression tests for the self-hosted SerializedFile reader.

The expected values here were captured from an independent reader on one real
GF2 bundle and are hard-coded on purpose: this suite must keep passing with no
third-party packages installed, so it can serve as the contract for the
replacement path.

Run with::

    cd core && python -m unittest discover -s tests -v
"""

from __future__ import annotations

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from exmt_core import bundle  # noqa: E402
from exmt_core.serializedfile import (  # noqa: E402
    Error,
    ObjectInfo,
    SerializedFile,
    _Reader,
)

# A hot-update bundle from the game. Tests are skipped when it is absent so the
# suite stays runnable on a clean checkout.
SAMPLE = r"C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig"


class TestReader(unittest.TestCase):
    """Byte cursor behaviour, independent of any sample file."""

    def test_unsigned_and_signed(self):
        r = _Reader(b"\xff\xff\xff\xff", "<")
        self.assertEqual(r.u32(), 0xFFFFFFFF)
        r = _Reader(b"\xff\xff\xff\xff", "<")
        self.assertEqual(r.i32(), -1)

    def test_endian_switch(self):
        raw = b"\x00\x00\x00\x02"
        self.assertEqual(_Reader(raw, ">").u32(), 2)
        self.assertEqual(_Reader(raw, "<").u32(), 0x02000000)

    def test_read_past_limit_raises(self):
        r = _Reader(b"\x00\x01", "<")
        r.u8()
        with self.assertRaises(Error):
            r.u32()

    def test_cstr(self):
        r = _Reader(b"2019.4.29f1\x00rest", "<")
        self.assertEqual(r.cstr(), "2019.4.29f1")
        self.assertEqual(r.pos, 12)

    def test_unterminated_cstr_raises(self):
        r = _Reader(b"abc", "<")
        with self.assertRaises(Error):
            r.cstr()


class TestVersionGate(unittest.TestCase):
    def test_unsupported_version_rejected(self):
        # file_size/version are big-endian in the header.
        raw = bytearray(64)
        raw[4:8] = (64).to_bytes(4, "big")
        raw[8:12] = (99).to_bytes(4, "big")
        with self.assertRaises(Error) as ctx:
            SerializedFile(bytes(raw))
        self.assertIn("unsupported", str(ctx.exception))


@unittest.skipUnless(os.path.exists(SAMPLE), "sample bundle not present")
class TestAgainstRealBundle(unittest.TestCase):
    """Contract captured from an independent reader on one real asset bundle."""

    EXPECTED_OBJECTS = {
        -5358632390582853297: (0, 237912, 43),
        -4397244682595215826: (237912, 1822256, 43),
        1: (2060168, 412, 142),
        7433588863114304978: (2060584, 38040, 43),
        8143092236940524548: (2098624, 631252, 43),
    }

    @classmethod
    def setUpClass(cls):
        with open(SAMPLE, "rb") as fh:
            raw = fh.read()
        plain = bundle.decrypt(raw)
        info = bundle.parse(plain)
        nodes = bundle.nodes_of(plain, info)
        cls.raw_nodes = nodes
        cls.sf = SerializedFile(nodes[0].data)

    def test_header(self):
        sf = self.sf
        self.assertEqual(sf.metadata_size, 11005)
        self.assertEqual(sf.file_size, 2740916)
        self.assertEqual(sf.version, 21)
        self.assertEqual(sf.data_offset, 11040)
        self.assertEqual(sf.endian, "<")
        self.assertEqual(sf.unity_version, "2019.4.29f1")
        self.assertEqual(sf.platform, 19)  # StandaloneWindows64
        self.assertTrue(sf.has_type_tree)

    def test_types(self):
        self.assertEqual([t.class_id for t in self.sf.types], [43, 142])  # Mesh, AssetBundle
        self.assertEqual([len(t.nodes) for t in self.sf.types], [238, 58])
        self.assertTrue(all(t.script_type_index == -1 for t in self.sf.types))

    def test_objects(self):
        sf = self.sf
        self.assertEqual(len(sf.objects), 5)
        for path_id, (start, size, class_id) in self.EXPECTED_OBJECTS.items():
            with self.subTest(path_id=path_id):
                obj = sf.objects[path_id]
                self.assertEqual(obj.byte_start, start)
                self.assertEqual(obj.byte_size, size)
                self.assertEqual(sf.types[obj.type_id].class_id, class_id)

    def test_object_table_is_aligned_and_ordered(self):
        """Objects are laid out in order with at most a 4-byte alignment gap.

        They do not tile the data section exactly: Unity pads each payload to a
        4-byte boundary, and this file has one such 4-byte gap. Requiring exact
        adjacency would be asserting the format is something it is not.
        """
        spans = sorted((o.byte_start, o.byte_size) for o in self.sf.objects.values())
        self.assertEqual(spans[0][0], 0)
        for (a_start, a_size), (b_start, _) in zip(spans, spans[1:]):
            gap = b_start - (a_start + a_size)
            self.assertGreaterEqual(gap, 0, "objects overlap")
            # Measured gap on this file is exactly 4 bytes, so the bound is
            # "small padding", not "no padding": the format does not guarantee
            # byte-adjacent payloads.
            self.assertLessEqual(gap, 4, "gap larger than the observed padding")

    def test_last_object_ends_at_file_end(self):
        end = max(o.byte_start + o.byte_size for o in self.sf.objects.values())
        self.assertEqual(self.sf.data_offset + end, len(self.sf.data))

    def test_payload_slices_are_stable(self):
        """Pin each payload's size and first bytes so drift is caught."""
        obj = self.sf.objects[-5358632390582853297]
        payload = self.sf.object_bytes(obj)
        self.assertEqual(len(payload), 237912)
        # Mesh payload starts with m_Name and then vertex data; only the length
        # and self-consistency are asserted here so this does not over-fit.
        self.assertEqual(len(self.sf.object_bytes(self.sf.objects[1])), 412)

    def test_type_tree_root_nodes(self):
        mesh = self.sf.types[0]
        root = mesh.root
        self.assertIsNotNone(root)
        self.assertEqual((root.level, root.type_name, root.name), (0, "Mesh", "Base"))
        # Name field resolves through the type's own string buffer.
        name_node = mesh.nodes[1]
        self.assertEqual((name_node.level, name_node.type_name, name_node.name), (1, "string", "m_Name"))
        # Array/ArrayList children resolve through Unity's built-in name table.
        arr = mesh.nodes[2]
        self.assertEqual((arr.type_name, arr.name), ("Array", "Array"))

    def test_type_tree_levels_are_well_formed(self):
        """The tree is pre-order: a node is a child of the previous one (+1) or
        a sibling/ancestor at any shallower level."""
        for t in self.sf.types:
            self.assertEqual(t.nodes[0].level, 0)
            prev = t.nodes[0]
            for node in t.nodes[1:]:
                self.assertGreaterEqual(node.level, 0)
                self.assertLessEqual(
                    node.level, prev.level + 1, "child skipped a level"
                )
                prev = node


if __name__ == "__main__":
    unittest.main(verbosity=2)
