/**
 * Models page data: meshes of a skin, its textures, preview geometry and the
 * workspace export that the Blender add-on imports.
 *
 * Everything here shells out to `python -m exmt_core` (see `lib/core.ts`) except
 * the texture index, which is cached in the cache folder because building it
 * walks every bundle once (~10 s).
 */
import { exists, mkdir, readDir, readTextFile, remove, writeTextFile } from "@tauri-apps/plugin-fs";
import { appDataDir, join } from "@tauri-apps/api/path";
import catalogData from "../data/catalog.json";
import { gameBundles, runCore } from "./core";
import { ensureGameCache } from "./gamecache";
import { backup, cacheFolder, refreshBackup } from "./settings";
import { isTauri } from "./tauri";
import { newWorkspaceIdentity, workspaceRoot } from "./workspaces";


export interface SkinMesh {
  name: string;
  verts: number;
  tris: number;
  bones: number;
  bundle: string;
}

export interface TextureEntry {
  name: string;
  bundle: string;
  width?: number;
  height?: number;
  /** base64 PNG thumbnail, filled in by `loadThumbs` */
  png?: string;
}

export interface PreviewGeometry {
  name: string;
  bundle: string;
  count: number;
  positions: Float32Array;
  normals: Float32Array | null;
  uvs: Float32Array | null;
  indices: Uint32Array;
  bounds: { min: number[]; max: number[] };
}

export interface TexIndex {
  format: string;
  built_at: string;
  bundle_dir: string;
  texture_count: number;
  textures: Record<string, string[]>;
}

export interface AnimEntry {
  name: string;
  /** every bundle that stores this clip (the game may read any of them) */
  bundles: string[];
}

export interface AnimIndex {
  format: string;
  built_at: string;
  bundle_dir: string;
  clip_count: number;
  clips: Record<string, string[]>;
}

/** Texture role suffix, used to order a submesh's maps (diffuse first). */
const ROLE_RANK: Record<string, number> = {
  d: 0,
  da: 1,
  dif: 2,
  bt: 3,
  dt: 4,
  nt: 5,
  n: 6,
  rmo: 7,
  spc: 8,
};

export function textureRole(name: string): string {
  const match = /_(da|dif|dt|bt|nt|rmo|spc|d|n)$/i.exec(name);
  return match ? match[1].toLowerCase() : "";
}

/** Texture name without its role suffix, e.g. `…_cloth1_d` → `…_cloth1`. */
export function textureStem(name: string): string {
  return name.replace(/_(da|dif|dt|bt|nt|rmo|spc|d|n)$/i, "");
}

/**
 * One submesh's textures, by the part stem they share.
 *
 * The game keeps meshes and textures in different bundles and the material
 * bundles are mostly missing, so name sharing is the only local evidence:
 * `c_X_slg_cloth1_lod0` → `c_X_slg_cloth1_d/_n/_rmo`; a mesh named `cloth`
 * also picks up `cloth01` / `cloth02` (a mesh can carry several material
 * families). Textures matched by no mesh are returned by
 * `unassignedTextures` instead of being hidden.
 */
export function submeshStem(meshName: string): string {
  return meshName
    .replace(/_lod\d+/gi, "")
    .replace(/_(dorm|fight)$/i, "");
}

export function texturesForMesh(meshName: string, textures: TextureEntry[]): TextureEntry[] {
  const stem = submeshStem(meshName);
  if (!stem) return [];
  return textures
    .filter((tex) => tex.name.startsWith(stem))
    .sort((a, b) => {
      // 先按贴图家族分组（cloth1 / cloth2），同族内再按 D/N/RMO 排
      const family = textureStem(a.name).localeCompare(textureStem(b.name));
      if (family) return family;
      const ra = ROLE_RANK[textureRole(a.name)] ?? 99;
      const rb = ROLE_RANK[textureRole(b.name)] ?? 99;
      return ra - rb || a.name.localeCompare(b.name);
    });
}

