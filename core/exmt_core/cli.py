"""Command line entry point: python -m exmt_core <command> ...

Commands
    list      <bundle>                          list objects
    meshes    <bundle>                          list mesh names
    extract   <bundle> <mesh> <out.exmtmesh>    export one mesh (+ bones/weights)
    extract-all <bundle> <outdir>               export every mesh in a bundle
    pack      <bundle> <mesh> <in.exmtmesh> <out.bundle>

    texindex  <bundleDir> <out.json>            index which bundle holds which texture
    skinfo    <bundle>                          meshes of one bundle as JSON
    geo       <bundle> <mesh>                   preview geometry as JSON (base64 f32)
    geoset    <spec.json>                       every mesh of one skin in one call
    thumbs    <bundle> <size> <texture...>      downscaled PNGs for the texture grid
    workspace <outdir> <spec.json>              meshes + textures + manifest for Blender
"""

from __future__ import annotations

import json
import os
import sys

from . import interchange, mesh as M
from .sf import open_bundle


def cmd_list(args):
    b = open_bundle(args[0])
    for o in b.objects():
        name = ""
        try:
            name = o.read_typetree().get("m_Name", "")
        except Exception:
            pass
        print("%-24s path_id=%-20d %s" % (o.type.name, o.path_id, name))
    return 0


def cmd_meshes(args):
    b = open_bundle(args[0])
    for o in b.meshes():
        t = o.read_typetree()
        md = M.from_tree(t)
        print("%s  verts=%d idx=%d bones=%d submeshes=%d" % (
            md.name, md.vertex_count, len(md.indices),
            len(md.bone_hashes), len(md.submeshes)))
    return 0


def cmd_extract(args):
    bundle_path, mesh_name, out = args
    b = open_bundle(bundle_path)
    o = b.mesh(mesh_name)
    tree = o.read_typetree()
    md = M.from_tree(tree)
    interchange.save(md, out, {"bundle": os.path.basename(bundle_path)}, tree=tree)
    print("exported %s -> %s (%d verts, %d bones)" % (md.name, out, md.vertex_count, len(md.bone_hashes)))
    return 0


def cmd_extract_all(args):
    bundle_path, outdir = args
    os.makedirs(outdir, exist_ok=True)
    b = open_bundle(bundle_path)
    skeleton = {}
    for o in b.meshes():
        tree = o.read_typetree()
        md = M.from_tree(tree)
        p = os.path.join(outdir, md.name + ".exmtmesh")
        interchange.save(md, p, {"bundle": os.path.basename(bundle_path)}, tree=tree)
        print("exported", p)
        # the .exmtmesh already carries bindpose + bone hashes; dump them as plain
        # text too, so the rig is inspectable without opening the binary file
        if md.bindpose is not None:
            import numpy as np

            rest = np.linalg.inv(np.asarray(md.bindpose, dtype=np.float64))
            skeleton[md.name] = {
                "bone_count": len(md.bone_hashes),
                "root_bone_hash": int(tree.get("m_RootBoneNameHash") or 0),
                "bones": [
                    {
                        "index": i,
                        "hash": int(md.bone_hashes[i]) if i < len(md.bone_hashes) else 0,
                        "rest_position": [round(float(v), 6) for v in rest[i][:3, 3]],
                    }
                    for i in range(len(md.bone_hashes))
                ],
            }
    if skeleton:
        with open(os.path.join(outdir, "skeleton.json"), "w", encoding="utf-8") as fh:
            json.dump({"bundle": os.path.basename(bundle_path), "meshes": skeleton}, fh,
                      ensure_ascii=False, indent=1)
        print("exported skeleton.json (%d meshes)" % len(skeleton))
    return 0


def cmd_hide_mesh(args):
    """hide-mesh <bundle> <mesh> <out.bundle> — collapse a mesh so nothing renders."""
    import numpy as np

    bundle_path, mesh_name, out = args
    b = open_bundle(bundle_path)
    o = b.mesh(mesh_name)
    tree = o.read_typetree()
    md = M.from_tree(tree)
    md.arrays[0] = np.full_like(md.arrays[0], 0.0, dtype=np.float32)
    md.arrays[0][:, 1] = -10000.0
    if 1 in md.arrays:
        md.arrays[1] = np.zeros_like(md.arrays[1], dtype=np.float32)
        md.arrays[1][:, 1] = 1.0
    md.aabb = M.aabb_from_vertices(md.arrays[0].astype("float32"))
    M.to_tree(tree, md)
    o.save_typetree(tree)
    size = len(b.save(out))
    print(json.dumps({"output": out, "bytes": size, "mesh": mesh_name,
                      "verts": md.vertex_count, "hidden": True}))
    return 0


