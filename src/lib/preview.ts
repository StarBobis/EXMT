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

let indexPromise: Promise<TexIndex> | null = null;

/** Load the texture index, rebuilding it when missing, stale or from another game folder. */
export async function ensureTexIndex(): Promise<TexIndex> {
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
    return await indexPromise;
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
// meshes / geometry / thumbnails
// --------------------------------------------------------------------------- //
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

/** Geometry for the whole skin in one core call (each bundle parsed once). */
export async function loadGeometrySet(
  entryId: string,
  meshes: SkinMesh[],
): Promise<PreviewGeometry[]> {
  if (!meshes.length) return [];
  const bundles: Record<string, string> = {};
  for (const mesh of meshes) bundles[mesh.bundle] = bundlePathOf(mesh.bundle);
  const specPath = await join(cacheFolder.value, `geoset-${entryId}.json`);
  await writeTextFile(
    specPath,
    JSON.stringify({
      bundles,
      meshes: meshes.map((mesh) => ({ bundle: mesh.bundle, mesh: mesh.name })),
    }),
  );
  const data = await runJson(["geoset", specPath]);
  return (data.meshes as any[]).map(decodeGeometry);
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