/** Textures that no submesh claims — face/eye maps are often in this group. */
export function unassignedTextures(
  meshes: SkinMesh[],
  textures: TextureEntry[],
): TextureEntry[] {
  const assigned = new Set<string>();
  for (const mesh of meshes) {
    for (const tex of texturesForMesh(mesh.name, textures)) assigned.add(tex.name);
  }
  return textures.filter((tex) => !assigned.has(tex.name));
}

export interface WorkspaceResult {
  id: string;
  label: string;
  dir: string;
  meshes: number;
  textures: number;
  failed_textures: number;
  /** reference-only clips written to `animations/` (never packed into a Mod) */
  animations?: number;
  failed_animations?: number;
  /** bones in the merged skin skeleton (the union of every mesh's own table) */
  bones: number;
  /** how many of them got a real name from a local Avatar/prefab */
  named_bones: number;
  bytes: number;
}

export function bundlePathOf(hash: string): string {
  const clean = hash.replace(/\.bundle$/i, "");
  // 预览一律读我们自己的原始备份；备份里没有时才回落到游戏目录
  if (backup.value.ready) return `${backup.value.dir}\\${clean}.bundle`;
  return `${gameBundles()}\\${clean}.bundle`;
}

/**
 * Bundle to *pack* from: the untouched copy always wins.
 *
 * Previewing a Modded game folder is fine (that is what the game shows), but
 * packing must start from the original bytes — otherwise generating a Mod twice
 * would bake the first Mod into the second one.
 */
export async function packSource(hash: string): Promise<string> {
  const clean = hash.replace(/\.bundle$/i, "");
  const cached = `${cacheFolder.value}\\game\\bundles\\${clean}.bundle`;
  if (await exists(cached)) return cached;
  const legacy = `${await appDataDir()}\\com.exmt.modbox\\backups\\${clean}.bundle`;
  if (await exists(legacy)) return legacy;
  return bundlePathOf(hash);
}

/** Every bundle hash a catalog entry mentions, de-duplicated. */
export function bundleHashesOf(entry: {
  bundle: string;
  part_bundles?: Record<string, string[]>;
}): string[] {
  const out = new Set<string>([entry.bundle]);
  for (const list of Object.values(entry.part_bundles ?? {})) {
    for (const hash of list) out.add(hash);
  }
  return [...out];
}

async function runJson(args: string[]): Promise<any> {
  const res = await runCore(args);
  if (res.code !== 0) throw new Error((res.stderr || res.stdout || "core failed").trim().slice(-500));
  const text = res.stdout.trim().split("\n").filter(Boolean).pop() ?? "{}";
  return JSON.parse(text);
}

// --------------------------------------------------------------------------- //
// texture index (from the analysed GameCache — pages never scan the game)
// --------------------------------------------------------------------------- //
let texIndexCache: TexIndex | null = null;

/** Texture index: read from `<cache>/GameCache`, built once by the analysis. */
export async function ensureTexIndex(): Promise<TexIndex> {
  if (texIndexCache) return texIndexCache;
  const cache = await ensureGameCache();
  const textures = cache.names.textures;
  texIndexCache = {
    format: cache.manifest.format,
    built_at: cache.manifest.built_at,
    bundle_dir: cache.manifest.bundle_dir,
    texture_count: Object.keys(textures).length,
    textures,
  };
  return texIndexCache;
}

/** Textures whose names belong to this skin (`<skinKey>_…`, e.g. `c_AlvaSSR01_`). */
export function texturesFor(index: TexIndex, skinKey: string): TextureEntry[] {
  const prefix = `${skinKey}_`;
  const out: TextureEntry[] = [];
  for (const [name, bundles] of Object.entries(index.textures)) {
    if (!name.startsWith(prefix) || !bundles.length) continue;
    out.push({ name, bundle: bundles[0].replace(/\.bundle$/i, "") });
  }
  out.sort((a, b) => a.name.localeCompare(b.name));
  return out;
}

// --------------------------------------------------------------------------- //
// animation index (same cache; 39 974 clips over the whole library)
// --------------------------------------------------------------------------- //
let animIndexCache: AnimIndex | null = null;

