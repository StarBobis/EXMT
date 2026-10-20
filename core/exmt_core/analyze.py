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
import shutil
import time
from concurrent.futures import ProcessPoolExecutor

FORMAT = "exmt-gamecache-1"

#: `c_VepleySSR0102_slg_body_lod0` / `cw_GrozaSR0102_WL_slg_lod0`
MESH_NAME = re.compile(r"^(c|cw)_([A-Za-z0-9]+)_(.+)$")
#: `_lod0` / `_lodm0` (mobile) and whatever follows (`_Dorm`, `_Fight`, `_trans`)
LOD = re.compile(r"^(.*?)_lod(m?)(\d+)(.*)$", re.IGNORECASE)


# --------------------------------------------------------------------------- #
# progress (polled by the UI; the command itself prints one JSON at the end)
# --------------------------------------------------------------------------- #
class Progress:
    """Writes `progress.json` after every phase so the UI can show a bar."""

    def __init__(self, path: str):
        self.path = path
        self.phases: list[dict] = []
        self.started = time.time()
        self.live: dict | None = None
        self._write("starting", None)

    def start(self, phase: str) -> None:
        self.live = None
        self._write("running", phase)

    def phase_progress(self, live: dict) -> None:
        """Mid-phase counters (e.g. packs 87/249) so the bar moves."""
        self.live = live
        self._write("running", live.get("phase"), None, live)

    def done(self, phase: str, seconds: float, detail: dict | None = None) -> None:
        row = {"phase": phase, "seconds": round(seconds, 1)}
        row.update(detail or {})
        self.phases.append(row)
        self.live = None
        self._write("running", None)

    def finish(self, error: str | None = None) -> None:
        self._write("error" if error else "done", None, error)

    def _write(self, state: str, phase: str | None, error: str | None = None,
               live: dict | None = None) -> None:
        payload = {
            "state": state,
            "phase": phase,
            "phases": self.phases,
            "seconds": round(time.time() - self.started, 1),
        }
        current = live if live is not None else self.live
        if current:
            payload["live"] = current
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
        # The part is everything *except* the lod token, but the suffix after it
        # stays: `..._slg_cloth_lod0` and `..._slg_cloth_lod0_Dorm` are two
        # different meshes (dorm outfit vs. combat outfit), not two LODs of one.
        # Merging them dropped real meshes — 比约卡 lost its whole combat cloth
        # and rendered half-naked with 3 submeshes instead of 8.
        if lod:
            part = (lod.group(1) + lod.group(4)).lower()
        else:
            part = rest.lower()
        if "emitter" in part:
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
        # keep the best variant of each part: lowest lod, and on a tie the full
        # (non-`lodm`) mesh, because `lodm0` is the mobile decimation
        mobile = bool(lod and lod.group(2))
        index = int(lod.group(3)) if lod else 0
        rank = (index, 1 if mobile else 0)
        previous = skin["_lods"].get(part)
        if previous is not None and previous <= rank:
            continue
        skin["_lods"][part] = rank
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
# skin packs: decode every roster skin now, so the studio never decodes again
# --------------------------------------------------------------------------- #
VARIANTS = {"dorm", "fight"}


def _shape(name: str) -> tuple[str, list[str]]:
    """`c_X_slg_P2_cloth2_lod0_Dorm` → ('cloth', ['p2', 'cloth']).

    The variant words (`Dorm`, `Fight`) say which outfit *state* a part belongs
    to; they are not part of the semantic head used for texture pairing.
    """
    tail = name
    match = re.search(r"_slg_(.+)$", tail)
    if match:
        tail = match.group(1)
    words = [w for w in re.split(r"[_\s]+", tail) if w and not re.match(r"(?i)^lodm?\d+$", w)]
    words = [w for w in words if w.lower() not in VARIANTS]
    tokens = [re.sub(r"\d+$", "", w.lower()) for w in words]
    return (tokens[-1] if tokens else ""), tokens


