"""AnimationClip decoding: Unity's flat curve space -> per-bone TRS tracks.

A built clip keeps its runtime animation data in three containers plus one flat
curve index space (verified on `c_GrozaSR0102_slg_LowCoverRtoL_N`, 62 frames,
623 dense curves, 775 bindings):

  * `m_MuscleClip.m_Clip.m_StreamedClip` — sparse cubic keys; every key names a
    flat curve index and four polynomial coefficients, evaluated as
    `((a*dt + b)*dt + c)*dt + d` with `dt = t - key.time`;
  * `m_Clip.m_DenseClip` — `m_FrameCount` frames of `m_CurveCount` samples,
    frame-major, starting at `m_BeginTime` with `m_SampleRate`;
  * `m_ConstantClip.data` — one value for every curve that never changes.

`m_ClipBindingConstant.genericBindings` describes that flat space *in order*: a
Transform binding (`typeID == 4`) owns 3 curves for `attribute == 1`
(localPosition), 4 for `2` (localRotation, quaternion), 3 for `3` (localScale)
and 3 for `4` (localEulerAnglesRaw); every other binding owns exactly one curve.
(`AnimationClipBindingConstant.FindBinding` in AssetStudio is the reference.)
`path` is `crc32(transform path)` — the same hash GF2 meshes use for their bones,
so `bonenames.json` turns it back into a readable bone path, and a clip can drive
the merged workspace skeleton directly.
"""

from __future__ import annotations

import json
import math
import struct
from array import array

#: curves owned by a Transform binding, keyed by its attribute
ATTR_CURVES = {1: 3, 2: 4, 3: 3, 4: 3}
ATTR_NAMES = {1: "position", 2: "rotation", 3: "scale", 4: "euler"}
TRANSFORM = 4
FORMAT = "exmt-anim-1"


def _streamed_frames(data) -> list:
    """`[(time, {curveIndex: (a, b, c, d)})]` from the streamed clip's uint32 payload."""
    if not data:
        return []
    raw = array("I", data).tobytes()
    end = len(raw)
    pos = 0
    frames = []
    while pos + 8 <= end:
        t = struct.unpack_from("<f", raw, pos)[0]
        pos += 4
        count = struct.unpack_from("<i", raw, pos)[0]
        pos += 4
        if count < 0 or pos + count * 20 > end:
            break
        keys = {}
        for _ in range(count):
            index = struct.unpack_from("<i", raw, pos)[0]
            keys[index] = struct.unpack_from("<4f", raw, pos + 4)
            pos += 20
        frames.append((t, keys))
    return frames


def bindings_of(tree: dict) -> list:
    """`[{path, attribute, type_id, base, count}]` in flat curve-index order."""
    raw = (tree.get("m_ClipBindingConstant") or {}).get("genericBindings") or []
    out = []
    base = 0
    for entry in raw:
        type_id = int(entry.get("typeID") or 0)
        attribute = int(entry.get("attribute") or 0)
        count = ATTR_CURVES.get(attribute, 1) if type_id == TRANSFORM else 1
        out.append(
            {
                "path": int(entry.get("path") or 0),
                "attribute": attribute,
                "type_id": type_id,
                "base": base,
                "count": count,
            }
        )
        base += count
    return out


