/**
 * GameCache: one analysis of the game folder, everything else reads from it.
 *
 * The app used to build its indexes on first use (`texindex` when a skin page
 * opens, `animidx` when the animation strip opens, `bonenames` when a workspace
 * is built), so every page paid its own full scan. Now the core writes
 * `<cache>/GameCache/{manifest,names,skins,clips,bones}.json` once — on first
 * run (SetupView) and after a game update (Settings) — and the pages only read
 * those files.
 *
 * `manifest.json` is written last, so "manifest parses" == "cache is complete";
 * a half-finished or interrupted analysis is never treated as usable.
 */
import { join } from "@tauri-apps/api/path";
import { exists, mkdir, readTextFile, remove, stat, writeTextFile } from "@tauri-apps/plugin-fs";
import { shallowRef } from "vue";
import { gameBundles, runCore } from "./core";
import { cacheFolder, gamePath } from "./settings";

export const CACHE_FORMAT = "exmt-gamecache-1";

export interface GameCacheManifest {
  format: string;
  built_at: string;
  bundle_dir: string;
  cache_dir: string;
  bundles: number;
  limited_to?: number | null;
  workers?: number;
  counts: {
    meshes: number;
    textures: number;
    clips: number;
    skins: number;
    bones: number;
  };
  seconds: number;
}

export interface SkinRow {
  id: string;
  name: string;
  variant: string;
  bundle: string;
  parts: Record<string, string>;
  part_bundles: Record<string, string[]>;
  mesh_count: number;
  texture_count: number;
  clip_count: number;
}

export interface GameCache {
  dir: string;
  manifest: GameCacheManifest;
  names: { meshes: Record<string, string[]>; textures: Record<string, string[]> };
  skins: SkinRow[];
  clips: Record<string, string[]>;
}

/** `…/GameCache`, inside the cache folder the user chose. */
export async function gameCacheDir(): Promise<string> {
  return join(cacheFolder.value, "GameCache");
}

/** `{state: ready|missing|incomplete|damaged|outdated|elsewhere, manifest}`. */
export async function cacheState(): Promise<{
  state: "ready" | "missing" | "incomplete" | "damaged" | "outdated" | "elsewhere";
  manifest: GameCacheManifest | null;
}> {
  const dir = await gameCacheDir();
  const path = await join(dir, "manifest.json");
  if (!(await exists(path).catch(() => false))) {
    const half = await exists(await join(dir, "progress.json")).catch(() => false);
    return { state: half ? "incomplete" : "missing", manifest: null };
  }
  try {
    const manifest = JSON.parse(await readTextFile(path)) as GameCacheManifest;
    if (manifest.format !== CACHE_FORMAT) return { state: "outdated", manifest };
    // a cache built for another install (or another drive) is not usable
    const wanted = gameBundles();
    if (wanted && manifest.bundle_dir.toLowerCase() !== wanted.toLowerCase()) {
      return { state: "elsewhere", manifest };
    }
    for (const name of ["names.json", "skins.json", "clips.json"]) {
      if (!(await exists(await join(dir, name)).catch(() => false))) {
        return { state: "incomplete", manifest };
      }
    }
    return { state: "ready", manifest };
  } catch {
    return { state: "damaged", manifest: null };
  }
}

async function readJson<T>(dir: string, name: string): Promise<T> {
  return JSON.parse(await readTextFile(await join(dir, name))) as T;
}

/** The one in-memory copy. Pages read this, never the disk, after bootstrap. */
export const gameCache = shallowRef<GameCache | null>(null);

/** Read the whole cache from disk (throws when it is not ready). */
export async function loadGameCache(): Promise<GameCache> {
  const dir = await gameCacheDir();
  const { state, manifest } = await cacheState();
  if (state !== "ready" || !manifest) {
    throw new Error(`game cache is ${state}`);
  }
  const names = await readJson<{ meshes: Record<string, string[]>; textures: Record<string, string[]> }>(
    dir,
    "names.json",
  );
  const skins = await readJson<{ skins: SkinRow[] }>(dir, "skins.json");
  const clips = await readJson<{ clips: Record<string, string[]> }>(dir, "clips.json");
  const cache: GameCache = {
    dir,
    manifest,
    names: { meshes: names.meshes ?? {}, textures: names.textures ?? {} },
    skins: skins.skins ?? [],
    clips: clips.clips ?? {},
  };
  gameCache.value = cache;
  return cache;
}

