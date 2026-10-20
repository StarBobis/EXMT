"""Build the **GameCache**: one analysis pass that everything else reads.

The app used to build its indexes on demand (`texindex` for a skin page,
`animidx` for the animation strip, `bonenames` for a workspace), which meant the
first visit to each page re-scanned the game folder. This module scans once and
writes a self-describing folder:

    <cache>/GameCache/
        manifest.json   what/where/when + counts (written **last**: its presence
                        is what makes the cache "complete")
        names.json      mesh / texture name -> bundles
        skins.json      skin -> parts (mesh per part) + counts
        clips.json      AnimationClip name -> bundles
        bones.json      bone path hash -> path (skeleton names)
        progress.json   live phase state, polled by the UI while this runs

Analysis is idempotent: run it again after a game update and every file is
replaced (each one atomically, manifest last).
"""

from __future__ import annotations

import json
import os
import re
import time

FORMAT = "exmt-gamecache-1"

#: `c_VepleySSR0102_slg_body_lod0` / `cw_GrozaSR0102_WL_slg_lod0`
MESH_NAME = re.compile(r"^(c|cw)_([A-Za-z0-9]+)_(.+)$")
LOD = re.compile(r"^(.*?)_lod(\d+)", re.IGNORECASE)


# --------------------------------------------------------------------------- #
# progress (polled by the UI; the command itself prints one JSON at the end)
# --------------------------------------------------------------------------- #
class Progress:
    """Writes `progress.json` after every phase so the UI can show a bar."""

    def __init__(self, path: str):
        self.path = path
        self.phases: list[dict] = []
        self.started = time.time()
        self._write("starting", None)

    def start(self, phase: str) -> None:
        self._write("running", phase)

    def done(self, phase: str, seconds: float, detail: dict | None = None) -> None:
        row = {"phase": phase, "seconds": round(seconds, 1)}
        row.update(detail or {})
        self.phases.append(row)
        self._write("running", None)

    def finish(self, error: str | None = None) -> None:
        self._write("error" if error else "done", None, error)

    def _write(self, state: str, phase: str | None, error: str | None = None) -> None:
        payload = {
            "state": state,
            "phase": phase,
            "phases": self.phases,
            "seconds": round(time.time() - self.started, 1),
        }
        if error:
            payload["error"] = error
        try:
            os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
            tmp = self.path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as fh:
                json.dump(payload, fh, ensure_ascii=False)
            os.replace(tmp, self.path)
        except OSError:
            pass


def _atomic_json(path: str, payload: dict) -> None:
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False)
    os.replace(tmp, path)


# --------------------------------------------------------------------------- #
# skins: the roster comes from the game, not from a hand-written catalogue
# --------------------------------------------------------------------------- #
def skins_of(names: dict, clips: dict | None = None, textures: dict | None = None) -> list:
    """`[{id, name, variant, bundle, parts, part_bundles, texture_count, clip_count}]`.

    A skin is derived from its mesh names (`c_<skin>_<part>`), so whatever the
    game ships shows up — a new character in an update needs no code change.
    Only the lowest LOD of each part is kept (the studio edits one mesh per part).
    """
    skins: dict[str, dict] = {}
    for mesh, bundles in (names.get("meshes") or {}).items():
        match = MESH_NAME.match(mesh)
        if not match:
            continue
        prefix, name, rest = match.groups()
        lod = LOD.match(rest)
        part = (lod.group(1) if lod else rest).lower()
        if part.startswith("emitter"):
            # particle/attachment markers ship as Mesh objects but have no skinning
            continue
        key = "c_" + name
        skin = skins.setdefault(key, {
            "id": key,
            "name": name,
            "variant": "dorm" if name.lower().endswith("dorm") else "slg",
            "bundle": "",
            "parts": {},
            "part_bundles": {},
            "_lods": {},
            "_bundles": [],
        })
        index = int(lod.group(2)) if lod else 0
        previous = skin["_lods"].get(part)
        if previous is not None and previous <= index:
            continue
        skin["_lods"][part] = index
        skin["parts"][part] = mesh
        skin["part_bundles"][part] = list(bundles)
        for hash_ in bundles:
            if hash_ not in skin["_bundles"]:
                skin["_bundles"].append(hash_)

    textures = textures or {}
    clip_table = clips or {}
    for key, skin in skins.items():
        prefix = key + "_"
        weapon = "cw_" + skin["name"] + "_"
        skin["texture_count"] = sum(
            1 for name in textures if name.startswith(prefix) or name.startswith(weapon)
        )
        skin["clip_count"] = sum(
            1 for name in clip_table if name.startswith(prefix) or name.startswith(weapon)
        )
        bundles = skin.pop("_bundles")
        skin.pop("_lods")
        # the bundle most of its meshes live in: what the studio opens first
        skin["bundle"] = max(bundles, key=bundles.count) if bundles else ""
        skin["mesh_count"] = len(skin["parts"])

    rows = list(skins.values())
    rows.sort(key=lambda row: row["name"].lower())
    return rows