class Clip:
    """One decoded AnimationClip; `value(index, t)` samples the flat curve space."""

    def __init__(self, tree: dict):
        self.tree = tree
        self.name = str(tree.get("m_Name") or "")
        self.sample_rate = float(tree.get("m_SampleRate") or 30.0)
        muscle = tree.get("m_MuscleClip") or {}
        self.start = float(muscle.get("m_StartTime") or 0.0)
        self.stop = float(muscle.get("m_StopTime") or 0.0)
        self.loop = bool(muscle.get("m_LoopTime"))
        self.start_x = muscle.get("m_StartX") or {}
        self.stop_x = muscle.get("m_StopX") or {}
        speed = muscle.get("m_AverageSpeed") or {}
        self.average_speed = (speed.get("x", 0.0), speed.get("y", 0.0), speed.get("z", 0.0))
        self.events = [
            {"time": float(e.get("time") or 0.0), "name": str(e.get("functionName") or "")}
            for e in (tree.get("m_Events") or [])
        ]

        clip = ((muscle.get("m_Clip") or {}).get("data") or {})
        streamed = clip.get("m_StreamedClip") or {}
        dense = clip.get("m_DenseClip") or {}
        const = clip.get("m_ConstantClip") or {}
        self.streamed_count = int(streamed.get("curveCount") or 0)
        self.frames = _streamed_frames(streamed.get("data") or [])
        self.dense_frames = int(dense.get("m_FrameCount") or 0)
        self.dense_count = int(dense.get("m_CurveCount") or 0)
        self.dense_rate = float(dense.get("m_SampleRate") or self.sample_rate)
        self.dense_begin = float(dense.get("m_BeginTime") or 0.0)
        self.dense = dense.get("m_SampleArray") or []
        self.const = const.get("data") or []
        self.bindings = bindings_of(tree)
        self._keys = None

    # ------------------------------------------------------------------ values
    @property
    def length(self) -> float:
        return max(0.0, self.stop - self.start)

    def _stream_keys(self) -> dict:
        if self._keys is None:
            table: dict = {}
            for t, keys in self.frames:
                for index, coeff in keys.items():
                    table.setdefault(index, []).append((t, coeff))
            for row in table.values():
                row.sort(key=lambda item: item[0])
            self._keys = table
        return self._keys

    def _streamed_value(self, index: int, t: float) -> float:
        row = self._stream_keys().get(index)
        if not row:
            return 0.0
        if t <= row[0][0]:
            return row[0][1][3]
        if t >= row[-1][0]:
            return row[-1][1][3]
        lo, hi = 0, len(row) - 1
        while hi - lo > 1:
            mid = (lo + hi) // 2
            if row[mid][0] <= t:
                lo = mid
            else:
                hi = mid
        key_t, (a, b, c, d) = row[lo]
        dt = t - key_t
        return ((a * dt + b) * dt + c) * dt + d

    def _dense_value(self, index: int, t: float) -> float:
        x = (t - self.dense_begin) * self.dense_rate
        span = self.dense_count
        if x <= 0:
            return self.dense[index]
        frame = int(x)
        if frame >= self.dense_frames - 1:
            return self.dense[(self.dense_frames - 1) * span + index]
        frac = x - frame
        a = self.dense[frame * span + index]
        b = self.dense[(frame + 1) * span + index]
        return a + (b - a) * frac

    def value(self, index: int, t: float) -> float:
        if index < 0:
            return 0.0
        if index < self.streamed_count:
            return self._streamed_value(index, t)
        if index < self.streamed_count + self.dense_count:
            if not self.dense_frames:
                return 0.0
            return self._dense_value(index - self.streamed_count, t)
        j = index - self.streamed_count - self.dense_count
        return float(self.const[j]) if 0 <= j < len(self.const) else 0.0

    # ------------------------------------------------------------------ tracks
    def tracks(self, rate: float | None = None, paths: dict | None = None) -> list:
        """Per-bone sampled tracks; `paths` maps `crc32(transform path)` -> path."""
        rate = float(rate or self.sample_rate or 30.0)
        steps = max(2, int(math.floor(self.length * rate)) + 1)
        times = [self.start + i / rate for i in range(steps)]
        by_path: dict = {}
        for binding in self.bindings:
            if binding["type_id"] != TRANSFORM:
                continue
            slot = by_path.setdefault(binding["path"], {})
            name = ATTR_NAMES.get(binding["attribute"])
            if name is None:
                continue
            rows = [
                [self.value(binding["base"] + c, t) for t in times]
                for c in range(binding["count"])
            ]
            slot[name] = rows
        out = []
        for path_hash, slot in by_path.items():
            row = {
                "path": (paths or {}).get(path_hash, ""),
                "hash": path_hash,
                "position": slot.get("position"),
                "rotation": slot.get("rotation"),
                "scale": slot.get("scale"),
                "euler": slot.get("euler"),
            }
            out.append(row)
        out.sort(key=lambda item: item["path"] or "~%d" % item["hash"])
        return out, times

    def summary(self, paths: dict | None = None) -> dict:
        bones = {b["path"] for b in self.bindings if b["type_id"] == TRANSFORM}
        named = sum(1 for h in bones if (paths or {}).get(h))
        return {
            "name": self.name,
            "sample_rate": self.sample_rate,
            "start": self.start,
            "stop": self.stop,
            "length": round(self.length, 4),
            "loop": self.loop,
            "events": len(self.events),
            "bindings": len(self.bindings),
            "bones": len(bones),
            "bones_named": named,
            "curves": sum(b["count"] for b in self.bindings),
            "streamed_curves": self.streamed_count,
            "dense_curves": self.dense_count,
            "dense_frames": self.dense_frames,
            "constant_curves": len(self.const),
        }


