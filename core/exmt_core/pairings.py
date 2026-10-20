"""Pair a skin's submeshes with the texture families the game ships for them.

**Why names, and not materials.**  A character's meshes and its textures live in
different bundles and the material objects live in `cab-*` bundles the game
downloads on demand — measured over the whole local cache, none of the 18 467
`SkinnedMeshRenderer.m_Materials` pointers resolves to a local material.  So the
only evidence a Mod toolbox has offline is the name, plus the mesh's UV layout.

**The naming convention**, from the shipped data (spaces are `_`):

    c_<char>_slg_<part>_lod<N>[_<variant>]      mesh
    c_<char>_slg_<part>_<role>                  texture     (d/da/n/rmo/spc…)
    cw_<char>_<part>_WL_lod<N>                  weapon mesh
    cw_<char>_WL[_uber]_<role>                  weapon texture

Three things defeat a plain `name.startswith(mesh_stem)`:

* **Family textures.**  A skin ships the outfit under its variant name
  (`c_BalthildeSSR01_…`) but face/eye/hair under the *character family*
  (`c_Balthilde_slg_hair_d`).  The variant suffix is not part of the family.
* **The head is one mesh.**  `…_slg_hair_lod0` carries two submeshes: the hair
  (UVs cover `…_hair_d` completely) and an inner face shell (UVs land on
  `…_slg_face_d`).  So the head mesh also owns face/eye/eyeblend maps.
* **Shared base atlases.**  113 of 205 `slg_body` meshes have no body texture of
  their own; their UVs land on `B_body_d/n/rmo` (the shared body atlas — the UV
  islands line up, while `face_d` would put the chest on black gutters).

Variant words (`Dorm`, `Fight`, `General`, `Flat`, `Bend`, `trans`, `vfx`, `uber`,
`mat`) never change which atlas a part samples, so they are dropped before
matching.  `P1/P2/P3` and `cloth1/cloth2` do change it and are kept.
"""

from __future__ import annotations

import re
from functools import lru_cache

#: texture-role suffixes; everything before the role is the family stem
ROLE = re.compile(r"^(d|da|dif|dt|bt|nt|n|rmo|spc|smo|smoe|h|ramp|mask|il|ds)$", re.I)

#: tokens that identify a LOD copy, not a different asset (`lod0`, `lodm0`, the
#: game also ships a few typos such as `od1`)
LOD = re.compile(r"^l?odm?\d+$", re.I)

#: outfit/pose variants: same materials, different mesh state (`trans1` … `trans3`,
#: `Drom` is a shipped typo of `Dorm`)
NOISE = re.compile(
    r"^(dorm|fight|general|flat|bend|trans|vfx|swap|wt|uber|ubertrans|mat|pearl|drom)\d*$",
    re.I,
)

#: the shared body atlas: (family, role) — first family that exists wins
BASE_BODY = ("B_body", "new_body", "body")

#: shipped spelling mistakes in mesh part names (`boby` for `body`)
TYPO = {
    "boby": "body",
    "boddy": "body",
    "hare": "hair",
    "hiar": "hair",
    # `slg_sanzi` is the fan (扇子 `shanzi`); the mesh and the atlas disagree
    "sanzi": "shanzi",
    # Lenna's mesh says `glasses`, every atlas says `glass`
    "glasses": "glass",
}

#: shared atlases do not follow the `c_<char>_…` convention, so their roles are
#: matched with a plain suffix test
BASE_ROLE = re.compile(r"_(d|da|dif|n|nt|rmo|spc|h)$", re.I)

#: heads that travel together on one mesh, most specific first
HEAD_GROUP = {
    "hair": ("hair", "face", "eye", "eyeblend", "eyelash", "head", "helmet"),
    "head": ("head", "face", "eye", "eyeblend", "eyelash", "hair"),
}

#: roles that make a mesh "textured": `bt` is the plush/base variant of `d`
DIFFUSE = {"d", "da", "dif", "bt"}

#: Hand props are shared across characters and often carry no `c_` prefix
#: (`cw_MP443_WL_*`, `SabrinaSSR01_Pizza_*`, `cw_Tongyong_SmokeGrenade_*`).
SHARED_NS = {
    "tongyong", "none", "aelg", "cf", "elid", "mecha", "motordriven",
    "br", "npc", "enemy", "prop", "item", "weapon", "handmade",
}

#: body/clothing heads are unique per character: never borrow another skin's
#: `hip2` / `head1` / `cloth6` atlas just because the token is equal
GENERIC_HEAD = re.compile(
    r"^(body|hair|face|eye|eyebrow|eyeblend|eyelash|head|hip|cloth|hand|leg|foot|"
    r"weapon|slg|wl|p\d|g\d)[\d_]*$",
    re.I,
)

