"""Merge the per-mesh bone tables of one skin into a single skeleton.

The game never ships "the character skeleton" with the meshes: every `Mesh` in
GF2 carries only the bones *it* skins (`m_BindPose` + `m_BoneNameHashes`), and
those subsets differ wildly — Alva's 12 meshes hold 3, 4, 8, 13, 26, 28, 32, 35,
46, 56 … bones while the union is 226.  Importing mesh-by-mesh therefore shows
only a slice of the rig (and binds every mesh to the *first* mesh's slice).

This module rebuilds the union: bones are identified by their CRC32 name hash
(the only identity the game ships) and their rest transform is taken from
whichever mesh declares them.  Two invariants make that sound:

* the same hash always carries the same rest transform
  (measured drift ≤ 1e-2 on Alva's 226 bones, tracked as `conflicts`), and
* the hash is `crc32(full transform path)`, so a hash found in a local Avatar
  or prefab also yields the real bone name *and* its ancestry.

The result is written next to the meshes as `skeleton.json` and embedded in
every `.exmtmesh` (so a single file still binds correctly on its own).
"""

from __future__ import annotations

import numpy as np

FORMAT = "exmt-skeleton-1"

#: bones whose real name is unknown are called `bone_<8 hex digits of the hash>`
BONE_PREFIX = "bone_"


def bone_name(hash_: int) -> str:
    return "%s%08X" % (BONE_PREFIX, int(hash_) & 0xFFFFFFFF)


def _paths_to_bones(paths: dict) -> tuple[dict, dict]:
    """`{hash: transform path}` -> (`{hash: leaf name}`, `{hash: parent hash}`).

    The hash is `crc32(full path)`, so the parent of a bone is the hash of its
    path minus the last segment.  Segments not present in `paths` simply leave
    the bone unparented.
    """
    import zlib

    def crc(text: str) -> int:
        return zlib.crc32(text.encode()) & 0xFFFFFFFF

    names = {}
    parents = {}
    for h, path in paths.items():
        text = str(path)
        leaf = text.rsplit("/", 1)[-1]
        names[int(h)] = leaf or bone_name(int(h))
        cut = text.rfind("/")
        if cut > 0:
            parents[int(h)] = crc(text[:cut])
    return names, parents


def merge(tables: list, tolerance: float = 0.01, paths: dict | None = None) -> dict:
    """Merge `[(mesh_name, hashes, bindpose), …]` into one skeleton.

    `bindpose` is the game's `m_BindPose` (world -> bone); the stored `rest` is
    its inverse (bone -> world), which is what a DCC rig wants.  `paths` is an
    optional `{hash: transform path}` dictionary for real names.
    """
    bones: list = []
    index_of: dict = {}
    local: dict = {}
    conflicts: list = []

    for mesh_name, hashes, bindpose in tables:
        rest = None
        if bindpose is not None and len(bindpose):
            rest = np.linalg.inv(np.asarray(bindpose, dtype=np.float64))
        mapping = []
        for i, h in enumerate(hashes):
            h = int(h)
            matrix = rest[i] if rest is not None and i < len(rest) else np.eye(4, dtype=np.float64)
            gi = index_of.get(h)
            if gi is None:
                gi = len(bones)
                index_of[h] = gi
                bones.append({
                    "hash": h,
                    "name": bone_name(h),
                    "parent": -1,
                    "rest": [float(v) for v in np.asarray(matrix, dtype=np.float64).reshape(-1)],
                    "meshes": [mesh_name],
                })
            else:
                prev = np.asarray(bones[gi]["rest"], dtype=np.float64).reshape(4, 4)
                dev = float(np.abs(prev - matrix).max())
                if dev > tolerance:
                    conflicts.append({"hash": h, "mesh": mesh_name, "deviation": round(dev, 6)})
                if mesh_name not in bones[gi]["meshes"]:
                    bones[gi]["meshes"].append(mesh_name)
            mapping.append(gi)
        local[mesh_name] = mapping

    named = 0
    if paths:
        names, parents = _paths_to_bones(paths)
        for b in bones:
            real = names.get(b["hash"])
            if real and not real.startswith(BONE_PREFIX):
                b["name"] = real
                named += 1
        for b in bones:
            parent_hash = parents.get(b["hash"])
            gi = index_of.get(parent_hash) if parent_hash is not None else None
            if gi is not None and gi != index_of[b["hash"]]:
                b["parent"] = gi
        _resolve_cycles(bones)
    _unique_names(bones)

    return {
        "format": FORMAT,
        "id": "",
        "count": len(bones),
        "named": named,
        "bones": bones,
        "local": local,
        "conflicts": conflicts,
    }


def _resolve_cycles(bones: list) -> None:
    """Drop parent links that would close a loop (defensive: paths can alias)."""
    for i, b in enumerate(bones):
        seen = {i}
        cur = b.get("parent", -1)
        while cur is not None and cur >= 0:
            if cur in seen:
                b["parent"] = -1
                break
            seen.add(cur)
            cur = bones[cur].get("parent", -1) if cur < len(bones) else -1


def _unique_names(bones: list) -> None:
    """Blender needs unique names, and truncates past 63 bytes."""
    used = set()
    for b in bones:
        base = (str(b.get("name") or "").strip() or bone_name(b["hash"]))[:56]
        name = base
        n = 1
        while name in used:
            n += 1
            name = "%s_%d" % (base, n)
        used.add(name)
        b["name"] = name


def rest_matrix(bone: dict) -> np.ndarray:
    return np.asarray(bone["rest"], dtype=np.float64).reshape(4, 4)


def for_file(skeleton: dict, mesh_name: str) -> dict:
    """The per-`.exmtmesh` view: skeleton id, full bone table, local index map."""
    return {
        "id": skeleton.get("id", ""),
        "count": int(skeleton.get("count", 0)),
        "named": int(skeleton.get("named", 0)),
        "bones": [{"hash": b["hash"], "name": b["name"], "parent": b["parent"], "rest": b["rest"]}
                  for b in skeleton["bones"]],
        "local": list(skeleton.get("local", {}).get(mesh_name, [])),
    }


def local_table(mesh_name: str, hashes: list, bindpose, paths: dict | None = None) -> dict:
    """Skeleton view for a file with no merged skeleton (single `extract`).

    `paths` is the same `{hash: path}` dictionary the workspace export uses, so a
    mesh exported on its own is named exactly like the merged skin skeleton.
    """
    rest = np.linalg.inv(np.asarray(bindpose, dtype=np.float64)) if bindpose is not None and len(bindpose) else None
    bones = []
    for i, h in enumerate(hashes):
        m = rest[i] if rest is not None and i < len(rest) else np.eye(4)
        bones.append({"hash": int(h), "name": bone_name(int(h)), "parent": -1,
                      "rest": [float(v) for v in np.asarray(m).reshape(-1)]})
    named = 0
    if paths:
        names, _parents = _paths_to_bones(paths)
        for b in bones:
            real = names.get(b["hash"])
            if real and not real.startswith(BONE_PREFIX):
                b["name"] = real
                named += 1
    _unique_names(bones)
    return {"id": mesh_name, "count": len(bones), "named": named, "bones": bones,
            "local": list(range(len(bones)))}