/** Clip index: read from `<cache>/GameCache`, no per-page scan. */
export async function ensureAnimIndex(): Promise<AnimIndex> {
  if (animIndexCache) return animIndexCache;
  const cache = await ensureGameCache();
  animIndexCache = {
    format: cache.manifest.format,
    built_at: cache.manifest.built_at,
    bundle_dir: cache.manifest.bundle_dir,
    clip_count: Object.keys(cache.clips).length,
    clips: cache.clips,
  };
  return animIndexCache;
}

/** Clips of one skin (`c_<skinKey>_…`), sorted by name. */
export function animationsFor(index: AnimIndex, skinKey: string): AnimEntry[] {
  // skinKey already starts with `c_` (see skinKeyOf) — same convention as textures
  const prefix = `${skinKey}_`;
  const out: AnimEntry[] = [];
  for (const [name, bundles] of Object.entries(index.clips ?? {})) {
    if (!name.startsWith(prefix) || !bundles.length) continue;
    out.push({ name, bundles: bundles.map((hash) => hash.replace(/\.bundle$/i, "")) });
  }
  out.sort((a, b) => a.name.localeCompare(b.name));
  return out;
}

// --------------------------------------------------------------------------- //
// bone names
// --------------------------------------------------------------------------- //
/**
 * `crc32(transform path) -> path`, harvested once from every local Avatar
 * (`m_TOS`) and prefab hierarchy. The game gives meshes only bone hashes, so
 * this is the one way to print real bone names — and, because the hash covers
 * the whole path, a hit also proves the parent chain.
 *
 * The analysis writes it to `GameCache/bones.json`; nothing builds it on demand
 * any more.
 */
export async function boneNamesIndexPath(): Promise<string> {
  const cache = await ensureGameCache();
  return join(cache.dir, "bones.json");
}

/**
 * Path of the bone-name table (`GameCache/bones.json`, 50 005 paths for GF2).
 *
 * The names come from Avatars and prefabs (`crc32(transform path)`), which only
 * the **game** folder has — the analysis writes them into the cache, so a
 * workspace export no longer triggers a scan of its own.
 */
export async function ensureBoneNames(): Promise<string | null> {
  const cache = await ensureGameCache();
  const path = await join(cache.dir, "bones.json");
  return (await exists(path).catch(() => false)) ? path : null;
}

// --------------------------------------------------------------------------- //
// mesh / texture name index (same cache: mesh -> every bundle that stores a copy)
// --------------------------------------------------------------------------- //
/**
 * A mesh is not necessarily stored once: 68 of the 643 inspected bundles hold
 * meshes of more than one skin. Patching only one copy would leave the game free
 * to load an unmodified one, so the workspace records every bundle per mesh.
 */
export async function ensureMeshIndex(): Promise<{
  meshes: Record<string, string[]>;
  textures: Record<string, string[]>;
} | null> {
  const cache = await ensureGameCache();
  return cache.names;
}

// --------------------------------------------------------------------------- //
// per-skin cache: <cache>/GameCache/skins/<skinKey>/{meta,meshes,geometry,thumbs,diffuse}.json
// --------------------------------------------------------------------------- //
const MEMORY_SKINS = 4;

export interface SkinEntry {
  id: string;
  name: string;
  bundle: string;
  parts?: Record<string, string>;
  part_bundles?: Record<string, string[]>;
}

export interface SkinData {
  key: string;
  meshes: SkinMesh[];
  geometry: PreviewGeometry[];
  textures: TextureEntry[];
  /** mesh name → `data:` URL of its diffuse (`_d` / `_da`) texture */
  maps: Record<string, string>;
  cached: boolean;
  builtAt: string | null;
}

const skinMemory = new Map<string, SkinData>();

/** `c_AlvaSSR01` — the prefix shared by a skin's meshes and textures. */
export function skinKeyOf(entry: SkinEntry): string {
  const first = Object.values(entry.parts ?? {})[0];
  const m = first ? /^(c_[^_]+)_/.exec(first) : null;
  return m ? m[1] : `c_${entry.name}`;
}

function remember(data: SkinData) {
  skinMemory.delete(data.key);
  skinMemory.set(data.key, data);
  while (skinMemory.size > MEMORY_SKINS) {
    const oldest = skinMemory.keys().next().value;
    if (oldest === undefined) break;
    skinMemory.delete(oldest);
  }
}