_VARIANT_TAIL = re.compile(r"(?:SSR|SR|R|Dorm|Fight)?\d*$")

#: `G1` / `P2`: a colour-group marker (see the numbered-atlas fallback in `pair`)
_VARIANT_TAIL_TOKEN = re.compile(r"^[GP]\d+$", re.I)


def family_of(char: str) -> str:
    """`BalthildeSSR01` → `Balthilde`; `AgentDorm` → `Agent`; `AGL01` → `AGL`."""
    base = char
    while True:
        trimmed = _VARIANT_TAIL.sub("", base)
        if trimmed == base or not trimmed:
            return trimmed or base
        base = trimmed


def _tokens(name: str) -> list[str]:
    return [tok for tok in name.split("_") if tok]


@lru_cache(maxsize=120000)
def parse(name: str) -> dict | None:
    """Split a mesh or texture name into `{prefix, char, tokens, head, role, stem}`.

    Returns None for names that do not follow the character convention (UI
    atlases, ramps, effect sheets: they are never paired with a submesh).
    """
    if not name or "_" not in name:
        return None
    raw = _tokens(name)
    if len(raw) < 2:
        return None
    role = ""
    if raw and ROLE.match(raw[-1]) and len(raw) > 2:
        role = raw[-1].lower()
        raw = raw[:-1]
    prefix = raw[0]
    if prefix not in ("c", "cw"):
        return None
    if len(raw) < 2:
        return None
    char = raw[1]
    body = [tok for tok in raw[2:] if not LOD.match(tok)]
    # variant words can sit before or after the lod token; keeping the semantic
    # tokens is what makes `cloth2_lod0_Fight` and `cloth2_lod0` one part
    tokens = [tok for tok in body if not LOD.match(tok) and not NOISE.match(tok)]
    tokens = [TYPO.get(tok.lower(), tok) for tok in tokens]
    if not tokens:
        if prefix == "cw":
            # a gun mesh without a part token still samples the weapon atlas
            tokens = ["WL"]
        else:
            # `c_LoreleySSR0101_lod0`: a whole-character mesh with no part token;
            # the character-level atlases are `c_<char>_d` / `c_<char>_slg_d`
            return {
                "name": name,
                "prefix": prefix,
                "char": char,
                "tokens": [],
                "head": "",
                "role": role,
                "stem": "_".join([prefix, char]),
                "base": family_of(char),
            }
    head = tokens[-1].lower()
    if prefix == "cw" and any(tok.lower() == "wl" for tok in tokens):
        # `cw_X_WL_Sight` / `cw_X_Silencer_WL`: the whole gun uses the WL atlas
        head = "wl"
    stem = "_".join([prefix, char] + tokens)
    return {
        "name": name,
        "prefix": prefix,
        "char": char,
        "tokens": tokens,
        "head": head,
        "role": role,
        "stem": stem,
        "base": family_of(char),
    }


def char_score(mesh: dict, tex: dict) -> int:
    """0 = different character, 3 = same variant, 2 = prefix variant, 1 = family."""
    a, b = mesh["char"], tex["char"]
    if a == b:
        return 3
    if a.startswith(b) or b.startswith(a):
        short, long_ = (a, b) if len(a) < len(b) else (b, a)
        tail = long_[len(short):]
        if tail and tail.isalnum() and short[-1].isalpha():
            return 2
    if mesh["base"] and mesh["base"] == tex["base"]:
        return 1
    return 0


def _direct(mesh: dict, tex: dict) -> bool:
    """One stem is the other plus a suffix (`cloth` → `cloth01`, `WL` → `WL_uber`)."""
    a, b = mesh["stem"], tex["stem"]
    return a.startswith(b + "_") or b.startswith(a + "_")


def heads_for(mesh: dict, mesh_heads: set[str]) -> tuple[str, ...]:
    """Which texture heads may sit on this mesh.

    A skin with a separate `face` mesh keeps face maps there; otherwise they ride
    on the head mesh (`hair`), which is where the game actually puts the face.
    """
    head = mesh["head"]
    group = HEAD_GROUP.get(head)
    if group:
        out = [head]
        for other in group[1:]:
            if other not in mesh_heads:
                out.append(other)
        return tuple(out)
    return (head,)


def families_of(textures) -> dict[str, list[str]]:
    """Group texture names by family stem: `{stem: [name, …]}` (roles together)."""
    families: dict[str, list[str]] = {}
    for name in textures:
        info = parse(name)
        if info and info["role"]:
            families.setdefault(info["stem"], []).append(name)
    return families


