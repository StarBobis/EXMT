/** App-side storage: game bundle paths, backups, installed-mod manifests. */
import { copyFile, exists, mkdir, readDir, readTextFile, remove, stat, writeTextFile } from "@tauri-apps/plugin-fs";
import { appDataDir, join } from "@tauri-apps/api/path";

export const GAME_BUNDLES =
  "C:\\GF2Exilium\\GF2 Game\\GF2_Exilium_Data\\LocalCache\\Data\\AssetBundles_Windows";

export function bundlePath(hash: string): string {
  return `${GAME_BUNDLES}\\${hash}.bundle`;
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

export async function cacheDir(): Promise<string> {
  const dir = await join(await appDataDir(), "cache");
  await ensureDir(dir);
  return dir;
}

export interface ModRecord {
  id: string;
  character?: string;
  preset?: string;
  mesh?: string;
  bundle?: string;
  source?: string;
  installed_at?: string;
  kind?: string;
}

/** Copy the pristine bundle aside the first time we touch it. */
export async function ensureBackup(hash: string): Promise<string> {
  const dir = await backupsDir();
  const bak = await join(dir, `${hash}.bundle`);
  if (!(await exists(bak))) {
    await copyFile(bundlePath(hash), bak);
    const info = await stat(bundlePath(hash));
    await writeTextFile(
      await join(dir, `${hash}.json`),
      JSON.stringify({ hash, size: info.size, created_at: new Date().toISOString() }, null, 2),
    );
  }
  return bak;
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
