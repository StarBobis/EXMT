/**
 * App-wide settings that outlive a view: where temporary files go.
 *
 * The cache folder is deliberately a real user-facing setting, because packing
 * a bundle writes a multi-hundred-megabyte file next to the game while it is
 * being built — people want that on a fast drive, not hidden in %APPDATA%.
 */
import { ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { exists, mkdir } from "@tauri-apps/plugin-fs";
import { openPath } from "@tauri-apps/plugin-opener";
import { isTauri } from "./tauri";

export const DEFAULT_CACHE_DIR = "C:\\EXMTCacheFolder";

const STORAGE_KEY = "exmt.cacheFolder";

/** Trim, drop trailing separators; an empty value means "the default". */
function normalize(dir: string): string {
  const trimmed = dir.trim().replace(/[\\/]+$/, "");
  return trimmed || DEFAULT_CACHE_DIR;
}

function readFolder(): string {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return normalize(raw);
  } catch {
    /* localStorage can be unavailable in restricted webviews */
  }
  return DEFAULT_CACHE_DIR;
}

export const cacheFolder = ref<string>(readFolder());

watch(cacheFolder, (value) => {
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    /* ignore */
  }
});

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