_PROP_ROLE = re.compile(r"_(d|da|dif|bt|n|nt|rmo|spc|h)$", re.I)


def _prop_families(all_names) -> dict[str, dict]:
    """Texture families that are *not* under the character namespace.

    Shared hand props: `cw_MP443_WL_*` (the gun every character can hold),
    `SabrinaSSR01_Pizza_*`, `Lollipop01_*`, `cw_Tongyong_SmokeGrenade_*`.  Their
    first token is either the object itself or a namespace (`Tongyong` = generic).
    """
    out: dict[str, dict] = {}
    for name in all_names:
        match = _PROP_ROLE.search(name)
        if not match:
            continue
        stem = name[:match.start()]
        tokens = [tok for tok in stem.split("_") if tok and not LOD.match(tok)]
        if not tokens:
            continue
        prefixed = tokens[0] in ("c", "cw")
        char = tokens[1] if prefixed and len(tokens) > 1 else tokens[0]
        rest = tokens[2:] if prefixed else tokens[1:]
        entry = out.setdefault(stem, {
            "names": [],
            "keys": set(),
            "char": char,
            "prefixed": prefixed,
            "shared": char.lower() in SHARED_NS
            or any(tok.lower() in SHARED_NS for tok in rest),
        })
        entry["names"].append(name)
        # trailing digits are object numbering, not identity (`Lollipop01`)
        for tok in [char, *rest]:
            low = tok.lower()
            entry["keys"].update({low, re.sub(r"\d+$", "", low)})
    return out


