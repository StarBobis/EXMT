/**
 * Which texture belongs to which submesh — the TypeScript twin of
 * `core/exmt_core/pairings.py`.
 *
 * The core decides this once per skin and writes `groups.json` into the pack, so
 * this module only runs for a pack built by an older analysis (or while the
 * on-demand `skinpack` spec is assembled).  Keep the rules in sync with the
 * Python side: the naming convention is the only offline evidence (materials
 * live in `cab-*` bundles the game downloads on demand).
 */

const ROLE = /^(d|da|dif|dt|bt|nt|n|rmo|spc|smo|smoe|h|ramp|mask|il|ds)$/i;
const LOD = /^l?odm?\d+$/i;
const NOISE = /^(dorm|fight|general|flat|bend|trans|vfx|swap|wt|uber|ubertrans|mat|pearl|drom)\d*$/i;
const VARIANT_TAIL = /(?:SSR|SR|R|Dorm|Fight)?\d*$/;
const TYPO: Record<string, string> = {
  boby: "body",
  boddy: "body",
  hare: "hair",
  hiar: "hair",
  sanzi: "shanzi",
  glasses: "glass",
};

/** Body/clothing heads are unique per character: never borrow another skin's. */
const GENERIC_HEAD =
  /^(body|hair|face|eye|eyebrow|eyeblend|eyelash|head|hip|cloth|hand|leg|foot|weapon|slg|wl|p\d|g\d)[\d_]*$/i;

/** Namespaces whose textures are shared hand props (`cw_Tongyong_…`). */
const SHARED_NS = new Set([
  "tongyong", "none", "aelg", "cf", "elid", "mecha", "motordriven",
  "br", "npc", "enemy", "prop", "item", "weapon", "handmade",
]);

/** `G1` / `P2`: a colour-group marker over numbered atlases (`_01`, `_02`). */
const VARIANT_TOKEN = /^[GP]\d+$/i;
const PROP_ROLE = /_(d|da|dif|bt|n|nt|rmo|spc|h)$/i;

/** Roles that make a submesh "textured" (`bt` is the plush/base variant of `d`). */
export const DIFFUSE_ROLES = new Set(["d", "da", "dif", "bt"]);

export interface ParsedName {
  name: string;
  prefix: string;
  char: string;
  tokens: string[];
  head: string;
  role: string;
  stem: string;
  base: string;
}

const cache = new Map<string, ParsedName | null>();

/** `BalthildeSSR01` → `Balthilde`; `AgentDorm` → `Agent`; `AGL01` → `AGL`. */
export function familyOf(char: string): string {
  let base = char;
  for (;;) {
    const trimmed = base.replace(VARIANT_TAIL, "");
    if (trimmed === base || !trimmed) return trimmed || base;
    base = trimmed;
  }
}

/**
 * Split a mesh/texture name into character, tokens, head and role.
 * Returns null for names outside the character convention (UI atlases, ramps).
 */
export function parseName(name: string): ParsedName | null {
  if (cache.has(name)) return cache.get(name) ?? null;
  let raw = name.split("_").filter(Boolean);
  let role = "";
  if (raw.length > 2 && ROLE.test(raw[raw.length - 1])) {
    role = raw[raw.length - 1].toLowerCase();
    raw = raw.slice(0, -1);
  }
  if (raw.length < 2) {
    cache.set(name, null);
    return null;
  }
  const prefix = raw[0];
  const char = raw[1];
  let tokens = raw.slice(2).filter((tok) => !LOD.test(tok) && !NOISE.test(tok));
  tokens = tokens.map((tok) => TYPO[tok.toLowerCase()] ?? tok);
  if (!tokens.length) {
    if (prefix !== "cw") {
      // whole-character mesh (`c_LoreleySSR0101_lod0`): the root atlases are
      // `c_<char>_d` / `c_<char>_slg_d`
      const root: ParsedName = {
        name,
        prefix,
        char,
        tokens: [],
        head: "",
        role,
        stem: `${prefix}_${char}`,
        base: familyOf(char),
      };
      cache.set(name, root);
      return root;
    }
    tokens = ["WL"];
  }
  let head = tokens[tokens.length - 1].toLowerCase();
  if (prefix === "cw" && tokens.some((tok) => tok.toLowerCase() === "wl")) {
    // `cw_X_WL_Sight`: the whole gun samples the WL atlas
    head = "wl";
  }
  const parsed: ParsedName = {
    name,
    prefix,
    char,
    tokens,
    head,
    role,
    stem: `${prefix}_${char}_${tokens.join("_")}`,
    base: familyOf(char),
  };
  cache.set(name, parsed);
  return parsed;
}

