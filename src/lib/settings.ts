/**
 * App-wide settings that outlive a view: where temporary files go, where the
 * game lives, and how much of its original assets we keep in our own folder.
 */
import { computed, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { exists, mkdir, readDir } from "@tauri-apps/plugin-fs";
import { openPath } from "@tauri-apps/plugin-opener";
import { isTauri } from "./tauri";

export const DEFAULT_CACHE_DIR = "C:\\EXMTCacheFolder";
/** Bundles folder relative to the game install. */
const BUNDLE_SUFFIX = "GF2_Exilium_Data\\LocalCache\\Data\\AssetBundles_Windows";

const CACHE_KEY = "exmt.cacheFolder";
const GAME_KEY = "exmt.gamePath";

/** Trim, drop trailing separators; an empty value means "the default". */
function normalize(dir: string): string {
  const trimmed = dir.trim().replace(/[\\/]+$/, "");
  return trimmed || DEFAULT_CACHE_DIR;
}

function readStored(key: string, fallback: string): string {
  try {
    const raw = localStorage.getItem(key);
    if (raw) return raw;
  } catch {
    /* localStorage can be unavailable in restricted webviews */
  }
  return fallback;
}

function store(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

export const cacheFolder = ref<string>(normalize(readStored(CACHE_KEY, DEFAULT_CACHE_DIR)));
watch(cacheFolder, (value) => store(CACHE_KEY, value));

/** Game executable (or its install root); empty until the user picks one. */
export const gamePath = ref<string>(readStored(GAME_KEY, "").trim());
watch(gamePath, (value) => store(GAME_KEY, value.trim()));

/** `…\GF2_Exilium_Data\LocalCache\Data\AssetBundles_Windows` for the chosen install. */
export function bundleDirOf(path: string): string {
  const clean = path.trim().replace(/[\\/]+$/, "");
  if (!clean) return "";
  const root = clean.replace(/[\\/]GF2_Exilium\.exe$/i, "");
  return `${root}\\${BUNDLE_SUFFIX}`;
}

export const gameBundleDir = computed(() => bundleDirOf(gamePath.value));

/** unset | ok | bad */
export const gameStatus = ref<"unset" | "ok" | "bad">("unset");
export const gameMessage = ref("");

export const gameReady = computed(() => gameStatus.value === "ok");

/** Verify a path is a real install (the bundles folder must exist). */
export async function checkGamePath(path: string): Promise<boolean> {
  if (!isTauri) return false;
  const dir = bundleDirOf(path);
  if (!dir) {
    gameStatus.value = "unset";
    gameMessage.value = "";
    return false;
  }
  const ok = await exists(dir).catch(() => false);
  gameStatus.value = ok ? "ok" : "bad";
  gameMessage.value = ok ? dir : `找不到资源目录：${dir}`;
  return ok;
}

/** Common places the game is installed; first hit wins. */
const CANDIDATES = [
  "C:\\GF2Exilium\\GF2 Game",
  "C:\\Program Files\\GF2Exilium\\GF2 Game",
  "C:\\Program Files (x86)\\GF2Exilium\\GF2 Game",
  "D:\\GF2Exilium\\GF2 Game",
  "D:\\Games\\GF2Exilium\\GF2 Game",
  "E:\\GF2Exilium\\GF2 Game",
  "E:\\Games\\GF2Exilium\\GF2 Game",
];

/** Look for `GF2_Exilium.exe` in the usual folders plus one level of drive roots. */
export async function detectGamePath(): Promise<string | null> {
  if (!isTauri) return null;
  const tried = new Set<string>();
  const probe = async (root: string): Promise<string | null> => {
    if (tried.has(root)) return null;
    tried.add(root);
    for (const name of ["GF2_Exilium.exe", "GF2Exilium.exe"]) {
      const exe = `${root}\\${name}`;
      if (await exists(exe).catch(() => false)) {
        return (await checkGamePath(exe)) ? exe : null;
      }
    }
    return null;
  };

  if (gamePath.value) {
    const hit = await probe(gamePath.value.replace(/[\\/]GF2_Exilium\.exe$/i, ""));
    if (hit) return hit;
  }
  for (const root of CANDIDATES) {
    const hit = await probe(root);
    if (hit) return hit;
  }

  // shallow scan of drive roots: <drive>:\<folder> and <drive>:\Games\<folder>
  for (const drive of ["C:", "D:", "E:", "F:"]) {
    const roots = [`${drive}\\`, `${drive}\\Games`, `${drive}\\Program Files`];
    for (const base of roots) {
      const entries = await readDir(base).catch(() => null);
      if (!entries) continue;
      for (const entry of entries) {
        if (!entry.isDirectory) continue;
        if (!/gf2|exilium/i.test(entry.name)) continue;
        const hit = await probe(`${base}\\${entry.name}`) ?? (await probe(`${base}\\${entry.name}\\GF2 Game`));
        if (hit) return hit;
      }
    }
  }
  return null;
}

/** Remember a verified path and use it from now on. */
export async function useGamePath(path: string): Promise<boolean> {
  const ok = await checkGamePath(path);
  if (ok) gamePath.value = path.trim();
  return ok;
}

/** Native file picker for the game executable. */
export async function chooseGameExe(title: string): Promise<string | null> {
  if (!isTauri) return null;
  const picked = await open({
    title,
    multiple: false,
    filters: [{ name: "游戏主程序", extensions: ["exe"] }],
  });
  return typeof picked === "string" ? picked : null;
}

// --------------------------------------------------------------------------- //
// cache folder
// --------------------------------------------------------------------------- //
async function ensureFolder(dir: string): Promise<void> {
  if (!isTauri) return;
  if (!(await exists(dir))) await mkdir(dir, { recursive: true });
}

/** Create the cache folder if needed and return it. Throws when it is unusable. */
export async function ensureCacheFolder(): Promise<string> {
  await ensureFolder(cacheFolder.value);
  return cacheFolder.value;
}

/**
 * Switch to `dir`, but only after it is proven usable — a failed mkdir must not
 * leave the app pointing at a folder that does not exist.
 */
export async function useCacheFolder(dir: string): Promise<void> {
  const next = normalize(dir);
  await ensureFolder(next);
  cacheFolder.value = next;
}

/** Native folder picker; resolves to `null` when the user cancels. */
export async function chooseCacheFolder(title: string): Promise<string | null> {
  if (!isTauri) return null;
  const picked = await open({ title, directory: true });
  return typeof picked === "string" ? picked : null;
}

/** Open the cache folder in Explorer, creating it first when missing. */
export async function openCacheFolder(): Promise<void> {
  const dir = await ensureCacheFolder();
  await openPath(dir);
}

// --------------------------------------------------------------------------- //
// original-asset backup (<cache>/game/bundles)
// --------------------------------------------------------------------------- //
export interface BackupInfo {
  dir: string;
  files: number;
  bytes: number;
  ready: boolean;
}

export const backup = ref<BackupInfo>({ dir: "", files: 0, bytes: 0, ready: false });

/** `…\game` — our private copy of the original bundles. */
export async function gameRoot(): Promise<string> {
  return `${normalize(cacheFolder.value)}\\game`;
}

export async function backupBundlesDir(): Promise<string> {
  return `${await gameRoot()}\\bundles`;
}

/** Count what we already keep; `ready` means the backup can be used as the source. */
export async function refreshBackup(): Promise<BackupInfo> {
  const dir = await backupBundlesDir();
  const marker = `${await gameRoot()}\\done.json`;
  let files = 0;
  let bytes = 0;
  const complete = isTauri && (await exists(marker).catch(() => false));
  if (isTauri && (await exists(dir).catch(() => false))) {
    const entries = await readDir(dir).catch(() => []);
    for (const entry of entries) {
      if (!entry.isFile) continue;
      files += 1;
      const info = await statSafe(`${dir}\\${entry.name}`);
      bytes += info;
    }
  }
  backup.value = { dir, files, bytes, ready: complete && files > 0 };
  return backup.value;
}

async function statSafe(path: string): Promise<number> {
  try {
    const { stat } = await import("@tauri-apps/plugin-fs");
    const info = await stat(path);
    return Number(info.size ?? 0);
  } catch {
    return 0;
  }
}

/** Open the backup folder in Explorer (creating it when missing). */
export async function openBackupFolder(): Promise<void> {
  const dir = await backupBundlesDir();
  await mkdir(dir, { recursive: true }).catch(() => undefined);
  await openPath(dir);
}