def pair(mesh_names: list[str], textures, families: dict | None = None) -> dict[str, list[str]]:
    """`{mesh name: [texture names]}` for one skin, strongest evidence first.

    Every texture family (stem) is assigned to at most one mesh: the one with the
    highest :func:`compatible` score.  Roles of a family always stay together, so
    a submesh lists `…_d`, `…_n`, `…_rmo` and not a random one of them.

    `textures` is an iterable of names or the `{name: [bundles]}` index; pass a
    prebuilt `families` when pairing many skins against one index.
    """
    meshes = []
    for name in mesh_names:
        info = parse(name)
        if info:
            meshes.append(info)
    if not meshes:
        return {}

    if families is None:
        families = families_of(textures)
    all_names = list(textures)

    mesh_heads = {m["head"] for m in meshes}
    # A family may genuinely serve several meshes: the gun parts (WL,
    # Silencer_WL, Sight_WL…) all sample the one WL atlas, and `cloth1` /
    # `cloth1_trans` are two meshes of the same material.  Strength is scored
    # per mesh, then each (mesh, head) keeps its best family — that is how
    # `cw_X_WL` picks the SSR atlas instead of collecting R/SR/SSR at once.
    winners: dict[str, dict[str, tuple[int, int, int, str]]] = {m["name"]: {} for m in meshes}
    for stem, names in families.items():
        info = parse(names[0])
        head = info["head"]
        diffuse = int(any(role_of(n) in DIFFUSE for n in names))
        for mesh in meshes:
            strength = char_score(mesh, info)
            if not strength:
                continue
            chain = int(mesh["tokens"] == info["tokens"] and mesh["prefix"] == info["prefix"])
            if mesh["head"] == head:
                score = (200 if chain else 100) + strength * 10
            elif mesh["head"] == "" and head == "slg":
                # whole-character mesh falls back to its `c_<char>_slg_d` atlas
                score = 40 + strength * 10
            elif _direct(mesh, info):
                # `Suitcase_slg_missle` → `Suitcase_slg`; `_1` → `_1_plane`
                score = 70 + strength * 10
            elif mesh["head"] and head.startswith(mesh["head"]) and head[len(mesh["head"]):].isdigit():
                # `slg_cloth` (one mesh, several material families) owns
                # `cloth01` / `cloth02`; same for `weapon` → `weapon01`
                score = 60 + strength * 10
            elif mesh["head"].startswith(head) and mesh["head"][len(head):].isdigit():
                # `P3_head1` / `head2` are numbered pieces of the family's
                # single `head` atlas
                score = 55 + strength * 10
            elif head in heads_for(mesh, mesh_heads):
                # the face shell lives on the head mesh when there is no face
                # mesh of its own
                score = 50 + strength * 10
            else:
                continue
            current = winners[mesh["name"]].get(head)
            key = (diffuse, score, chain)
            if current is None or key > (current[0], current[1], current[2]):
                winners[mesh["name"]][head] = (diffuse, score, chain, stem)

    out: dict[str, list[str]] = {m["name"]: [] for m in meshes}
    for mesh in meshes:
        # when the mesh's own head has a family, numbered sub-families
        # (`body1`/`body2` next to `body`) are alternatives, not extra maps
        if mesh["head"] and mesh["head"] in winners[mesh["name"]]:
            for head in list(winners[mesh["name"]]):
                rest = head[len(mesh["head"]):] if head.startswith(mesh["head"]) else ""
                if head != mesh["head"] and rest.isdigit():
                    del winners[mesh["name"]][head]
        for _d, _score, _chain, stem in winners[mesh["name"]].values():
            out[mesh["name"]].extend(families[stem])

    # A sibling submesh often carries the atlas: `hand2` mirrors `hand1` (same
    # UV layout, flipped), `hip3` mirrors `hip1`.  Only the same P-group counts,
    # and only when the sibling really ships a base-colour map.
    for mesh in meshes:
        if any(role_of(n) in DIFFUSE for n in out[mesh["name"]]):
            continue
        m = re.match(r"^(.*?)(\d+)$", mesh["head"])
        if not m or not m.group(1):
            continue
        base_head = m.group(1)
        best = None
        for stem, names_ in families.items():
            ti = parse(names_[0])
            if not ti:
                continue
            sib = re.match(r"^(.*?)(\d+)$", ti["head"])
            if not sib or sib.group(1) != base_head:
                continue
            if ti["tokens"][:-1] != mesh["tokens"][:-1]:
                continue
            if not char_score(mesh, ti):
                continue
            if not any(role_of(n) in DIFFUSE for n in names_):
                continue
            key = (int(sib.group(2)), -char_score(mesh, ti))
            if best is None or key < best[0]:
                best = (key, stem)
        if best:
            out[mesh["name"]].extend(families[best[1]])

    # shared hand props (guns, food, grenades) sit outside the character
    # namespace: match them by an exact part token, never by another skin's
    # body/clothing head.
    unresolved = [m for m in meshes if not any(role_of(n) in DIFFUSE for n in out[m["name"]])]
    if unresolved:
        props = _prop_families(all_names)
        for mesh in unresolved:
            head = mesh["head"]
            if len(head) < 3 or GENERIC_HEAD.match(head):
                continue
            stem_head = re.sub(r"\d+$", "", head)
            best = None
            for stem, entry in props.items():
                if head not in entry["keys"] and stem_head not in entry["keys"]:
                    continue
                if entry["char"].lower() == head:
                    rank = 0
                elif entry["char"].lower() == mesh["char"].lower():
                    # `Mishty_Beverage_*` beats the generic `Beverage_Clu_*`
                    rank = 1
                elif not entry["prefixed"]:
                    rank = 2
                elif entry["shared"]:
                    rank = 3
                else:
                    continue
                key = (rank, len(stem), stem)
                if best is None or key < best[0]:
                    best = (key, stem)
            if best:
                out[mesh["name"]].extend(props[best[1]]["names"])

    # `…_slg_fanOpen` / `_fanShut` are states of the same `fan` family
    for mesh in meshes:
        if any(role_of(n) in DIFFUSE for n in out[mesh["name"]]):
            continue
        head = mesh["head"]
        best = None
        for stem, names_ in families.items():
            ti = parse(names_[0])
            if not ti or not ti["head"] or not head.startswith(ti["head"]):
                continue
            tail = head[len(ti["head"]):]
            if len(tail) < 3 or not tail.isalpha():
                continue
            if not char_score(mesh, ti):
                continue
            if not any(role_of(n) in DIFFUSE for n in names_):
                continue
            key = (-char_score(mesh, ti), -len(ti["head"]), len(stem))
            if best is None or key < best[0]:
                best = (key, stem)
        if best:
            out[mesh["name"]].extend(families[best[1]])

    # A mech/colour variant can relabel its material group (`…_slg_G1`, `…_P2`)
    # while the atlas is numbered (`…_slg_01`, `_02`): same tokens, numbered
    # tail. Take the lowest number — the default colour — only when nothing
    # else matched.
    for mesh in meshes:
        if any(role_of(n) in DIFFUSE for n in out[mesh["name"]]):
            continue
        marker = next(
            (i for i, tok in enumerate(mesh["tokens"]) if _VARIANT_TAIL_TOKEN.match(tok)),
            None,
        )
        if marker is None:
            continue
        # `[Mech, WL01, P1, slg]` → `[Mech, WL01, slg]`
        prefix = mesh["tokens"][:marker] + mesh["tokens"][marker + 1:]
        if not prefix:
            continue
        best = None
        for stem, names_ in families.items():
            ti = parse(names_[0])
            if not ti or len(ti["tokens"]) < 2 or ti["tokens"][:-1] != prefix:
                continue
            if not ti["tokens"][-1].isdigit() or not char_score(mesh, ti):
                continue
            if not any(role_of(n) in DIFFUSE for n in names_):
                continue
            key = (int(ti["tokens"][-1]), len(stem))
            if best is None or key < best[0]:
                best = (key, stem)
        if best:
            out[mesh["name"]].extend(families[best[1]])

    # a body with no base-colour map of its own samples the shared body atlas
    for mesh in meshes:
        if not mesh["head"].startswith("body"):
            continue
        if any(role_of(n) in DIFFUSE for n in out[mesh["name"]]):
            continue
        for family in BASE_BODY:
            picked = [n for n in all_names
                      if n.startswith(family + "_") and BASE_ROLE.search(n)]
            if picked:
                out[mesh["name"]].extend(picked)
                break

    return out