export async function ensureGameCache(): Promise<GameCache> {
  return gameCache.value ?? (await loadGameCache());
}

// --------------------------------------------------------------------------- //
// analysis
// --------------------------------------------------------------------------- //
export interface AnalyzeEvent {
  state: "running" | "done" | "error";
  phase: string | null;
  phases: { phase: string; seconds: number; [key: string]: unknown }[];
  seconds: number;
  error?: string;
}

/** Live progress the core writes while it scans (`GameCache/progress.json`). */
export async function readAnalyzeProgress(): Promise<AnalyzeEvent | null> {
  const dir = await gameCacheDir();
  const path = await join(dir, "progress.json");
  if (!(await exists(path).catch(() => false))) return null;
  try {
    return JSON.parse(await readTextFile(path)) as AnalyzeEvent;
  } catch {
    return null;
  }
}

/** Prove the folder is writable before we spend minutes writing into it. */
export async function verifyCacheFolder(dir: string): Promise<void> {
  const probe = `${dir}\\exmt-write-test.tmp`;
  await mkdir(dir, { recursive: true });
  await writeTextFile(probe, "exmt");
  const info = await stat(probe).catch(() => null);
  if (!info) throw new Error("缓存目录不可写");
  await remove(probe).catch(() => undefined);
}

export interface AnalyzeOptions {
  onProgress?: (event: AnalyzeEvent | null) => void;
  /** poll interval while the core runs, ms */
  intervalMs?: number;
  /** stop after N bundles — only used by tests */
  limit?: number;
}

/**
 * Run the full analysis and reload the cache. Progress is polled because the
 * core call is a one-shot invoke: the scan takes minutes, the UI needs a bar.
 */
export async function analyzeGame(options: AnalyzeOptions = {}): Promise<GameCacheManifest> {
  const bundles = gameBundles();
  if (!gamePath.value || !bundles) throw new Error("还没选游戏目录");
  if (!(await exists(bundles).catch(() => false))) throw new Error(`找不到资源目录：${bundles}`);
  await verifyCacheFolder(cacheFolder.value);

  const dir = await gameCacheDir();
  await mkdir(dir, { recursive: true }).catch(() => undefined);
  const argb = ["analyze", bundles, cacheFolder.value];
  if (options.limit) argb.push("8", String(options.limit));

  let live = true;
  const tick = async () => {
    if (!live) return;
    options.onProgress?.(await readAnalyzeProgress());
  };
  void tick();
  const timer = setInterval(() => void tick(), options.intervalMs ?? 900);

  try {
    const res = await runCore(argb);
    if (res.code !== 0) {
      const tail = (res.stderr || res.stdout).trim().split(/\r?\n/).slice(-6).join("\n");
      throw new Error(tail || `分析失败（退出码 ${res.code}）`);
    }
    const parsed = JSON.parse(res.stdout.trim().split(/\r?\n/).pop() ?? "{}") as GameCacheManifest;
    const cache = await loadGameCache();
    const last = (await readAnalyzeProgress()) ?? null;
    options.onProgress?.({
      state: "done",
      phase: null,
      phases: last?.phases ?? [],
      seconds: last?.seconds ?? cache.manifest.seconds,
    });
    return cache.manifest ?? parsed;
  } finally {
    live = false;
    clearInterval(timer);
  }
}

/** Forget the in-memory copy (cache folder / game path changed). */
export function dropGameCache(): void {
  gameCache.value = null;
}

/** Is the analysis forced yet? (first launch gate; cleared when the cache dies) */
const SETUP_KEY = "exmt.setup.done";
export const setupDone = shallowRef(localStorage.getItem(SETUP_KEY) === "1");

export function markSetupDone(done = true): void {
  setupDone.value = done;
  try {
    if (done) localStorage.setItem(SETUP_KEY, "1");
    else localStorage.removeItem(SETUP_KEY);
  } catch {
    /* ignore */
  }
}