# ---------------------------------------------------------------------- export
def export(clip: Clip, out_path: str, paths: dict | None = None, rate: float | None = None,
           decimals: int = 5, parents: dict | None = None,
           skeleton_hashes: set | None = None) -> dict:
    """Write sampled tracks as JSON; returns the payload (without the sample rows).

    Sampled, not raw: a clip is a *reference* to look at skinning in Blender, so
    linear samples at `rate` (default 30 Hz — half the game's 60 Hz and half the
    file) are enough, and keep the file readable instead of re-implementing
    Unity's cubic keys in the add-on.  `decimals` trims float noise: rotation
    numbers below 1e-5 are not visible on screen.

    `parents` (`{path hash: parent hash}` from `hierarchy`) goes into every row:
    the add-on has to compose world transforms down the chain to pose a bone, and
    the hierarchy is nowhere in the clip itself.
    """
    rate = float(rate or min(clip.sample_rate or 30.0, 30.0))
    rows, times = clip.tracks(rate=rate, paths=paths)
    if decimals is not None:
        rows = [_round_row(row, decimals) for row in rows]
    if parents:
        for row in rows:
            row["parent"] = int(parents.get(row["hash"], 0))
    payload = {
        "format": FORMAT,
        "name": clip.name,
        "source_clip": clip.name,
        "sample_rate": rate,
        "start": clip.start,
        "stop": clip.stop,
        "length": clip.length,
        "loop": clip.loop,
        "events": clip.events,
        "times": [round(t, 6) for t in times],
        "bones": rows,
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, separators=(",", ":"))
    return {
        "format": FORMAT,
        "name": clip.name,
        "file": out_path,
        "samples": len(times),
        "bones": len(rows),
        # how many of the clip's bones exist in the workspace skeleton: a clip from
        # another body variant (or an export that only ticked one submesh) drives
        # bones the rig does not have, and the pose then looks broken in Blender
        "matched": (len([r for r in rows if int(r["hash"]) in skeleton_hashes])
                    if skeleton_hashes is not None else None),
        "bytes": _size(out_path),
    }


def _round_row(row: dict, decimals: int) -> dict:
    out = dict(row)
    for key in ("position", "rotation", "scale", "euler"):
        rows = row.get(key)
        if rows:
            out[key] = [[round(v, decimals) for v in axis] for axis in rows]
    return out


def _size(path: str) -> int:
    import os

    try:
        return os.path.getsize(path)
    except OSError:
        return 0