def cmd_subdivide(args):
    """subdivide <in.exmtmesh> <out.exmtmesh> <target_vertices> — grow a mesh."""
    import numpy as np

    from . import subdivide as S

    src, out, target = args[0], args[1], int(args[2])
    md, _ = interchange.load(src)
    big = S.subdivide(md, target)
    interchange.rebuild_submeshes(big, np.array([0], dtype=np.uint32),
                                  np.array([len(big.indices)], dtype=np.uint32))
    interchange.save(big, out, {"bundle": "", "subdivided_from": md.name})
    print(json.dumps({"output": out, "verts_in": md.vertex_count,
                      "verts_out": big.vertex_count, "indices": int(len(big.indices)),
                      "index_format": big.index_format}))
    return 0


def cmd_pack(args):
    """pack <bundle> <mesh> <in.exmtmesh> <out.bundle>

    The `.exmtmesh` carries the mesh's full typetree, so packing restores every
    field the game shipped (bone hashes, bind poses, per-bone AABBs, collision
    meshes, usage flags …) and only replaces the geometry.
    """
    import copy

    bundle_path, mesh_name, src, out = args
    b = open_bundle(bundle_path)
    o = b.mesh(mesh_name)
    target_tree = o.read_typetree()
    md, extra = interchange.load(src, target_tree)

    base = extra["info"].get("tree")
    if base:
        tree = copy.deepcopy(base)
        # the target bundle's own name must win
        tree["m_Name"] = target_tree.get("m_Name", tree.get("m_Name"))
    else:
        tree = target_tree

    # restore binary blobs that we do not regenerate (collision meshes)
    raw = extra.get("raw") or {}
    for field, key in (("m_BakedConvexCollisionMesh", "raw_baked_convex"),
                       ("m_BakedTriangleCollisionMesh", "raw_baked_tri")):
        if tree.get(field) is None:
            if key in raw:
                tree[field] = raw[key]
            elif target_tree.get(field) is not None:
                tree[field] = target_tree[field]

    interchange.rebuild_submeshes(md, extra["first"], extra["count"])
    md.aabb = M.aabb_from_vertices(md.arrays[0].astype("float32"))
    M.to_tree(tree, md)
    o.save_typetree(tree)
    size = len(b.save(out))
    print(json.dumps({"output": out, "bytes": size, "mesh": md.name,
                      "verts": md.vertex_count, "indices": int(len(md.indices)),
                      "tree_restored": bool(base)}))
    return 0