# --------------------------------------------------------------------------- #
# the analysis itself
# --------------------------------------------------------------------------- #
def analyze(bundle_dir: str, cache_root: str, workers: int | None = None,
            limit: int | None = None, with_bones: bool = True) -> dict:
    """Scan `bundle_dir` once and (re)write `<cache_root>/GameCache`."""
    from . import animidx, meshidx
    from . import bonenames

    if not os.path.isdir(bundle_dir):
        raise SystemExit("游戏资源目录不存在：%s" % bundle_dir)
    bundles = [name for name in os.listdir(bundle_dir) if name.endswith(".bundle")]
    if not bundles:
        raise SystemExit("游戏资源目录里没有 .bundle 文件：%s" % bundle_dir)

    out_dir = os.path.join(cache_root, "GameCache")
    os.makedirs(out_dir, exist_ok=True)
    # a manifest from an older analysis must not survive a failed rebuild
    manifest_path = os.path.join(out_dir, "manifest.json")
    if os.path.isfile(manifest_path):
        os.replace(manifest_path, manifest_path + ".old")

    progress = Progress(os.path.join(out_dir, "progress.json"))
    total = time.time()
    workers = workers or 0
    try:
        progress.start("names")
        started = time.time()
        summary = meshidx.build(bundle_dir, os.path.join(out_dir, "names.json"),
                                limit or 0, workers or 6)
        progress.done("names", time.time() - started, {
            "scanned": summary.get("bundles"),
            "meshes": summary.get("mesh_count"),
            "textures": summary.get("texture_count"),
        })

        names = _read(os.path.join(out_dir, "names.json"))
        clips_table = {}
        clips_path = os.path.join(out_dir, "clips.json")
        progress.start("clips")
        started = time.time()
        clip_summary = animidx.build(bundle_dir, clips_path, workers or 6, limit)
        clips_table = _read(clips_path).get("clips") or {}
        progress.done("clips", time.time() - started, {
            "clipped": clip_summary.get("clip_count") or len(clips_table),
        })

        progress.start("skins")
        started = time.time()
        skins = skins_of(names, clips_table, names.get("textures") or {})
        _atomic_json(os.path.join(out_dir, "skins.json"), {
            "format": "exmt-skins-1",
            "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "skins": skins,
        })
        progress.done("skins", time.time() - started, {"skins": len(skins)})

        bones_count = 0
        if with_bones:
            progress.start("bones")
            started = time.time()
            bone_summary = bonenames.build(bundle_dir, os.path.join(out_dir, "bones.json"),
                                           limit, workers or None)
            bones_count = bone_summary.get("names") or 0
            progress.done("bones", time.time() - started, {"bones": bones_count})

        manifest = {
            "format": FORMAT,
            "built_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "bundle_dir": os.path.abspath(bundle_dir),
            "cache_dir": os.path.abspath(out_dir),
            "bundles": len(bundles),
            "limited_to": limit,
            "workers": workers or 6,
            "counts": {
                "meshes": len(names.get("meshes") or {}),
                "textures": len(names.get("textures") or {}),
                "clips": len(clips_table),
                "skins": len(skins),
                "bones": bones_count,
            },
            "seconds": round(time.time() - total, 1),
        }
        # manifest last: its presence is what says "this cache is complete"
        _atomic_json(manifest_path, manifest)
        progress.finish()
        return manifest
    except SystemExit:
        raise
    except BaseException as exc:  # noqa: BLE001 - report, then let the CLI exit non-zero
        progress.finish(str(exc))
        raise


def _read(path: str) -> dict:
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def status(cache_root: str) -> dict:
    """`{state, manifest}` — `ready` only when the manifest parses and matches."""
    out_dir = os.path.join(cache_root, "GameCache")
    manifest_path = os.path.join(out_dir, "manifest.json")
    if not os.path.isfile(manifest_path):
        stale = os.path.isfile(manifest_path + ".old")
        return {"state": "incomplete" if stale else "missing", "manifest": None}
    try:
        manifest = _read(manifest_path)
    except Exception:
        return {"state": "damaged", "manifest": None}
    if manifest.get("format") != FORMAT:
        return {"state": "outdated", "manifest": manifest}
    missing = [name for name in ("names.json", "skins.json", "clips.json")
               if not os.path.isfile(os.path.join(out_dir, name))]
    return {"state": "ready" if not missing else "incomplete",
            "manifest": manifest, "missing": missing}