def load(path: str) -> dict:
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def hierarchy(env) -> tuple:
    """`({path hash: parent hash}, {path hash: path})` for one bundle's prefab.

    The clip's `genericBindings` only carry `crc32(transform path)`; the bone
    hierarchy is not in the clip at all.  It *is* in the character prefab that
    ships in the same bundle (634 `Transform` + `GameObject` objects for Groza),
    and the clip's path is a **suffix** of the prefab's path: the prefab starts at
    the model object (`CF_GrozaSR0102_slg_skin/root/Root_M/…`) while the clip is
    relative to the Animator (`root/Root_M/…`).  So every path is registered with
    every suffix, exactly like `bonenames._add_path`.

    Measured on `c_GrozaSR0102_slg_LowCoverRtoL_N`: 257 of 258 animated bones get
    a parent (and a readable path) this way, while the name index alone knows only
    ~126 of them because it stops after each bundle's metadata prefix.
    """
    import zlib

    names: dict = {}
    father: dict = {}
    owner: dict = {}
    for obj in env.objects:
        kind = obj.type.name
        if kind == "GameObject":
            try:
                names[obj.path_id] = (obj.read_typetree() or {}).get("m_Name") or ""
            except Exception:
                continue
        elif kind == "Transform":
            try:
                tree = obj.read_typetree() or {}
            except Exception:
                continue
            father[obj.path_id] = (tree.get("m_Father") or {}).get("m_PathID", 0)
            owner[obj.path_id] = (tree.get("m_GameObject") or {}).get("m_PathID", 0)

    def crc(text: str) -> int:
        return zlib.crc32(text.encode()) & 0xFFFFFFFF

    path_of: dict = {}
    for tid in father:
        parts, cur, guard = [], tid, 0
        while cur and cur in father and guard < 64:
            parts.append(names.get(owner.get(cur), ""))
            cur = father[cur]
            guard += 1
        path = "/".join(reversed([p for p in parts if p]))
        if path:
            path_of[tid] = path

    parents: dict = {}
    paths: dict = {}
    # one hash can be registered under several trim depths; the shallowest (root
    # aligned) registration knows the parent, deeper ones cut it off.  Preferring
    # "has a parent" over "does not" is what keeps `Weapon/bolt02` under `Weapon`
    # instead of turning five bones into bogus roots that displace their subtrees.
    depth: dict = {}
    for tid, path in path_of.items():
        parent_path = path_of.get(father.get(tid, 0), "")
        parent_parts = parent_path.split("/") if parent_path else []
        parts = path.split("/")
        for start in range(len(parts)):
            suffix = "/".join(parts[start:])
            if not suffix:
                continue
            parent_suffix = "/".join(parent_parts[start:]) if len(parent_parts) > start else ""
            h = crc(suffix)
            candidate = crc(parent_suffix) if parent_suffix else 0
            prev_parent = parents.get(h)
            if prev_parent is None or (candidate and not prev_parent) or (
                    start < depth.get(h, 1 << 30) and bool(candidate) >= bool(prev_parent)):
                parents[h] = candidate
                depth[h] = start
            paths.setdefault(h, suffix)
    return parents, paths


def open_clips(bundle_path: str):
    """`(env, {clip name: object})` for one bundle — names only, no typetree reads.

    `read_clips` decodes every clip in the bundle (146 of them, ~1 MB of typetree
    each, 60 MB of payload), which is right for a listing but far too much when
    the user exported three clips.  This keeps the bundle open and names cheap;
    `clip_of` then reads exactly one typetree.
    """
    import UnityPy

    from .bundle import decrypt

    with open(bundle_path, "rb") as fh:
        env = UnityPy.load(decrypt(fh.read()))
    table = {}
    for obj in env.objects:
        if obj.type.name != "AnimationClip":
            continue
        try:
            name = obj.peek_name()
        except Exception:
            continue
        if name:
            table[str(name)] = obj
    return env, table


def clip_of(table: dict, name: str):
    """Decode one clip out of an `open_clips` table (None when it is not there)."""
    obj = (table or {}).get(name)
    if obj is None:
        return None
    try:
        return Clip(obj.read_typetree())
    except Exception:
        return None


def read_clips(bundle_path: str) -> list:
    """Every AnimationClip in one bundle, decoded (`[Clip]`)."""
    env, table = open_clips(bundle_path)
    out = []
    for name in table:
        clip = clip_of(table, name)
        if clip is not None:
            out.append(clip)
    out.sort(key=lambda clip: clip.name)
    return out