async function readJson<T>(path: string): Promise<T | null> {
  try {
    if (!(await exists(path))) return null;
    return JSON.parse(await readTextFile(path)) as T;
  } catch {
    return null;
  }
}

function decodeSet(raw: { meshes?: any[] } | null): PreviewGeometry[] {
  return (raw?.meshes ?? []).map(decodeGeometry);
}

/**
 * `c_X_slg_cloth1_lod0` → `c_X_slg_cloth1_d` (or `_da`, the alpha variant).
 * Falls back to the longest shared stem so `emitter_slg_lod0` still finds
 * `emitter_slg_d`.
 */
export function diffuseNameFor(mesh: string, names: string[]): string | null {
  const stem = mesh.replace(/_lod\d+$/i, "");
  const all = new Set(names);
  for (const suffix of ["_d", "_da", "_dif"]) {
    const candidate = `${stem}${suffix}`;
    if (all.has(candidate)) return candidate;
  }
  let best: string | null = null;
  let bestLength = 0;
  for (const name of names) {
    const m = /^(.*?)_(d|da|dif)$/i.exec(name);
    if (!m) continue;
    const textureStem = m[1];
    if (!(stem.startsWith(textureStem) || textureStem.startsWith(stem))) continue;
    const shared = Math.min(stem.length, textureStem.length);
    if (shared > bestLength) {
      bestLength = shared;
      best = name;
    }
  }
  return best;
}

export type SkinPhase = "meshes" | "geometry" | "textures";

/**
 * Everything the studio shows for one skin.
 *
 * The analysis decodes **every** roster skin up front (`GameCache/skins/<key>/`:
 * meshes, geometry, thumbs, diffuse). So this normally reads four files and
 * returns — no Python process, no bundle parsing, ~30 ms.
 *
 * If a skin has no pack (the game shipped one the analysis skipped), it is
 * decoded once through `skinpack` and written into the same folder, so even that
 * case is a single process and only ever happens once.
 */
export async function loadSkin(
  entry: SkinEntry,
  onPhase?: (phase: SkinPhase) => void,
): Promise<SkinData> {
  const key = skinKeyOf(entry);
  const hit = skinMemory.get(key);
  if (hit) {
    remember(hit);
    return { ...hit, cached: true };
  }
  if (!isTauri) throw new Error("desktop only");

  onPhase?.("meshes");
  const cache = await ensureGameCache();
  const dir = await join(cache.dir, "skins", key);

  const read = async () => {
    const meta = await readJson<{ cache_built_at?: string; built_at?: string }>(
      await join(dir, "meta.json"),
    );
    if (!meta || meta.cache_built_at !== cache.manifest.built_at) return null;
    const meshes = await readJson<SkinMesh[]>(await join(dir, "meshes.json"));
    const raw = await readJson<{ meshes: any[] }>(await join(dir, "geometry.json"));
    const thumbs = await readJson<TextureEntry[]>(await join(dir, "thumbs.json"));
    if (!meshes || !raw || !thumbs) return null;
    const maps = (await readJson<Record<string, string>>(await join(dir, "diffuse.json"))) ?? {};
    return { meshes, raw, thumbs, maps, builtAt: meta.built_at ?? null };
  };

  let pack = await read();

  if (!pack) {
    // not pre-decoded (or built by an older analysis): one process does it all
    onPhase?.("geometry");
    const textures = texturesFor(await ensureTexIndex(), key);
    const groups = new Map<string, string[]>();
    for (const tex of textures) {
      const list = groups.get(tex.bundle) ?? [];
      list.push(tex.name);
      groups.set(tex.bundle, list);
    }
    const spec = {
      key,
      dir,
      bundle_dir: cache.manifest.bundle_dir,
      cache_built_at: cache.manifest.built_at,
      meshes: Object.entries(entry.parts ?? {}).map(([part, mesh]) => ({
        mesh,
        bundles: entry.part_bundles?.[part] ?? [entry.bundle],
      })),
      textures: [...groups.entries()].map(([bundle, names]) => ({ bundle, names })),
      maps: Object.values(entry.parts ?? {})
        .map((mesh) => {
          const name = diffuseNameFor(mesh, textures.map((tex) => tex.name));
          const bundle = textures.find((tex) => tex.name === name)?.bundle;
          return name && bundle ? { mesh, texture: name, bundle } : null;
        })
        .filter(Boolean),
      thumb_size: 160,
      map_size: 1024,
    };
    const specPath = await join(cacheFolder.value, `skinpack-${key}.json`);
    await mkdir(cacheFolder.value, { recursive: true }).catch(() => undefined);
    await writeTextFile(specPath, JSON.stringify(spec));
    const res = await runCore(["skinpack", specPath, ]);
    if (res.code !== 0) {
      throw new Error((res.stderr || res.stdout || "skin decode failed").trim().slice(-400));
    }
    pack = await read();
    if (!pack) throw new Error(`skin ${key} could not be decoded`);
  }

  onPhase?.("textures");
  const meshes = pack.meshes
    // a submesh whose buffers are missing (truncated weapon bundles) has no
    // geometry row — showing it would be a row that opens nothing
    .filter((mesh) => !(mesh as { error?: string }).error)
    .sort((a, b) => a.name.localeCompare(b.name));
  const data: SkinData = {
    key,
    meshes,
    geometry: decodeSet(pack.raw),
    textures: pack.thumbs.sort((a, b) => a.name.localeCompare(b.name)),
    maps: pack.maps,
    cached: true,
    builtAt: pack.builtAt,
  };
  remember(data);
  return data;
}

