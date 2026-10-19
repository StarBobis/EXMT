/** App-side storage: game bundle paths, backups, installed-mod manifests. */
import { copyFile, exists, mkdir, readDir, readTextFile, remove, stat, writeTextFile } from "@tauri-apps/plugin-fs";
import { appDataDir, join } from "@tauri-apps/api/path";
import { ensureCacheFolder } from "./settings";
import { gameBundles, runCore } from "./core";

export function bundlePath(hash: string): string {
  return `${gameBundles()}\\${hash}.bundle`;
}

async function ensureDir(dir: string) {
  if (!(await exists(dir))) await mkdir(dir, { recursive: true });
}

export async function backupsDir(): Promise<string> {
  const dir = await join(await appDataDir(), "backups");
  await ensureDir(dir);
  return dir;
}

export async function modsDir(): Promise<string> {
  const dir = await join(await appDataDir(), "mods");
  await ensureDir(dir);
  return dir;
}

/** Scratch folder for packed bundles — user-configurable in Settings. */
export async function cacheDir(): Promise<string> {
  return ensureCacheFolder();
}

/** One edited mesh inside a Mod: what to patch, and where it has to land. */
export interface ModPatch {
  mesh: string;
  /** `.exmtmesh` path, relative to the Mod's `source` folder (or absolute) */
  file: string;
  /** every bundle that stores a copy of this mesh */
  bundles: string[];
}

/** One replaced texture inside a Mod: encoded blocks plus what they replace. */
export interface ModTexture {
  name: string;
  /** `.bin` payload path, relative to the Mod's `source` folder (or absolute) */
  file: string;
  /** every bundle that stores a copy of this texture */
  bundles: string[];
  /** the asset's own format (Unity TextureFormat), size and mip count */
  format?: number;
  width?: number;
  height?: number;
  mips?: number;
}

export interface ModRecord {
  id: string;
  character?: string;
  preset?: string;
  mesh?: string;
  bundle?: string;
  /** every bundle this record replaced (a workspace Mod usually has one) */
  bundles?: string[];
  /** workspace id when the Mod came from `<workspace>/GeneratedMod` */
  workspace?: string;
  source?: string;
  installed_at?: string;
  kind?: string;
  /** geometry patches — what makes two Mods mergeable */
  patches?: ModPatch[];
  /** texture patches — encoded in the target's own format by the add-on */
  textures?: ModTexture[];
  /** false = kept in the library but not applied to the game */
  enabled?: boolean;
}

/** Copy the pristine bundle aside the first time we touch it. */
export async function ensureBackup(hash: string): Promise<string> {
  const dir = await backupsDir();
  const bak = await join(dir, `${hash}.bundle`);
  if (!(await exists(bak))) {
    const source = await pristineSource(hash);
    await copyFile(source, bak);
    const info = await stat(source);
    await writeTextFile(
      await join(dir, `${hash}.json`),
      JSON.stringify(
        { hash, size: info.size, created_at: new Date().toISOString(), source },
        null,
        2,
      ),
    );
  }
  return bak;
}

/**
 * The untouched copy of a game bundle, best source first.
 *
 * The app keeps its own pristine copies under `<cache>/game/bundles`, and the
 * old replace flow left some in `%APPDATA%\com.exmt.modbox\backups`; the live
 * game file is only the last resort, because a previously installed Mod may
 * already have overwritten it — backing that up would make "还原" restore a Mod.
 */
export async function pristineSource(hash: string): Promise<string> {
  const legacy = await join(await appDataDir(), "com.exmt.modbox", "backups", `${hash}.bundle`);
  if (await exists(legacy)) return legacy;
  const cached = `${await cacheDir()}\\game\\bundles\\${hash}.bundle`;
  if (await exists(cached)) return cached;
  return bundlePath(hash);
}

export async function restoreBackup(hash: string): Promise<void> {
  const bak = await join(await backupsDir(), `${hash}.bundle`);
  if (!(await exists(bak))) throw new Error(`没有备份：${hash}`);
  await copyFile(bak, bundlePath(hash));
}