def _diffuse_name(mesh: str, textures: dict, prefix: str) -> str | None:
    """`c_X_slg_cloth_lod0` → `c_X_slg_cloth1_d` (or `_da`, `_dif`).

    1. exact `stem + _d/_da/_dif`
    2. same character family (`c_BastiSSR0101` may use `c_BastiSSR01_...`) and the
       same semantic head (`cloth` / `body` / `hair` / `head` …). Ranking prefers
       the same P-group, then the longest shared prefix — so `cloth2_Dorm` still
       picks `cloth2_da` instead of `cloth1_d`.
    3. nothing; the caller falls back to the skin's dominant atlas.
    """
    stem = re.sub(r"_lod\d+$", "", mesh, flags=re.I)
    for suffix in ("_d", "_da", "_dif"):
        candidate = stem + suffix
        if candidate in textures:
            return candidate

    head, tokens = _shape(mesh)
    if not head:
        return None
    best: str | None = None
    best_key = (-1, -1)
    for name in textures:
        match = re.match(r"^(.*?)_(d|da|dif)$", name, re.I)
        if not match:
            continue
        texture_stem = match.group(1)
        shared = 0
        for a, b in zip(texture_stem, stem):
            if a != b:
                break
            shared += 1
        if shared < 10:
            continue
        other_head, other_tokens = _shape(texture_stem)
        if other_head != head:
            continue
        same_group = 1 if tokens[:1] == other_tokens[:1] else 0
        key = (same_group, shared)
        if key > best_key:
            best_key = key
            best = name
    return best


def _pack_spec(out_dir: str, bundle_dir: str, names: dict, skin: dict,
               built_at: str) -> dict:
    """Everything `skinpack.build` needs for one skin, from the cache itself."""
    key = skin["id"]
    prefix = key + "_"
    mesh_items = []
    for part, mesh in (skin.get("parts") or {}).items():
        mesh_items.append({"mesh": mesh, "bundles": list((skin.get("part_bundles") or {}).get(part) or [])})

    texture_groups: dict[str, list] = {}
    for name, bundles in (names.get("textures") or {}).items():
        if not name.startswith(prefix) or not bundles:
            continue
        texture_groups.setdefault(bundles[0], []).append(name)

    textures = names.get("textures") or {}
    maps = []
    for mesh in (skin.get("parts") or {}).values():
        texture = _diffuse_name(mesh, textures, prefix)
        if texture:
            maps.append({"mesh": mesh, "texture": texture, "bundle": textures[texture][0]})

    # Some parts (the base `body`, `hair`, …) have no same-named texture at all:
    # the outfit atlas covers them through shared UVs. Give those the atlas that
    # the rest of the skin already uses, instead of leaving them flat grey.
    if maps:
        counts = {}
        for item in maps:
            counts[item["texture"]] = counts.get(item["texture"], 0) + 1
        dominant = max(counts.items(), key=lambda kv: kv[1])[0]
        used = {item["mesh"] for item in maps}
        for mesh in (skin.get("parts") or {}).values():
            if mesh in used:
                continue
            maps.append({"mesh": mesh, "texture": dominant, "bundle": textures[dominant][0]})

    return {
        "key": key,
        "dir": os.path.join(out_dir, "skins", key),
        "bundle_dir": bundle_dir,
        "meshes": mesh_items,
        "textures": [{"bundle": h, "names": v} for h, v in texture_groups.items()],
        "maps": maps,
        "cache_built_at": built_at,
        "thumb_size": 160,
        "map_size": 1024,
    }


def _pack_one(spec: dict) -> dict:
    from . import skinpack

    try:
        return skinpack.build(spec)
    except Exception as exc:  # noqa: BLE001 - one bad skin must not sink the analysis
        return {"key": spec.get("key"), "error": str(exc)[:200]}


def _roster_skins(skins: list) -> list:
    """The skins the studio can open: one `slg_` part, or a dorm skin."""
    out = []
    for skin in skins:
        parts = skin.get("parts") or {}
        if not parts:
            continue
        if any(part.startswith("slg") for part in parts) or \
                skin.get("variant") == "dorm":
            out.append(skin)
    return out