/** 0 = different character, 3 = same variant, 2 = prefix variant, 1 = family. */
export function charScore(mesh: ParsedName, tex: ParsedName): number {
  const a = mesh.char;
  const b = tex.char;
  if (a === b) return 3;
  if (a.startsWith(b) || b.startsWith(a)) {
    const short = a.length < b.length ? a : b;
    const long = a.length < b.length ? b : a;
    const tail = long.slice(short.length);
    if (tail && /^[A-Za-z0-9]+$/.test(tail) && /[A-Za-z]$/.test(short.slice(-1))) return 2;
  }
  if (mesh.base && mesh.base === tex.base) return 1;
  return 0;
}

export function roleOf(name: string): string {
  const parsed = parseName(name);
  if (parsed?.role) return parsed.role;
  const m = /_(d|da|dif|n|nt|rmo|spc|h)$/i.exec(name ?? "");
  return m ? m[1].toLowerCase() : "";
}

function direct(mesh: ParsedName, tex: ParsedName): boolean {
  return tex.stem.startsWith(`${mesh.stem}_`) || mesh.stem.startsWith(`${tex.stem}_`);
}

const HEAD_GROUP: Record<string, string[]> = {
  hair: ["hair", "face", "eye", "eyeblend", "eyelash", "head", "helmet"],
  head: ["head", "face", "eye", "eyeblend", "eyelash", "hair"],
};

const BASE_BODY = ["B_body", "new_body", "body"];
const BASE_ROLE = /_(d|da|dif|n|nt|rmo|spc|h)$/i;

interface PropFamily {
  names: string[];
  keys: Set<string>;
  char: string;
  prefixed: boolean;
  shared: boolean;
}

/**
 * Texture families outside the character namespace — shared hand props.
 * Their first token is either the object (`MP443`) or a namespace (`Tongyong`).
 */
function propFamilies(names: string[]): Map<string, PropFamily> {
  const out = new Map<string, PropFamily>();
  for (const name of names) {
    const match = PROP_ROLE.exec(name);
    if (!match) continue;
    const stem = name.slice(0, match.index);
    const toks = stem.split("_").filter((tok) => tok && !LOD.test(tok));
    if (!toks.length) continue;
    const prefixed = toks[0] === "c" || toks[0] === "cw";
    const char = prefixed && toks.length > 1 ? toks[1] : toks[0];
    const rest = prefixed ? toks.slice(2) : toks.slice(1);
    let entry = out.get(stem);
    if (!entry) {
      entry = {
        names: [],
        keys: new Set<string>(),
        char,
        prefixed,
        shared:
          SHARED_NS.has(char.toLowerCase()) ||
          rest.some((tok) => SHARED_NS.has(tok.toLowerCase())),
      };
      out.set(stem, entry);
    }
    entry.names.push(name);
    // trailing digits are object numbering, not identity (`Lollipop01`)
    for (const tok of [char, ...rest]) {
      const low = tok.toLowerCase();
      entry.keys.add(low);
      entry.keys.add(low.replace(/\d+$/, ""));
    }
  }
  return out;
}

/**
 * `meshName → texture names`, strongest evidence first.  Mirrors `pair()` in
 * Python: stems (families) are grouped, scored per mesh, and each (mesh, head)
 * keeps its best family; roles of a family always stay together.
 */