const meshCache = new Map<string, SkinMesh[]>();

/** All meshes across the entry's bundles; unreadable bundles are skipped. */
export async function loadMeshes(hashes: string[]): Promise<SkinMesh[]> {
  const seen = new Map<string, SkinMesh>();
  for (const hash of hashes) {
    let list = meshCache.get(hash);
    if (!list) {
      try {
        const data = await runJson(["skinfo", bundlePathOf(hash)]);
        list = (data.meshes as SkinMesh[]).map((mesh) => ({ ...mesh, bundle: hash }));
      } catch {
        // a bundle listed in the catalog may not be downloaded
        list = [];
      }
      meshCache.set(hash, list);
    }
    for (const mesh of list) if (!seen.has(mesh.name)) seen.set(mesh.name, mesh);
  }
  return [...seen.values()];
}

function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

function float32(b64: string | null): Float32Array | null {
  if (!b64) return null;
  return new Float32Array(b64ToBytes(b64).buffer);
}

function decodeGeometry(raw: any): PreviewGeometry {
  return {
    name: raw.name,
    bundle: raw.bundle,
    count: raw.count,
    positions: float32(raw.positions) ?? new Float32Array(),
    normals: float32(raw.normals),
    uvs: float32(raw.uvs),
    indices: new Uint32Array(b64ToBytes(raw.indices).buffer),
    bounds: raw.bounds,
  };
}

/** Downscaled PNGs for the grid; `size` is the long edge in pixels. */
export async function loadThumbs(hash: string, names: string[], size = 160): Promise<TextureEntry[]> {
  if (!names.length) return [];
  const data = await runJson(["thumbs", bundlePathOf(hash), String(size), ...names]);
  return (data.textures as TextureEntry[]).map((tex) => ({ ...tex, bundle: hash }));
}

// --------------------------------------------------------------------------- //
// original-asset backup (<cache>/game/bundles)
// --------------------------------------------------------------------------- //
interface CatalogSkin {
  name: string;
  bundle: string;
  part_bundles?: Record<string, string[]>;
}

/**
 * Every bundle whose original bytes we keep: all catalog mesh bundles plus the
 * texture bundles of those skins. Around 6.5 GB for the whole roster.
 */
export async function backupHashes(): Promise<string[]> {
  const skins = (catalogData as { characters: CatalogSkin[] }).characters;
  const hashes = new Set<string>();
  for (const skin of skins) {
    hashes.add(skin.bundle);
    for (const list of Object.values(skin.part_bundles ?? {})) for (const hash of list) hashes.add(hash);
  }
  const index = await ensureTexIndex();
  for (const skin of skins) {
    const prefix = `c_${skin.name}_`;
    for (const [name, bundles] of Object.entries(index.textures)) {
      if (name.startsWith(prefix) && bundles.length) hashes.add(bundles[0].replace(/\.bundle$/i, ""));
    }
  }
  return [...hashes].sort();
}

