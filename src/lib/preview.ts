/**
 * Models page data: meshes of a skin, its textures, preview geometry and the
 * workspace export that the Blender add-on imports.
 *
 * Everything here shells out to `python -m exmt_core` (see `lib/core.ts`) except
 * the texture index, which is cached in the cache folder because building it
 * walks every bundle once (~10 s).
 */
import { exists, mkdir, readDir, readTextFile, remove, stat, writeTextFile } from "@tauri-apps/plugin-fs";
import { appDataDir, join } from "@tauri-apps/api/path";
import catalogData from "../data/catalog.json";
import { gameBundles, runCore } from "./core";
import { backup, cacheFolder, refreshBackup } from "./settings";
import { isTauri } from "./tauri";
import { newWorkspaceIdentity, workspaceRoot } from "./workspaces";

const INDEX_MAX_AGE_MS = 12 * 60 * 60 * 1000;

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
// texture index
// --------------------------------------------------------------------------- //
async function indexPath(): Promise<string> {
  return join(cacheFolder.value, "texindex.json");
}

async function readIndex(): Promise<TexIndex | null> {
  const path = await indexPath();
  if (!(await exists(path))) return null;
  try {
    const info = await stat(path);
    const age = Date.now() - new Date(info.mtime ?? 0).getTime();
    if (age > INDEX_MAX_AGE_MS) return null;
    const data = JSON.parse(await readTextFile(path)) as TexIndex;
    if (data.bundle_dir !== indexSourceDir()) return null;
    return data;
  } catch {
    return null;
  }
}

/** 贴图索引要从“原始资源”里建：有备份就用备份，否则用游戏目录。 */
function indexSourceDir(): string {
  return backup.value.ready ? backup.value.dir : gameBundles();
}

let indexCache: TexIndex | null = null;
let indexPromise: Promise<TexIndex> | null = null;

