"""Open a GF2 bundle with UnityPy, patch objects, rebuild the container."""

from __future__ import annotations

import io
import os

import UnityPy

from .bundle import BundleInfo, Node, build, decrypt, encrypt, nodes_of, parse


class Bundle:
    def __init__(self, path: str):
        self.path = path
        self.raw = open(path, "rb").read()
        self.plain = decrypt(self.raw)
        self.info: BundleInfo = parse(self.plain)
        self.nodes = nodes_of(self.plain, self.info)
        self.env = UnityPy.load(io.BytesIO(self.nodes[0].data))

    # -- inspection ------------------------------------------------------ #
    def objects(self):
        for o in self.env.objects:
            yield o

    def find(self, type_name: str, name: str | None = None):
        for o in self.env.objects:
            if o.type.name != type_name:
                continue
            if name is None:
                yield o
                continue
            try:
                tree = o.read_typetree()
                if tree.get("m_Name") == name:
                    yield o
            except Exception:
                continue

    def mesh(self, name: str):
        for o in self.find("Mesh", name):
            return o
        raise KeyError("mesh %r not found" % name)

    def meshes(self) -> list:
        return list(self.find("Mesh"))

    # -- write-back ------------------------------------------------------ #
    def save(self, out_path: str | None = None, compress: bool = True) -> bytes:
        """Serialize the (possibly modified) SerializedFile and rebuild the container."""
        sf = self.env.file.save()
        if not sf:
            raise RuntimeError("SerializedFile save failed")
        nodes = list(self.nodes)
        nodes[0] = Node(path=nodes[0].path, data=sf, flags=nodes[0].flags)
        plain = build(nodes, self.info, compress=compress)
        cipher = encrypt(plain, self.raw[:4])
        out_path = out_path or self.path
        tmp = out_path + ".tmp"
        with open(tmp, "wb") as fh:
            fh.write(cipher)
        os.replace(tmp, out_path)
        return cipher


def open_bundle(path: str) -> Bundle:
    return Bundle(path)


def texture_info(b: Bundle, name: str) -> dict:
    """Locate a Texture2D payload: either inside the .resS node or inline."""
    for o in b.objects():
        if o.type.name != "Texture2D":
            continue
        t = o.read_typetree()
        if t.get("m_Name") != name:
            continue
        sd = t.get("m_StreamData") or {}
        if sd.get("path"):
            return {
                "name": name,
                "streamed": True,
                "offset": int(sd.get("offset") or 0),
                "size": int(sd.get("size") or 0),
                "format": t.get("m_TextureFormat"),
                "width": t.get("m_Width"),
                "height": t.get("m_Height"),
            }
        img = t.get("image data")
        if img is None:
            img = t.get("m_ImageData")
        if img is None:
            # UnityPy keeps a Texture2D's pixels in the typetree field
            # `image data`; older/odd serializations expose `image_data`
            try:
                img = obj.read().image_data
            except Exception:
                img = None
        return {
            "name": name,
            "streamed": False,
            "inline": bytes(img) if img is not None else b"",
            "format": t.get("m_TextureFormat"),
            "width": t.get("m_Width"),
            "height": t.get("m_Height"),
        }
    raise KeyError("texture %r not found" % name)


def texture_payload(b: Bundle, info: dict) -> bytes:
    if info["streamed"]:
        if len(b.nodes) < 2:
            raise ValueError("bundle has no .resS node")
        data = b.nodes[1].data
        return data[info["offset"]:info["offset"] + info["size"]]
    return info["inline"]
