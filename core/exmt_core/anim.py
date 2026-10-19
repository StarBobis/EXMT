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
def export(clip: Clip, out_path: str, paths: dict | None = None, rate: float | None = None) -> dict:
    """Write sampled tracks as JSON; returns the payload (without the sample rows)."""
    rows, times = clip.tracks(rate=rate, paths=paths)
    payload = {
        "format": FORMAT,
        "name": clip.name,
        "source_clip": clip.name,
        "sample_rate": float(rate or clip.sample_rate),
        "start": clip.start,
        "stop": clip.stop,
        "length": clip.length,
        "loop": clip.loop,
        "events": clip.events,
        "times": [round(t, 6) for t in times],
        "bones": rows,
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(payload, fh)
    return {
        "format": FORMAT,
        "name": clip.name,
        "file": out_path,
        "samples": len(times),
        "bones": len(rows),
        "bytes": _size(out_path),
    }


def _size(path: str) -> int:
    import os

    try:
        return os.path.getsize(path)
    except OSError:
        return 0


def load(path: str) -> dict:
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def read_clips(bundle_path: str) -> list:
    """Every AnimationClip in one bundle, decoded (`[(Clip, tree)]`)."""
    import UnityPy

    from .bundle import decrypt

    with open(bundle_path, "rb") as fh:
        env = UnityPy.load(decrypt(fh.read()))
    out = []
    for obj in env.objects:
        if obj.type.name != "AnimationClip":
            continue
        try:
            tree = obj.read_typetree()
        except Exception:
            continue
        try:
            out.append(Clip(tree))
        except Exception:
            continue
    out.sort(key=lambda clip: clip.name)
    return out
