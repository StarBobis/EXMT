/**
 * Models page data: meshes of a skin, its textures, preview geometry and the
 * workspace export that the Blender add-on imports.
 *
 * Everything here shells out to `python -m exmt_core` (see `lib/core.ts`) except
 * the texture index, which is cached in the cache folder because building it
 * walks every bundle once (~10 s).
 */
import { exists, mkdir, readTextFile, stat, writeTextFile } from "@tauri-apps/plugin-fs";
import { join } from "@tauri-apps/api/path";
import { GAME_BUNDLES, runCore } from "./core";
import { cacheFolder } from "./settings";
import { isTauri } from "./tauri";

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

export interface WorkspaceResult {
  dir: string;
  meshes: number;
  textures: number;
  bytes: number;
}

export function bundlePathOf(hash: string): string {
  const clean = hash.replace(/\.bundle$/i, "");
  return `${GAME_BUNDLES}\\${clean}.bundle`;
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
    if (data.bundle_dir !== GAME_BUNDLES) return null;
    return data;
  } catch {
    return null;
  }
}

let indexCache: TexIndex | null = null;
let indexPromise: Promise<TexIndex> | null = null;

/** Load the texture index, rebuilding it when missing, stale or from another game folder. */
export async function ensureTexIndex(): Promise<TexIndex> {
  if (indexCache) return indexCache;
  if (indexPromise) return indexPromise;
  indexPromise = (async () => {
    const cached = await readIndex();
    if (cached) return cached;
    const path = await indexPath();
    const res = await runCore(["texindex", GAME_BUNDLES, path]);
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

async function thumbsFor(textures: TextureEntry[]): Promise<TextureEntry[]> {
  const byBundle = new Map<string, TextureEntry[]>();
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
      const data: SkinData = {
        key,
        meshes,
        geometry: decodeSet(raw),
        textures: thumbs,
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

  const builtAt = new Date().toISOString();
  await mkdir(dir, { recursive: true }).catch(() => undefined);
  await Promise.all([
    writeJson(await join(dir, "meshes.json"), meshes),
    writeJson(await join(dir, "geometry.json"), raw),
    writeJson(await join(dir, "thumbs.json"), thumbs),
    writeJson(await join(dir, "meta.json"), {
      version: SKIN_CACHE_VERSION,
      key,
      signature,
      built_at: builtAt,
    }),
  ]).catch(() => undefined);

  const data: SkinData = { key, meshes, geometry: decodeSet(raw), textures: thumbs, cached: false, builtAt };
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
// workspace
// --------------------------------------------------------------------------- //
export async function workspaceRoot(): Promise<string> {
  const root = await join(cacheFolder.value, "workspace");
  await mkdir(root, { recursive: true }).catch(() => undefined);
  return root;
}

/**
 * Write meshes + textures + manifest into `<cache>/workspace/<entry id>/` and
 * point `<cache>/workspace/current.json` at it, which is what the Blender
 * add-on reads for its one-click import.
 */
export async function makeWorkspace(
  entry: { id: string; name: string; bundle: string },
  display: string,
  meshes: SkinMesh[],
  textures: TextureEntry[],
): Promise<WorkspaceResult> {
  if (!isTauri) throw new Error("workspace export needs the desktop shell");
  const root = await workspaceRoot();
  const bundles: Record<string, string> = {};
  for (const mesh of meshes) bundles[mesh.bundle] = bundlePathOf(mesh.bundle);
  for (const tex of textures) bundles[tex.bundle] = bundlePathOf(tex.bundle);

  const spec = {
    id: entry.id,
    name: entry.name,
    display,
    game_bundles: GAME_BUNDLES,
    bundles,
    meshes: meshes.map((mesh) => ({ bundle: mesh.bundle, mesh: mesh.name })),
    textures: textures.map((tex) => ({ bundle: tex.bundle, name: tex.name })),
  };
  const specPath = await join(root, `spec-${entry.id}.json`);
  await writeTextFile(specPath, JSON.stringify(spec));

  const outdir = await join(root, entry.id);
  const result = (await runJson(["workspace", outdir, specPath])) as WorkspaceResult;
  await writeTextFile(
    await join(root, "current.json"),
    JSON.stringify({ id: entry.id, dir: outdir, updated_at: new Date().toISOString() }, null, 1),
  );
  return result;
}