def role_of(name: str) -> str:
    """Texture role for both `c_…` names and shared atlases (`B_body_d`)."""
    info = parse(name)
    if info and info["role"]:
        return info["role"]
    match = re.search(r"_(d|da|dif|n|nt|rmo|spc|h)$", name or "", re.I)
    return match.group(1).lower() if match else ""


def diffuse_of(names: list[str]) -> str | None:
    """The base-colour map of a family list (`_d` before `_da` before `_dif`)."""
    ranked = {"d": 0, "da": 1, "dif": 2, "bt": 3}
    best = None
    for name in names:
        role = role_of(name)
        if role not in ranked:
            continue
        if best is None or ranked[role] < ranked[role_of(best)]:
            best = name
    return best


def _compatible_head(mesh: dict, tex: dict, mesh_heads: set) -> bool:
    """Would the rules ever attach this family to this mesh (ignoring roles)?"""
    if not char_score(mesh, tex):
        return False
    head = tex["head"]
    if mesh["head"] == head:
        return True
    if mesh["head"] == "" and head == "slg":
        return True
    if _direct(mesh, tex):
        return True
    if mesh["head"] and head.startswith(mesh["head"]) and head[len(mesh["head"]):].isdigit():
        return True
    if mesh["head"] and mesh["head"].startswith(head) and mesh["head"][len(head):].isdigit():
        return True
    return head in heads_for(mesh, mesh_heads)


def audit(names: dict, skins: list, roster: bool = True) -> dict:
    """Coverage of the pairing rules over the cache; `pairs <cacheDir>`.

    A part is "covered" when at least one diffuse role is paired.  Every miss is
    classified so a parser regression cannot hide behind missing downloads:

    * `missing_bug` — a diffuse family exists that the rules would accept, yet
      nothing was paired.  Must be empty.
    * `missing_no_diffuse` — a family exists, but the cache only has n/rmo/spc
      (the base-colour map is not installed).
    * `no_texture_in_cache` — nothing under any plausible name.
    """
    textures = names.get("textures") or {}
    families = families_of(textures)
    fam_infos = [(stem, parse(names_[0])) for stem, names_ in families.items()]
    rows = []
    bugs = []
    covered = 0
    parts = 0
    unpaired = []
    no_diffuse = []
    for skin in skins:
        if roster and not _roster(skin):
            continue
        mesh_names = list((skin.get("parts") or {}).values())
        groups = pair(mesh_names, textures, families)
        mesh_heads = {info["head"] for info in map(parse, mesh_names) if info}
        for part, mesh in (skin.get("parts") or {}).items():
            parts += 1
            got = groups.get(mesh) or []
            if any(role_of(n) in DIFFUSE for n in got):
                covered += 1
                continue
            row = {"skin": skin["id"], "part": part, "mesh": mesh, "textures": got}
            rows.append(row)
            info = parse(mesh)
            candidates = []
            for stem, tex in fam_infos:
                if tex and _compatible_head(info, tex, mesh_heads):
                    candidates.append((stem, tex))
            if any(any(role_of(n) in DIFFUSE for n in families[stem])
                   for stem, _tex in candidates):
                bugs.append(row)
            elif candidates:
                no_diffuse.append(row)
            else:
                unpaired.append((skin["id"], mesh))
    return {
        "parts": parts,
        "covered": covered,
        "missing": len(rows),
        "missing_bug": bugs,
        "missing_no_diffuse": no_diffuse,
        "missing_parts": rows,
        "no_texture_in_cache": unpaired,
    }


def _roster(skin: dict) -> bool:
    parts = skin.get("parts") or {}
    return any(part.startswith("slg") for part in parts) or skin.get("variant") == "dorm"