export async function backupExists(hash: string): Promise<boolean> {
  return exists(await join(await backupsDir(), `${hash}.bundle`));
}

export interface BackupEntry {
  hash: string;
  size: number;
  created_at?: string;
  installed: boolean;
}

export async function listBackups(): Promise<BackupEntry[]> {
  const dir = await backupsDir();
  const entries = await readDir(dir);
  const out: BackupEntry[] = [];
  for (const e of entries) {
    if (!e.name?.endsWith(".bundle")) continue;
    const hash = e.name.slice(0, -7);
    const info = await stat(await join(dir, e.name));
    let created_at: string | undefined;
    const metaPath = await join(dir, `${hash}.json`);
    if (await exists(metaPath)) {
      try {
        created_at = JSON.parse(await readTextFile(metaPath)).created_at;
      } catch {
        /* ignore */
      }
    }
    out.push({ hash, size: info.size ?? 0, created_at, installed: true });
  }
  out.sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""));
  return out;
}

export async function listMods(): Promise<ModRecord[]> {
  const dir = await modsDir();
  const entries = await readDir(dir);
  const out: ModRecord[] = [];
  for (const e of entries) {
    if (!e.name?.endsWith(".json")) continue;
    try {
      const rec = JSON.parse(await readTextFile(await join(dir, e.name))) as ModRecord;
      out.push({ ...rec, id: rec.id ?? e.name.slice(0, -5) });
    } catch {
      /* ignore broken manifests */
    }
  }
  out.sort((a, b) => (b.installed_at ?? "").localeCompare(a.installed_at ?? ""));
  return out;
}

export async function writeMod(record: ModRecord): Promise<void> {
  const dir = await modsDir();
  await writeTextFile(await join(dir, `${record.id}.json`), JSON.stringify(record, null, 2));
}

export async function deleteMod(id: string): Promise<void> {
  const p = await join(await modsDir(), `${id}.json`);
  if (await exists(p)) await remove(p);
}

/** Install a built bundle: back the original up first, then replace it. */
export async function installBundle(hash: string, builtBundle: string): Promise<string> {
  const bak = await ensureBackup(hash);
  await copyFile(builtBundle, bundlePath(hash));
  return bak;
}

// --------------------------------------------------------------------------- //
// Mods staged by the Blender add-on in <workspace>/GeneratedMod
// --------------------------------------------------------------------------- //
export interface GeneratedBundle {
  hash: string;
  file: string;
  bytes: number;
  meshes: string[];
}

export interface GeneratedMod {
  format: string;
  label?: string;
  created_at?: string;
  mesh_count?: number;
  bundles: GeneratedBundle[];
  patches?: ModPatch[];
  /** texture patches the add-on encoded in each target's own format */
  textures?: ModTexture[];
  dir: string;
}

/** `<workspace>/GeneratedMod/mod.json`, or null when nothing was generated yet. */
export async function readGeneratedMod(workspaceDir: string): Promise<GeneratedMod | null> {
  const dir = `${workspaceDir}\\GeneratedMod`;
  const manifest = `${dir}\\mod.json`;
  try {
    if (!(await exists(manifest))) return null;
    const data = JSON.parse(await readTextFile(manifest)) as GeneratedMod;
    if (!data.bundles?.length) return null;
    return { ...data, dir };
  } catch {
    return null;
  }
}

// --------------------------------------------------------------------------- //
// merging: the game state is *pristine + every enabled Mod*, never "last write
// wins". Two Mods that touch the same bundle (68 of the scanned bundles hold more
// than one skin) would otherwise silently overwrite each other.
// --------------------------------------------------------------------------- //
export interface AppliedState {
  format: string;
  updated_at: string;
  bundles: Record<string, { mods: string[]; meshes: number; textures?: number }>;
}

async function appliedStatePath(): Promise<string> {
  return join(await cacheDir(), "applied.json");
}

export async function readAppliedState(): Promise<AppliedState> {
  const empty: AppliedState = { format: "exmt-applied-1", updated_at: "", bundles: {} };
  try {
    const path = await appliedStatePath();
    if (!(await exists(path))) return empty;
    const data = JSON.parse(await readTextFile(path)) as AppliedState;
    return { ...empty, ...data, bundles: data.bundles ?? {} };
  } catch {
    return empty;
  }
}