def _build_packs(out_dir: str, bundle_dir: str, names: dict, skins: list,
                 progress, workers: int, limit: int | None,
                 built_at: str) -> int:
    """Decode every roster skin into `<cache>/skins/<key>/` and report progress.

    `built_at` **must** be the same stamp the manifest gets: the studio only
    accepts a pack whose `cache_built_at` matches the manifest, which is how a
    re-analysis invalidates all the old packs in one go.
    """
    rows = _roster_skins(skins)
    if not rows:
        return 0
    specs = [_pack_spec(out_dir, bundle_dir, names, row, built_at) for row in rows]
    if limit:
        specs = specs[: max(1, limit)]

    done = 0
    total = len(specs)
    started = time.time()
    progress.start("packs")
    ok = 0
    with ProcessPoolExecutor(max_workers=max(1, min(workers, 8))) as pool:
        for result in pool.map(_pack_one, specs, chunksize=1):
            done += 1
            if not result.get("error"):
                ok += 1
            if done == 1 or done % 5 == 0 or done == total:
                progress.phase_progress({
                    "phase": "packs",
                    "done": done,
                    "total": total,
                    "ok": ok,
                    "seconds": round(time.time() - started, 1),
                })
    progress.done("packs", time.time() - started, {"packs": ok, "skins": total})
    return ok


# --------------------------------------------------------------------------- #
# the analysis itself
# --------------------------------------------------------------------------- #
def analyze(bundle_dir: str, cache_root: str, workers: int | None = None,
            limit: int | None = None, with_bones: bool = True,
            with_packs: bool = True) -> dict:
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
    # skin packs are tied to one analysis: start from a clean tree
    if with_packs:
        shutil.rmtree(os.path.join(out_dir, "skins"), ignore_errors=True)

    progress = Progress(os.path.join(out_dir, "progress.json"))
    total = time.time()
    # one stamp for the whole run: the manifest and every skin pack carry it, so
    # "pack belongs to this analysis" is a single string comparison
    built_at = time.strftime("%Y-%m-%dT%H:%M:%S")
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
            "built_at": built_at,
            "skins": skins,
        })
        progress.done("skins", time.time() - started, {"skins": len(skins)})

        # Decode every roster skin here, not on the studio's first visit: the
        # user's requirement is "switch to another skin = instant".
        pack_count = 0
        if with_packs:
            pack_count = _build_packs(out_dir, bundle_dir, names, skins, progress,
                                      workers or 6, limit, built_at)

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
            "built_at": built_at,
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
                "packs": pack_count,
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


def rebuild_packs(cache_root: str, workers: int | None = None,
                  limit: int | None = None) -> dict:
    """Re-create `skins.json` + every skin pack **without** rescanning bundles.

    Useful when the part/LOD rules change: the expensive name scan is reused and
    the manifest stamp is kept, so the studio accepts the fresh packs.
    """
    out_dir = os.path.join(cache_root, "GameCache")
    manifest_path = os.path.join(out_dir, "manifest.json")
    if not os.path.isfile(manifest_path):
        raise SystemExit("还没有分析结果，先跑 analyze：%s" % out_dir)
    manifest = _read(manifest_path)
    names = _read(os.path.join(out_dir, "names.json"))
    clips = (_read(os.path.join(out_dir, "clips.json")).get("clips") or {})
    built_at = manifest.get("built_at") or time.strftime("%Y-%m-%dT%H:%M:%S")

    progress = Progress(os.path.join(out_dir, "progress.json"))
    skins = skins_of(names, clips, names.get("textures") or {})
    _atomic_json(os.path.join(out_dir, "skins.json"), {
        "format": "exmt-skins-1", "built_at": built_at, "skins": skins,
    })
    shutil.rmtree(os.path.join(out_dir, "skins"), ignore_errors=True)
    pack_count = _build_packs(out_dir, manifest["bundle_dir"], names, skins, progress,
                              workers or 6, limit, built_at)
    manifest.setdefault("counts", {})["packs"] = pack_count
    manifest["counts"]["skins"] = len(skins)
    _atomic_json(manifest_path, manifest)
    progress.finish()
    return {"skins": len(skins), "packs": pack_count, "built_at": built_at}


def status(cache_root: str) -> dict:
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