export function pairTextures(
  meshNames: string[],
  textureNames: Iterable<string>,
): Record<string, string[]> {
  const meshes = meshNames
    .map((name) => parseName(name))
    .filter((item): item is ParsedName => !!item);
  const out: Record<string, string[]> = {};
  for (const mesh of meshes) out[mesh.name] = [];
  if (!meshes.length) return out;

  const names = Array.from(textureNames);
  const families = new Map<string, string[]>();
  for (const name of names) {
    const info = parseName(name);
    if (!info || !info.role) continue;
    const list = families.get(info.stem) ?? [];
    list.push(name);
    families.set(info.stem, list);
  }

  const meshHeads = new Set(meshes.map((mesh) => mesh.head));
  const winners = new Map<string, Map<string, { diffuse: number; score: number; chain: number; stem: string }>>();
  for (const mesh of meshes) winners.set(mesh.name, new Map());

  for (const [stem, list] of families) {
    const info = parseName(list[0]);
    if (!info) continue;
    const head = info.head;
    const diffuse = list.some((name) => DIFFUSE_ROLES.has(roleOf(name))) ? 1 : 0;
    for (const mesh of meshes) {
      const strength = charScore(mesh, info);
      if (!strength) continue;
      const chain = mesh.tokens.length === info.tokens.length &&
        mesh.tokens.every((tok, index) => tok === info.tokens[index]) &&
        mesh.prefix === info.prefix
        ? 1
        : 0;
      let score: number;
      if (mesh.head === head) score = (chain ? 200 : 100) + strength * 10;
      else if (mesh.head === "" && head === "slg") score = 40 + strength * 10;
      else if (direct(mesh, info)) score = 70 + strength * 10;
      else if (mesh.head && head.startsWith(mesh.head) && /^\d+$/.test(head.slice(mesh.head.length)))
        score = 60 + strength * 10;
      else if (mesh.head.startsWith(head) && /^\d+$/.test(mesh.head.slice(head.length)))
        score = 55 + strength * 10;
      else if ((HEAD_GROUP[mesh.head] ?? []).includes(head) && !meshHeads.has(head))
        score = 50 + strength * 10;
      else continue;
      const slot = winners.get(mesh.name)!;
      const current = slot.get(head);
      if (
        !current ||
        diffuse > current.diffuse ||
        (diffuse === current.diffuse && score > current.score) ||
        (diffuse === current.diffuse && score === current.score && chain > current.chain)
      ) {
        slot.set(head, { diffuse, score, chain, stem });
      }
    }
  }

  for (const mesh of meshes) {
    const slot = winners.get(mesh.name)!;
    // when the mesh's own head has a family, numbered alternatives (`body1`)
    // and variant prefixes (`Dorm_cloth` vs `cloth1`) are replacements
    if (mesh.head && slot.has(mesh.head)) {
      for (const head of Array.from(slot.keys())) {
        if (head === mesh.head) continue;
        const tail = mesh.head.startsWith(head) ? mesh.head.slice(head.length) : "";
        const ext = head.startsWith(mesh.head) ? head.slice(mesh.head.length) : "";
        if ((tail && /^\d+$/.test(tail)) || (ext && /^\d+$/.test(ext))) slot.delete(head);
      }
    }
    for (const winner of slot.values()) {
      out[mesh.name].push(...(families.get(winner.stem) ?? []));
    }
  }

  // a sibling submesh often carries the atlas (`hand2` mirrors `hand1`'s UVs)
  for (const mesh of meshes) {
    if (out[mesh.name].some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
    const m = /^(.*?)(\d+)$/.exec(mesh.head);
    if (!m || !m[1]) continue;
    let best: { key: number; stem: string } | null = null;
    for (const [stem, list] of families) {
      const info = parseName(list[0]);
      if (!info) continue;
      const sib = /^(.*?)(\d+)$/.exec(info.head);
      if (!sib || sib[1] !== m[1]) continue;
      if (info.tokens.slice(0, -1).join("_") !== mesh.tokens.slice(0, -1).join("_")) continue;
      if (!charScore(mesh, info)) continue;
      if (!list.some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
      const key = Number(sib[2]) * 10 - charScore(mesh, info);
      if (!best || key < best.key) best = { key, stem };
    }
    if (best) out[mesh.name].push(...(families.get(best.stem) ?? []));
  }

  // shared hand props (guns, food, grenades) live outside `c_<char>_`:
  // `cw_MP443_WL_*`, `SabrinaSSR01_Pizza_*`, `cw_Tongyong_SmokeGrenade_*`
  const unresolved = meshes.filter(
    (mesh) => !out[mesh.name].some((name) => DIFFUSE_ROLES.has(roleOf(name))),
  );
  if (unresolved.length) {
    const props = propFamilies(names);
    for (const mesh of unresolved) {
      const head = mesh.head;
      if (head.length < 3 || GENERIC_HEAD.test(head)) continue;
      const stemHead = head.replace(/\d+$/, "");
      let best: { rank: number; len: number; stem: string } | null = null;
      for (const [stem, entry] of props) {
        if (!entry.keys.has(head) && !entry.keys.has(stemHead)) continue;
        let rank: number;
        if (entry.char.toLowerCase() === head) rank = 0;
        else if (entry.char.toLowerCase() === mesh.char.toLowerCase()) rank = 1;
        else if (!entry.prefixed) rank = 2;
        else if (entry.shared) rank = 3;
        else continue;
        if (!best || rank < best.rank || (rank === best.rank && stem.length < best.len)) {
          best = { rank, len: stem.length, stem };
        }
      }
      if (best) out[mesh.name].push(...(props.get(best.stem)?.names ?? []));
    }
  }

  // `…_slg_fanOpen` / `_fanShut` are states of the same `fan` family
  for (const mesh of meshes) {
    if (out[mesh.name].some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
    const head = mesh.head;
    let best: { strength: number; headLen: number; stem: string } | null = null;
    for (const [stem, list] of families) {
      const info = parseName(list[0]);
      if (!info || !info.head || !head.startsWith(info.head)) continue;
      const tail = head.slice(info.head.length);
      if (tail.length < 3 || !/^[a-z]+$/i.test(tail)) continue;
      const strength = charScore(mesh, info);
      if (!strength) continue;
      if (!list.some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
      if (
        !best ||
        strength > best.strength ||
        (strength === best.strength && info.head.length > best.headLen)
      ) {
        best = { strength, headLen: info.head.length, stem };
      }
    }
    if (best) out[mesh.name].push(...(families.get(best.stem) ?? []));
  }

  // a colour group (`G1`, `P2`) over numbered atlases (`_01`, `_02`)
  for (const mesh of meshes) {
    if (out[mesh.name].some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
    const marker = mesh.tokens.findIndex((tok) => VARIANT_TOKEN.test(tok));
    if (marker < 0) continue;
    const prefix = [...mesh.tokens.slice(0, marker), ...mesh.tokens.slice(marker + 1)].join("_");
    let best: { num: number; len: number; stem: string } | null = null;
    for (const [stem, list] of families) {
      const info = parseName(list[0]);
      if (!info || info.tokens.length < 2) continue;
      if (info.tokens.slice(0, -1).join("_") !== prefix) continue;
      const last = info.tokens[info.tokens.length - 1];
      if (!/^\d+$/.test(last)) continue;
      if (!charScore(mesh, info)) continue;
      if (!list.some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
      const num = Number(last);
      if (!best || num < best.num || (num === best.num && stem.length < best.len)) {
        best = { num, len: stem.length, stem };
      }
    }
    if (best) out[mesh.name].push(...(families.get(best.stem) ?? []));
  }

  // a body with no base-colour map of its own samples the shared body atlas
  for (const mesh of meshes) {
    if (!mesh.head.startsWith("body")) continue;
    if (out[mesh.name].some((name) => DIFFUSE_ROLES.has(roleOf(name)))) continue;
    for (const family of BASE_BODY) {
      const picked = names.filter((name) => name.startsWith(`${family}_`) && BASE_ROLE.test(name));
      if (picked.length) {
        out[mesh.name].push(...picked);
        break;
      }
    }
  }
  return out;
}

/** The base-colour map of one submesh (`_d` before `_da` before `_dif`). */
export function diffuseOf(names: string[]): string | null {
  const rank: Record<string, number> = { d: 0, da: 1, dif: 2, bt: 3 };
  let best: string | null = null;
  for (const name of names) {
    const role = roleOf(name);
    if (!(role in rank)) continue;
    if (best === null || rank[role] < rank[roleOf(best)]) best = name;
  }
  return best;
}
