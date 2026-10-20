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
const TYPO: Record<string, string> = { boby: "body", boddy: "body", hare: "hair", hiar: "hair" };

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
    for (const winner of winners.get(mesh.name)!.values()) {
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