export interface ApplyResult {
  /** Mods that contributed patches to this merge */
  mods: number;
  /** Older records (installed before merging existed) — left untouched */
  legacy: number;
  bundles: number;
  meshes: number;
  textures: number;
  restored: number;
  skipped: string[];
  errors: string[];
}

/** Absolute path of a patch's `.exmtmesh` inside its Mod folder. */
function patchFile(rec: ModRecord, patch: ModPatch): string {
  if (/^[A-Za-z]:[\\/]/.test(patch.file)) return patch.file;
  return `${rec.source ?? ""}\\${patch.file.replace(/\//g, "\\")}`;
}

/**
 * Rebuild every touched bundle from the pristine copy plus the patches of all
 * enabled Mods, then write the results once.
 *
 * `modpack` does the packing — it takes the pristine bundle and a list of
 * `{bundle, mesh, file}` rows, so merging is just "give it the union of every
 * Mod's rows, in install order". Bundles that no longer belong to any enabled
 * Mod are restored from their backups.
 */
export async function applyMods(): Promise<ApplyResult> {
  const result: ApplyResult = {
    mods: 0,
    legacy: 0,
    bundles: 0,
    meshes: 0,
    textures: 0,
    restored: 0,
    skipped: [],
    errors: [],
  };
  const records = (await listMods()).sort((a, b) =>
    String(a.installed_at ?? "").localeCompare(String(b.installed_at ?? "")),
  );
  const enabled = records.filter((rec) => rec.enabled !== false);

  const rows: { bundle: string; mesh: string; file: string }[] = [];
  const texRows: {
    bundle: string;
    name: string;
    file: string;
    format?: number;
    width?: number;
    height?: number;
    mips?: number;
  }[] = [];
  const byBundle: Record<string, Set<string>> = {};
  const texByBundle: Record<string, Set<string>> = {};
  const owner: Record<string, Set<string>> = {};
  for (const rec of enabled) {
    if (!rec.patches?.length) {
      // legacy record (installed before merging existed): it is already in the
      // game and has no patch files, so there is nothing to replay
      result.legacy += 1;
      result.skipped.push(rec.id);
      continue;
    }
    let used = 0;
    for (const patch of rec.patches) {
      const file = patchFile(rec, patch);
      if (!(await exists(file))) {
        result.skipped.push(`${rec.id}:${patch.mesh}`);
        continue;
      }
      for (const hash of patch.bundles?.length ? patch.bundles : rec.bundles ?? []) {
        rows.push({ bundle: hash, mesh: patch.mesh, file });
        (byBundle[hash] ??= new Set<string>()).add(patch.mesh);
        (owner[hash] ??= new Set<string>()).add(rec.id);
        used += 1;
      }
    }
    // texture patches: the payload is already encoded in the target's own format
    // (the add-on ran texconv), so merging only has to hand the bytes to modpack
    for (const tex of rec.textures ?? []) {
      const file = patchFile(rec, { mesh: tex.name, file: tex.file, bundles: tex.bundles });
      if (!(await exists(file))) {
        result.skipped.push(`${rec.id}:${tex.name}`);
        continue;
      }
      for (const hash of tex.bundles?.length ? tex.bundles : rec.bundles ?? []) {
        texRows.push({
          bundle: hash,
          name: tex.name,
          file,
          format: tex.format,
          width: tex.width,
          height: tex.height,
          mips: tex.mips,
        });
        (owner[hash] ??= new Set<string>()).add(rec.id);
        (texByBundle[hash] ??= new Set<string>()).add(tex.name);
        used += 1;
      }
    }
    if (used) result.mods += 1;
  }

  const previous = await readAppliedState();
  // a bundle can be touched only by a texture patch, so both maps contribute
  const targets = Array.from(new Set([...Object.keys(byBundle), ...Object.keys(texByBundle)]));

  if (rows.length || texRows.length) {
    // every patch is applied to a fresh copy of the *original* bundle; modpack
    // writes each bundle once, so several Mods touching one bundle merge cleanly
    const outdir = `${await cacheDir()}\\merge`;
    const bundles: Record<string, string> = {};
    for (const hash of targets) bundles[hash] = await pristineSource(hash);
    const spec = {
      outdir,
      label: "merged",
      workspace: "merge",
      game_bundles: gameBundles(),
      bundles,
      meshes: rows,
      textures: texRows,
    };
    const specPath = `${outdir}.spec.json`;
    await writeTextFile(specPath, JSON.stringify(spec));
    const res = await runCore(["modpack", specPath]);
    if (res.code !== 0) {
      result.errors.push((res.stderr || res.stdout || "modpack 失败").slice(-300));
      return result;
    }
    const summary = JSON.parse(res.stdout.trim().split("\n").pop() ?? "{}") as {
      bundles?: number;
      meshes?: number;
      textures?: number;
      missing?: number;
    };
    result.bundles = summary.bundles ?? 0;
    result.meshes = summary.meshes ?? 0;
    result.textures = summary.textures ?? 0;
    if (summary.missing) result.errors.push(`${summary.missing} 项缺文件`);

    const state: AppliedState = { format: "exmt-applied-1", updated_at: new Date().toISOString(), bundles: {} };
    for (const hash of targets) {
      const built = `${outdir}\\${hash}.bundle`;
      if (!(await exists(built))) continue;
      await ensureBackup(hash);
      await copyFile(built, bundlePath(hash));
      state.bundles[hash] = {
        mods: Array.from(owner[hash] ?? []),
        meshes: byBundle[hash]?.size ?? 0,
        textures: texByBundle[hash]?.size ?? 0,
      };
    }
    // bundles that were patched before but no Mod claims them any more
    for (const hash of Object.keys(previous.bundles)) {
      if (state.bundles[hash]) continue;
      if (await backupExists(hash)) {
        await restoreBackup(hash);
        result.restored += 1;
      }
    }
    await writeTextFile(await appliedStatePath(), JSON.stringify(state, null, 2));
    return result;
  }

  // nothing enabled: put the game back to original
  for (const hash of Object.keys(previous.bundles)) {
    if (await backupExists(hash)) {
      await restoreBackup(hash);
      result.restored += 1;
    }
  }
  await writeTextFile(
    await appliedStatePath(),
    JSON.stringify({ format: "exmt-applied-1", updated_at: new Date().toISOString(), bundles: {} }, null, 2),
  );
  return result;
}