export interface BackupResult {
  copied: number;
  skipped: number;
  missing: number;
  bytes: number;
  seconds: number;
  files: number;
}

/**
 * Original bundles kept by the old replace flow in `%APPDATA%\com.exmt.modbox\backups`.
 * They predate any mod, so they win over the live game file when present.
 */
async function legacyBackupMap(hashes: string[]): Promise<Record<string, string>> {
  const root = `${await appDataDir()}\\com.exmt.modbox\\backups`;
  const entries = await readDir(root).catch(() => []);
  const present = new Set(
    entries
      .filter((entry) => entry.isFile && entry.name.toLowerCase().endsWith(".bundle"))
      .map((entry) => entry.name.replace(/\.bundle$/i, "")),
  );
  const map: Record<string, string> = {};
  for (const hash of hashes) {
    if (present.has(hash)) map[hash] = `${root}\\${hash}.bundle`;
  }
  return map;
}

/** Copy the original bundles out of the game before anything can modify them. */
export async function backupOriginals(): Promise<BackupResult> {
  if (!isTauri) throw new Error("backup needs the desktop shell");
  const root = await join(cacheFolder.value, "game");
  const dir = await join(root, "bundles");
  await mkdir(dir, { recursive: true }).catch(() => undefined);
  const hashes = await backupHashes();
  const listPath = await join(root, "hashes.json");
  await writeTextFile(listPath, JSON.stringify({ hashes }));
  // 旧版本替换流程留下的原始包（%APPDATA%\com.exmt.modbox\backups）优先于游戏目录，
  // 这样即使游戏包已经被 Mod 覆盖，也能拿到真正的原始文件。
  const prefer = await legacyBackupMap(hashes);
  const preferPath = await join(root, "prefer.json");
  await writeTextFile(preferPath, JSON.stringify(prefer));
  const marker = await join(root, "done.json");
  // 备份没做完之前不要让预览读半套文件
  await remove(marker).catch(() => undefined);
  const stats = (await runJson(["backup", gameBundles(), dir, listPath, preferPath])) as BackupResult;
  await writeTextFile(
    marker,
    JSON.stringify({ at: new Date().toISOString(), count: hashes.length, ...stats }),
  );
  await refreshBackup();
  // 备份完成后，索引/皮肤缓存的数据源要从游戏目录换成备份
  await resetDerivedCaches();
  return { ...stats, files: hashes.length };
}

/** Copy the backed-up originals back over the game folder. */
export async function restoreOriginals(): Promise<BackupResult> {
  if (!isTauri) throw new Error("restore needs the desktop shell");
  const root = await join(cacheFolder.value, "game");
  const dir = await join(root, "bundles");
  const listPath = await join(root, "hashes.json");
  if (!(await exists(listPath))) throw new Error("还没有备份，先备份一次");
  const stats = (await runJson(["restore", dir, gameBundles(), listPath])) as BackupResult;
  await resetDerivedCaches();
  return stats;
}

/** Drop per-skin caches derived from the live game folder (the analysis stays). */
async function resetDerivedCaches(): Promise<void> {
  texIndexCache = null;
  animIndexCache = null;
  skinMemory.clear();
  meshCache.clear();
  await remove(await join(cacheFolder.value, "skins"), { recursive: true }).catch(() => undefined);
}

// --------------------------------------------------------------------------- //
// workspace
// --------------------------------------------------------------------------- //
/**
 * Write meshes + textures + manifest into a fresh
 * `<cache>/workspace/<skin>__<timestamp>/` folder. A new folder per export is
 * deliberate: one skin may own many workspaces, and an old session must not be
 * overwritten. The active-workspace pointer (`EXMTConfig.json`) is written by
 * the view after it knows which workspace the user selected.
 */
