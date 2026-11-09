"""Turn an EXMT workspace export into a `.blend` with one rig and skinned meshes.

The workspace folder (written by `workspace_cli`) holds `skeleton.json` -- the
merged rig of that skin -- and `meshes/*.exmtmesh`.  This script builds one
armature from that rig, skins every mesh against it, checks the result and
saves the file.

Checks (the project's four-number rule for skin data):

* every vertex group's weight row sums to 1;
* a mesh's bone indices stay inside *its own* bone table -- the number of its
  vertex groups, not the number of influence slots (4);
* a mesh whose weights are all zero is a rigid accessory (a plush toy, a
  trinket) that the game pins to one bone, not broken data;
* every vertex group has a bone in the rig.

Run headless:

    blender --background --factory-startup --python scripts/build_blend.py -- \
        --workspace <workspace dir> --out <file.blend> [--label <name>] [--preview <png>]

`--preview` renders a Workbench image (shape and rig) next to the file.
"""

import argparse
import importlib.util
import json
import os
import sys

import bpy
import numpy as np
from mathutils import Matrix, Vector

ADDONS = os.path.join(
    os.path.expanduser("~"), "AppData", "Roaming", "Blender Foundation", "Blender",
    "5.2", "scripts", "addons",
)


def plugin():
    """The add-on's mesh module on its own.

    Importing the package would register its operators, which needs a full UI
    context; `mesh_io` only needs bpy, numpy and mathutils.
    """
    path = os.path.join(ADDONS, "EXMTBlender", "mesh_io.py")
    spec = importlib.util.spec_from_file_location("exmt_mesh_io", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def clear_scene():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    for block in (bpy.data.meshes, bpy.data.armatures, bpy.data.materials):
        for item in list(block):
            if item.users == 0:
                block.remove(item)


def look_at(obj, center, up=Vector((0.0, 1.0, 0.0))):
    """Aim `obj` at `center` with `up` as the screen-up axis.

    `Vector.to_track_quat` always aligns its up axis with world **Z**, so a
    standing character ends up lying on its side in the render; the camera
    basis is built by hand instead.
    """
    forward = (center - obj.location).normalized()
    right = forward.cross(up)
    if right.length < 1e-6:
        right = Vector((1.0, 0.0, 0.0))
    right.normalize()
    true_up = right.cross(forward).normalized()
    basis = Matrix((right, true_up, -forward)).transposed()
    obj.rotation_euler = basis.to_euler()


def rig_shape(arm, bones):
    """Roots, depth and where the unnamed bones sit."""
    roots, unnamed, depth = [], [], {}
    for bone in bones:
        parent = bone.get("parent", -1)
        level, seen = 0, set()
        while isinstance(parent, int) and 0 <= parent < len(bones) and parent not in seen:
            seen.add(parent)
            level += 1
            parent = bones[parent].get("parent", -1)
        depth[bone["name"]] = level
        if level == 0:
            roots.append(bone["name"])
        if str(bone.get("name", "")).startswith("bone_"):
            unnamed.append(bone["name"])
    return {
        "root_count": len(roots),
        "max_depth": max(depth.values()) if depth else 0,
        "unnamed": len(unnamed),
    }


def preview(arm, meshes, path):
    """Workbench renders of the shape and of the rig alone."""
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.render.resolution_x = 1100
    scene.render.resolution_y = 1100
    shading = scene.display.shading
    shading.light = "STUDIO"
    shading.color_type = "RANDOM"
    shading.show_shadows = True
    shading.show_cavity = True
    shading.show_object_outline = True
    scene.display.render_aa = "8"

    points = []
    for obj in meshes:
        for corner in obj.bound_box:
            points.append(obj.matrix_world @ Vector(corner))
    for bone in arm.data.bones:
        points.append(arm.matrix_world @ bone.head_local)
        points.append(arm.matrix_world @ bone.tail_local)
    if not points:
        return None
    lo = Vector((min(p[i] for p in points) for i in range(3)))
    hi = Vector((max(p[i] for p in points) for i in range(3)))
    center = (lo + hi) * 0.5
    height = max(hi.y - lo.y, 0.1)

    cam_data = bpy.data.cameras.new("preview_cam")
    cam = bpy.data.objects.new("preview_cam", cam_data)
    bpy.context.collection.objects.link(cam)
    cam_data.lens = 70
    scene.camera = cam

    light_data = bpy.data.lights.new("preview_light", type="SUN")
    light_data.energy = 4.0
    light = bpy.data.objects.new("preview_light", light_data)
    bpy.context.collection.objects.link(light)
    light.location = center + Vector((1.0, 1.2, -1.0))
    look_at(light, center)

    shots = {}
    for name, direction in (("three_quarter", Vector((0.85, 0.0, -0.55)).normalized()),
                            ("side", Vector((0.0, 0.0, 1.0)))):
        cam.location = center + direction * (height * 2.0)
        look_at(cam, center)
        scene.render.filepath = path.replace(".png", "_%s.png" % name)
        bpy.ops.render.render(write_still=True)
        shots[name] = scene.render.filepath

    for obj in meshes:
        obj.hide_render = True
    scene.render.filepath = path.replace(".png", "_rig.png")
    bpy.ops.render.render(write_still=True)
    shots["rig"] = scene.render.filepath
    for obj in meshes:
        obj.hide_render = False

    # the preview rig is scaffolding: the saved file gets the character only
    bpy.data.objects.remove(cam, do_unlink=True)
    bpy.data.objects.remove(light, do_unlink=True)
    return {
        "bbox_min": [round(v, 4) for v in lo],
        "bbox_max": [round(v, 4) for v in hi],
        "height_y": round(hi.y - lo.y, 4),
        "width_x": round(hi.x - lo.x, 4),
        "depth_z": round(hi.z - lo.z, 4),
        "shots": shots,
    }


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--label", default="")
    parser.add_argument("--preview", default="")
    parser.add_argument("--report", default="")
    args = parser.parse_args(argv)

    mesh_io = plugin()
    clear_scene()

    workspace = args.workspace
    label = args.label or os.path.splitext(os.path.basename(args.out))[0]

    with open(os.path.join(workspace, "skeleton.json"), encoding="utf-8") as fh:
        bones = json.load(fh)["bones"]
    arm = mesh_io.build_armature(label, bones)

    with open(os.path.join(workspace, "workspace.json"), encoding="utf-8") as fh:
        manifest = json.load(fh)

    report = {
        "label": label,
        "out": args.out,
        "bones_total": len(bones),
        "bones_named": sum(1 for b in bones if not str(b.get("name", "")).startswith("bone_")),
        "armature_bones": len(arm.data.bones),
        "rig": rig_shape(arm, bones),
        "meshes": [],
    }
    problems, rigid = [], []

    for item in manifest.get("meshes", []):
        path = os.path.join(workspace, item["file"])
        if not os.path.isfile(path):
            problems.append("missing file: " + item["file"])
            continue
        obj, _arm, _meta = mesh_io.import_mesh(path, armature=arm)

        data = np.load(path, allow_pickle=False)
        weights = np.asarray(data["weights"], dtype=np.float64)
        bones_idx = np.asarray(data["bones"], dtype=np.int64)

        table = len(obj.vertex_groups)          # this mesh's own bone table
        rowsum = weights.sum(axis=1)
        used = int((np.abs(weights).sum(axis=1) > 1e-6).sum())
        is_rigid = used == 0
        peak = int(bones_idx.max()) if bones_idx.size else -1

        report["meshes"].append({
            "mesh": item.get("mesh", ""),
            "object": obj.name,
            "vertices": len(np.asarray(data["positions"], dtype=np.float32)),
            "polygons": len(obj.data.polygons),
            "vertex_groups": table,
            "influenced_vertices": used,
            "rigid": is_rigid,
            "weight_rowsum_min": round(float(rowsum.min()), 6) if rowsum.size else None,
            "weight_rowsum_max": round(float(rowsum.max()), 6) if rowsum.size else None,
            "max_bone_index": peak,
            "modifiers": [m.type for m in obj.modifiers],
        })

        if is_rigid:
            rigid.append(item.get("mesh", ""))
            continue
        if abs(float(rowsum.min()) - 1.0) > 0.02 or abs(float(rowsum.max()) - 1.0) > 0.02:
            problems.append("%s: weight rows do not sum to 1" % item.get("mesh"))
        if peak >= table:
            problems.append("%s: bone index %d outside its own table (%d)"
                            % (item.get("mesh"), peak, table))
        if not any(m.type == "ARMATURE" for m in obj.modifiers):
            problems.append("%s: no armature modifier" % item.get("mesh"))

    groups = set()
    for obj in bpy.data.objects:
        if obj.type == "MESH":
            groups.update(g.name for g in obj.vertex_groups)
    rig_names = set(b.name for b in arm.data.bones)
    report["groups"] = len(groups)
    report["groups_without_bone"] = sorted(groups - rig_names)
    if groups - rig_names:
        problems.append("vertex groups without a bone: %d" % len(groups - rig_names))
    report["rigid_meshes"] = rigid

    if args.preview:
        meshes = [o for o in bpy.data.objects if o.type == "MESH"]
        report["preview"] = preview(arm, meshes, args.preview)

    bpy.ops.wm.save_as_mainfile(filepath=args.out)
    report["saved"] = os.path.isfile(args.out)
    report["size_bytes"] = os.path.getsize(args.out) if report["saved"] else 0
    report["problems"] = problems

    if args.report:
        with open(args.report, "w", encoding="utf-8") as fh:
            json.dump(report, fh, ensure_ascii=False, indent=1)
    print("BLEND " + json.dumps(report, ensure_ascii=False))
    return 0 if not problems else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []))