/** Enable or disable one Mod and re-apply the whole set. */
export async function setModEnabled(id: string, enabled: boolean): Promise<ApplyResult> {
  const records = await listMods();
  const rec = records.find((item) => item.id === id);
  if (!rec) throw new Error(`找不到 Mod：${id}`);
  rec.enabled = enabled;
  await writeMod(rec);
  return applyMods();
}

export interface InstallResult {
  bundles: number;
  meshes: number;
  bytes: number;
  backups: number;
}

/** Copy a generated Mod into the game folder, keeping the originals aside. */
export async function installGeneratedMod(mod: GeneratedMod): Promise<InstallResult> {
  let bytes = 0;
  let meshes = 0;
  let backups = 0;
  for (const entry of mod.bundles) {
    const built = `${mod.dir}\\${entry.file}`;
    if (!(await exists(built))) throw new Error(`缺少打包文件：${entry.file}`);
    const had = await backupExists(entry.hash);
    await ensureBackup(entry.hash);
    if (!had) backups += 1;
    await copyFile(built, bundlePath(entry.hash));
    bytes += entry.bytes ?? 0;
    meshes += entry.meshes?.length ?? 0;
  }
  return { bundles: mod.bundles.length, meshes, bytes, backups };
}

/** Put the original bundles back for every bundle a Mod replaced. */
export async function uninstallBundles(hashes: string[]): Promise<number> {
  let restored = 0;
  for (const hash of hashes) {
    if (!(await backupExists(hash))) continue;
    await restoreBackup(hash);
    restored += 1;
  }
  return restored;
}