export async function makeWorkspace(
  entry: { id: string; name: string; bundle: string },
  display: string,
  meshes: SkinMesh[],
  textures: TextureEntry[],
  animations: AnimEntry[] = [],
  /** every mesh of the skin, used to build the rig even when not exported */
  skeletonMeshes: SkinMesh[] = meshes,
): Promise<WorkspaceResult> {
  if (!isTauri) throw new Error("workspace export needs the desktop shell");
  const identity = await newWorkspaceIdentity({
    id: entry.id,
    name: entry.name,
    display,
  });
  const root = await workspaceRoot();
  const outdir = await join(root, identity.id);
  await mkdir(outdir, { recursive: true }).catch(() => undefined);

  const bundles: Record<string, string> = {};
  for (const mesh of meshes) bundles[mesh.bundle] = await packSource(mesh.bundle);
  for (const tex of textures) bundles[tex.bundle] = await packSource(tex.bundle);

  // real bone names come from an Avatar/prefab index; without it the skeleton is
  // still complete, just named by hash
  const boneNames = await ensureBoneNames();
  // every copy of every exported mesh / texture, so a Mod lands wherever the game looks
  const index = await ensureMeshIndex();
  const meshBundles: Record<string, string[]> = {};
  for (const mesh of meshes) {
    const found = index?.meshes?.[mesh.name] ?? [];
    const targets = found.length ? Array.from(new Set(found)) : [mesh.bundle];
    meshBundles[mesh.name] = targets;
    for (const hash of targets) {
      if (!bundles[hash]) bundles[hash] = await packSource(hash);
    }
  }
  const textureBundles: Record<string, string[]> = {};
  for (const tex of textures) {
    const found = index?.textures?.[tex.name] ?? [];
    const targets = found.length ? Array.from(new Set(found)) : [tex.bundle];
    textureBundles[tex.name] = targets;
    for (const hash of targets) {
      if (!bundles[hash]) bundles[hash] = await packSource(hash);
    }
  }

  // animations: reference-only, but their bundles have to be readable at export
  // time. Clips usually live in the **game** folder (the backup keeps only mesh
  // and texture bundles), so `packSource` falls back to the live file — our Mods
  // never rewrite animation objects, so that copy is still the game's own.
  const animItems: { clip: string; bundle: string }[] = [];
  for (const clip of animations) {
    const targets = clip.bundles.length ? clip.bundles : [];
    if (!targets.length) continue;
    for (const hash of targets) {
      if (!bundles[hash]) bundles[hash] = await packSource(hash);
    }
    // one copy is enough to read the clip; the rest are duplicates of the asset
    animItems.push({ clip: clip.name, bundle: targets[0] });
  }
  if (animItems.length) {
    // the rig takes the union of *all* the skin's meshes, so their bundles must
    // be readable even when that mesh is not part of the export
    for (const mesh of skeletonMeshes) {
      if (!bundles[mesh.bundle]) bundles[mesh.bundle] = await packSource(mesh.bundle);
    }
  }

  const spec = {
    workspace_id: identity.id,
    label: identity.label,
    created_at: identity.createdAt,
    id: entry.id,
    name: entry.name,
    display,
    game_bundles: gameBundles(),
    bundles,
    mesh_bundles: meshBundles,
    texture_bundles: textureBundles,
    meshes: meshes.map((mesh) => ({ bundle: mesh.bundle, mesh: mesh.name })),
    textures: textures.map((tex) => ({ bundle: tex.bundle, name: tex.name })),
    animations: animItems,
    // 动作按整副角色骨架做的：带动作时骨架要取**这个皮肤全部网格**的并集，
    // 否则只勾一个身体 → 32 根骨 vs 动作 213 根，播起来就是一团扭曲
    skeleton_meshes: animations.length
      ? skeletonMeshes.map((mesh) => ({ bundle: mesh.bundle, mesh: mesh.name }))
      : [],
    bone_names: boneNames ?? "",
  };
  const specPath = await join(outdir, "spec.json");
  await writeTextFile(specPath, JSON.stringify(spec));

  const result = (await runJson(["workspace", outdir, specPath])) as Omit<
    WorkspaceResult,
    "id" | "label"
  >;
  return { ...result, id: identity.id, label: identity.label };
}