def cmd_retarget(args):
    src_path, tpl_path, out = args
    from . import retarget as R
    src_md, _ = interchange.load(src_path)
    tpl_md, _ = interchange.load(tpl_path)
    md = R.retarget(src_md, tpl_md)
    interchange.rebuild_submeshes(md, [s["firstByte"] // 2 for s in md.submeshes],
                                  [s["indexCount"] for s in md.submeshes])
    interchange.save(md, out, {"bundle": "", "retargeted_from": src_md.name})
    print(json.dumps({"output": out, "verts": md.vertex_count,
                      "bones_src": int(len(src_md.bone_hashes)),
                      "bones_dst": int(len(tpl_md.bone_hashes))}))
    return 0


def cmd_copy_texture(args):
    from .bundle import Node, build, encrypt
    from .sf import open_bundle, texture_info, texture_payload
    dst_path, dst_name, src_path, src_name, out = args
    dst = open_bundle(dst_path)
    info = texture_info(dst, dst_name)
    src = open_bundle(src_path)
    sinfo = texture_info(src, src_name)
    payload = texture_payload(src, sinfo)
    if not info["streamed"]:
        raise ValueError("destination texture is not in .resS")
    if len(payload) != info["size"]:
        raise ValueError("size mismatch: src %d vs dst %d (formats %s/%s)" %
                         (len(payload), info["size"], sinfo["format"], info["format"]))
    resS = bytearray(dst.nodes[1].data)
    off = info["offset"]
    resS[off:off + info["size"]] = payload
    nodes = list(dst.nodes)
    nodes[1] = Node(path=nodes[1].path, data=bytes(resS), flags=nodes[1].flags)
    cipher = encrypt(build(nodes, dst.info), dst.raw[:4])
    with open(out, "wb") as fh:
        fh.write(cipher)
    print(json.dumps({"output": out, "texture": dst_name, "bytes": len(cipher),
                      "format": info["format"], "size": info["size"]}))
    return 0


def cmd_set_texture(args):
    """set-texture <bundle> <texture> <image.png> <out.bundle> — encode + replace in .resS."""
    import math

    from . import bc1
    from .bundle import Node, build, encrypt
    from .sf import open_bundle, texture_info

    bundle_path, tex, png, out = args
    b = open_bundle(bundle_path)
    info = texture_info(b, tex)
    if not info["streamed"]:
        raise SystemExit("texture %s is not stored in .resS" % tex)
    if info["format"] != 10:
        raise SystemExit("texture %s is format %s, only DXT1(10) is supported" %
                         (tex, info["format"]))
    width = int(info["width"])
    levels = int(math.log2(width)) + 1
    payload = bc1.encode_file(png, size=width, levels=levels)
    if len(payload) != info["size"]:
        raise SystemExit("encoded %d bytes but slot is %d" % (len(payload), info["size"]))
    resS = bytearray(b.nodes[1].data)
    off = info["offset"]
    resS[off:off + info["size"]] = payload
    nodes = list(b.nodes)
    nodes[1] = Node(path=nodes[1].path, data=bytes(resS), flags=nodes[1].flags)
    cipher = encrypt(build(nodes, b.info), b.raw[:4])
    with open(out, "wb") as fh:
        fh.write(cipher)
    print(json.dumps({"output": out, "texture": tex, "bytes": len(cipher),
                      "size": info["size"], "levels": levels, "width": width}))
    return 0


def cmd_texindex(args):
    """texindex <bundleDir> <out.json> - which bundle holds which Texture2D."""
    from . import preview

    summary = preview.build_texindex(args[0], args[1])
    print(json.dumps(summary))
    return 0


def cmd_skinfo(args):
    """skinfo <bundle> - meshes of one bundle (name / verts / tris)."""
    from . import preview

    print(json.dumps({"meshes": preview.list_meshes(args[0])}))
    return 0


def cmd_geo(args):
    """geo <bundle> <mesh> - positions/normals/uvs/indices for the 3D preview."""
    from . import preview

    print(json.dumps(preview.geometry(args[0], args[1])))
    return 0


def cmd_geoset(args):
    """geoset <spec.json> - every mesh of one skin in one call."""
    from . import preview

    with open(args[0], encoding="utf-8") as fh:
        spec = json.load(fh)
    print(json.dumps(preview.geoset(spec)))
    return 0


def cmd_thumbs(args):
    """thumbs <bundle> <size> <texture...> - base64 PNGs for the texture grid."""
    from . import preview

    bundle, size, names = args[0], int(args[1]), args[2:]
    print(json.dumps({"textures": preview.thumbs(bundle, names, size)}))
    return 0


def cmd_workspace(args):
    """workspace <outdir> <spec.json> - export meshes + textures for Blender."""
    from . import preview

    outdir, spec_path = args
    with open(spec_path, encoding="utf-8") as fh:
        spec = json.load(fh)
    print(json.dumps(preview.make_workspace(outdir, spec)))
    return 0


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv:
        print(__doc__)
        return 2
    cmd, rest = argv[0], argv[1:]
    table = {
        "list": cmd_list,
        "meshes": cmd_meshes,
        "extract": cmd_extract,
        "extract-all": cmd_extract_all,
        "pack": cmd_pack,
        "hide-mesh": cmd_hide_mesh,
    
        "subdivide": cmd_subdivide,
        "retarget": cmd_retarget,
        "copy-texture": cmd_copy_texture,
        "set-texture": cmd_set_texture,
        "texindex": cmd_texindex,
        "skinfo": cmd_skinfo,
        "geo": cmd_geo,
        "geoset": cmd_geoset,
        "thumbs": cmd_thumbs,
        "workspace": cmd_workspace,
    }
    if cmd not in table:
        print("unknown command", cmd)
        return 2
    return table[cmd](rest)


if __name__ == "__main__":
    raise SystemExit(main())