/** Load the texture index, rebuilding it when missing, stale or from another folder. */
export async function ensureTexIndex(): Promise<TexIndex> {
  if (indexCache) return indexCache;
  if (indexPromise) return indexPromise;
  indexPromise = (async () => {
    const cached = await readIndex();
    if (cached) return cached;
    const path = await indexPath();
    const res = await runCore(["texindex", indexSourceDir(), path]);
    if (res.code !== 0) throw new Error((res.stderr || res.stdout).trim().slice(-500));
    return JSON.parse(await readTextFile(path)) as TexIndex;
  })();
  try {
    indexCache = await indexPromise;
    return indexCache;
  } finally {
    indexPromise = null;
  }
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
// bone names
// --------------------------------------------------------------------------- //
/**
 * `crc32(transform path) -> path`, harvested once from every local Avatar
 * (`m_TOS`) and prefab hierarchy. The game gives meshes only bone hashes, so
 * this is the one way to print real bone names — and, because the hash covers
 * the whole path, a hit also proves the parent chain.
 */
async function boneNamesPath(): Promise<string> {
  return join(cacheFolder.value, "bonenames.json");
}

/** Where the bone-name index lives — the Blender add-on reads it via the config. */
export async function boneNamesIndexPath(): Promise<string> {
  return boneNamesPath();
}

let boneNamesPromise: Promise<string | null> | null = null;

/**
 * Path of the bone-name index, building it on first use (≈13 s, then cached).
 *
 * Scans the **game** folder, not the backup: names come from Avatars and prefabs
 * (`crc32(transform path)`), and the backup only holds the bundles this app
 * reads meshes and textures from — 418 paths there versus 50 005 in the game
 * folder, i.e. almost no bone would get a real name.
 */
export async function ensureBoneNames(): Promise<string | null> {
  if (boneNamesPromise) return boneNamesPromise;
  boneNamesPromise = (async () => {
    const path = await boneNamesPath();
    const source = gameBundles();
    try {
      if (await exists(path)) {
        const data = JSON.parse(await readTextFile(path)) as { bundle_dir?: string };
        if (data.bundle_dir === source) return path;
      }
      const res = await runCore(["bonenames", source, path]);
      if (res.code !== 0) return null;
      return path;
    } catch {
      return null;
    }
  })();
  try {
    return await boneNamesPromise;
  } finally {
    boneNamesPromise = null;
  }
}

// --------------------------------------------------------------------------- //
// mesh index: mesh name -> every bundle that stores a copy
// --------------------------------------------------------------------------- //
/**
 * A mesh is not necessarily stored once: 68 of the 643 bundles this app reads
 * hold meshes of more than one skin. Patching only the copy the catalog happens
 * to name would leave the game free to load an unmodified copy, so the workspace
 * records every bundle for every mesh it exports.
 */
async function meshIndexPath(): Promise<string> {
  return join(cacheFolder.value, "meshindex.json");
}

let meshIndexPromise: Promise<{ meshes: Record<string, string[]>; textures: Record<string, string[]> } | null> | null =
  null;

/**
 * Name index over the 643 backed-up bundles: `mesh/texture name -> [bundle hash]`.
 *
 * The **backup** folder is the right target here (unlike the bone names, which
 * only exist in the game folder's Avatars and prefabs): it holds every bundle
 * this app can pack into, including the extra copies of a mesh, and it does not
 * change while the user is working.
 *
 * Textures are duplicated more often than meshes (72 of 3824 names live in more
 * than one bundle), so both maps come out of the same single scan.
 */
export async function ensureMeshIndex(): Promise<{
  meshes: Record<string, string[]>;
  textures: Record<string, string[]>;
} | null> {
  if (meshIndexPromise) return meshIndexPromise;
  meshIndexPromise = (async () => {
    const path = await meshIndexPath();
    const source = backup.value.ready ? backup.value.dir : gameBundles();
    try {
      if (await exists(path)) {
        const data = JSON.parse(await readTextFile(path)) as {
          game_bundles?: string;
          meshes?: Record<string, string[]>;
          textures?: Record<string, string[]>;
        };
        // an index built before textures were tracked has no `textures` map: rebuild
        if (data.game_bundles === source && data.meshes && data.textures) {
          return { meshes: data.meshes, textures: data.textures };
        }
      }
      const res = await runCore(["meshidx", source, path]);
      if (res.code !== 0) return null;
      const data = JSON.parse(await readTextFile(path)) as {
        meshes?: Record<string, string[]>;
        textures?: Record<string, string[]>;
      };
      return { meshes: data.meshes ?? {}, textures: data.textures ?? {} };
    } catch {
      return null;
    }
  })();
  try {
    return await meshIndexPromise;
  } finally {
    meshIndexPromise = null;
  }
}

// --------------------------------------------------------------------------- //
// per-skin cache: <cache>/skins/<skinKey>/{meta,meshes,geometry,thumbs}.json
// --------------------------------------------------------------------------- //
const SKIN_CACHE_VERSION = 1;
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

async function skinsRoot(): Promise<string> {
  const root = await join(cacheFolder.value, "skins");
  await mkdir(root, { recursive: true }).catch(() => undefined);
  return root;
}

async function readJson<T>(path: string): Promise<T | null> {
  try {
    if (!(await exists(path))) return null;
    return JSON.parse(await readTextFile(path)) as T;
  } catch {
    return null;
  }
}

async function writeJson(path: string, value: unknown): Promise<void> {
  await writeTextFile(path, JSON.stringify(value));
}

/** Bundle files change only on a game update, so size+mtime is enough to invalidate. */
async function bundleSignature(hashes: string[]): Promise<string> {
  const parts: string[] = [];
  for (const hash of [...new Set(hashes)].sort()) {
    try {
      const info = await stat(bundlePathOf(hash));
      parts.push(`${hash}:${info.size}:${new Date(info.mtime ?? 0).getTime()}`);
    } catch {
      parts.push(`${hash}:missing`);
    }
  }
  return parts.join("|");
}

function decodeSet(raw: { meshes?: any[] } | null): PreviewGeometry[] {
  return (raw?.meshes ?? []).map(decodeGeometry);
}

async function geosetRaw(skinKey: string, meshes: SkinMesh[]): Promise<{ meshes: any[] }> {
  if (!meshes.length) return { meshes: [] };
  const bundles: Record<string, string> = {};
  for (const mesh of meshes) bundles[mesh.bundle] = bundlePathOf(mesh.bundle);
  const specPath = await join(cacheFolder.value, `geoset-${skinKey}.json`);
  await writeTextFile(
    specPath,
    JSON.stringify({
      bundles,
      meshes: meshes.map((mesh) => ({ bundle: mesh.bundle, mesh: mesh.name })),
    }),
  );
  return (await runJson(["geoset", specPath])) as { meshes: any[] };
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

async function buildDiffuseMaps(
  skinKey: string,
  meshes: SkinMesh[],
  textures: TextureEntry[],
): Promise<Record<string, string>> {
  const byName = new Map(textures.map((tex) => [tex.name, tex]));
  const names = textures.map((tex) => tex.name);
  const pairs: { mesh: string; texture: string; bundle: string }[] = [];
  for (const mesh of meshes) {
    const texture = diffuseNameFor(mesh.name, names);
    if (!texture) continue;
    const entry = byName.get(texture);
    if (!entry) continue;
    pairs.push({ mesh: mesh.name, texture, bundle: entry.bundle });
  }
  if (!pairs.length) return {};

  const bundles: Record<string, string> = {};
  for (const pair of pairs) bundles[pair.bundle] = bundlePathOf(pair.bundle);
  const specPath = await join(cacheFolder.value, `maps-${skinKey}.json`);
  await writeTextFile(specPath, JSON.stringify({ bundles, maps: pairs }));
  const data = (await runJson(["maps", specPath, "1024"])) as {
    maps?: { mesh: string; png?: string }[];
  };
  const out: Record<string, string> = {};
  for (const item of data.maps ?? []) {
    if (item.png) out[item.mesh] = `data:image/png;base64,${item.png}`;
  }
  return out;
}

async function thumbsFor(textures: TextureEntry[]): Promise<TextureEntry[]> {  const byBundle = new Map<string, TextureEntry[]>();
  for (const tex of textures) {
    const group = byBundle.get(tex.bundle);
    if (group) group.push(tex);
    else byBundle.set(tex.bundle, [tex]);
  }
  const out: TextureEntry[] = [];
  for (const [hash, items] of byBundle) {
    try {
      out.push(...(await loadThumbs(hash, items.map((i) => i.name))));
    } catch {
      out.push(...items);
    }
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

export type SkinPhase = "meshes" | "geometry" | "textures";

/**
 * Everything the studio shows for one skin.
 *
 * First visit parses the bundles and writes `<cache>/skins/<key>/`; later
 * visits read those four small files instead, so switching back is instant.
 * The in-memory LRU avoids even the disk read for the last few skins.
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

  const index = await ensureTexIndex();
  const texs = texturesFor(index, key);
  const meshHashes = bundleHashesOf(entry);
  const allHashes = [...meshHashes, ...texs.map((t) => t.bundle)];
  const dir = await join(await skinsRoot(), key);

  const signature = await bundleSignature(allHashes);
  const meta = await readJson<{ version: number; signature: string; built_at: string }>(
    await join(dir, "meta.json"),
  );
  if (meta && meta.version === SKIN_CACHE_VERSION && meta.signature === signature) {
    const meshes = await readJson<SkinMesh[]>(await join(dir, "meshes.json"));
    const raw = await readJson<{ meshes: any[] }>(await join(dir, "geometry.json"));
    const thumbs = await readJson<TextureEntry[]>(await join(dir, "thumbs.json"));
    if (meshes && raw && thumbs) {
      let maps = await readJson<Record<string, string>>(await join(dir, "diffuse.json"));
      if (!maps && meshes.length && texs.length) {
        maps = await buildDiffuseMaps(key, meshes, texs);
        await writeJson(await join(dir, "diffuse.json"), maps).catch(() => undefined);
      }
      const data: SkinData = {
        key,
        meshes,
        geometry: decodeSet(raw),
        textures: thumbs,
        maps: maps ?? {},
        cached: true,
        builtAt: meta.built_at,
      };
      remember(data);
      return data;
    }
  }

  onPhase?.("meshes");
  const prefix = `${key}_`;
  const found = await loadMeshes(meshHashes);
  const meshes = found
    .filter((mesh) => mesh.name.startsWith(prefix))
    .sort((a, b) => a.name.localeCompare(b.name));

  onPhase?.("geometry");
  const raw = await geosetRaw(key, meshes);

  onPhase?.("textures");
  const thumbs = await thumbsFor(texs);
  const maps = await buildDiffuseMaps(key, meshes, texs);

  const builtAt = new Date().toISOString();
  await mkdir(dir, { recursive: true }).catch(() => undefined);
  await Promise.all([
    writeJson(await join(dir, "meshes.json"), meshes),
    writeJson(await join(dir, "geometry.json"), raw),
    writeJson(await join(dir, "thumbs.json"), thumbs),
    writeJson(await join(dir, "diffuse.json"), maps),
    writeJson(await join(dir, "meta.json"), {
      version: SKIN_CACHE_VERSION,
      key,
      signature,
      built_at: builtAt,
    }),
  ]).catch(() => undefined);

  const data: SkinData = {
    key,
    meshes,
    geometry: decodeSet(raw),
    textures: thumbs,
    maps,
    cached: false,
    builtAt,
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

/** Drop everything derived from the live game folder (index, per-skin cache). */
async function resetDerivedCaches(): Promise<void> {
  indexCache = null;
  indexPromise = null;
  skinMemory.clear();
  meshCache.clear();
  await remove(await indexPath()).catch(() => undefined);
  await remove(await boneNamesPath()).catch(() => undefined);
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
