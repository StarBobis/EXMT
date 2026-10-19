/**
 * Workspaces: the folders the Models page exports and the Blender add-on imports.
 *
 * A workspace belongs to exactly one skin (立绘), but a skin may own many
 * workspaces. Each export gets its own timestamped folder, so "开始制作" never
 * overwrites an older session. The currently selected workspace is mirrored to
 * `%APPDATA%\<app>\EXMTConfig.json`, which is what the Blender panel reads for
 * its one-click import.
 */
import { appDataDir, join } from "@tauri-apps/api/path";
import {
  exists,
  mkdir,
  readDir,
  readTextFile,
  remove,
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import { cacheFolder } from "./settings";
import { boneNamesIndexPath } from "./preview";
import { isTauri } from "./tauri";

export interface WorkspaceEntry {
  id: string;
  name: string;
  display: string;
}

export interface WorkspaceSummary {
  id: string;
  label: string;
  dir: string;
  entry: WorkspaceEntry;
  createdAt: string;
  meshNames: string[];
  textureNames: string[];
  meshCount: number;
  textureCount: number;
}

export interface WorkspaceIdentity {
  id: string;
  label: string;
  createdAt: string;
}

interface WorkspaceManifest {
  format?: string;
  workspace_id?: string;
  label?: string;
  created_at?: string;
  entry?: Partial<WorkspaceEntry>;
  meshes?: { mesh?: string }[];
  textures?: { name?: string }[];
}

interface ExmtConfig {
  format: "exmt-config-1";
  app: string;
  updated_at: string;
  workspace_root: string;
  /** where the Blender add-on finds crc32(transform path) -> path */
  bone_names?: string;
  workspace: {
    id: string;
    label: string;
    dir: string;
    entry: WorkspaceEntry;
    created_at: string;
  } | null;
}

const CONFIG_NAME = "EXMTConfig.json";

export async function workspaceRoot(): Promise<string> {
  const root = await join(cacheFolder.value, "workspace");
  await mkdir(root, { recursive: true }).catch(() => undefined);
  return root;
}

/** `%APPDATA%\com.exmt.modbox\EXMTConfig.json` on Windows. */
export async function configPath(): Promise<string> {
  return join(await appDataDir(), CONFIG_NAME);
}

function pad(n: number) {
  return String(n).padStart(2, "0");
}

function shortTime(d: Date): string {
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function formatMoment(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : shortTime(d);
}

function entryOf(raw: Partial<WorkspaceEntry> | undefined, fallbackId: string): WorkspaceEntry {
  const id = String(raw?.id ?? fallbackId);
  const name = String(raw?.name ?? id);
  const display = String(raw?.display ?? name);
  return { id, name, display };
}

function labelFor(entry: WorkspaceEntry, createdAt: string): string {
  const display = entry.display || entry.name || entry.id || "工作空间";
  const time = formatMoment(createdAt);
  return time ? `${display} · ${time}` : display;
}

/** A unique id + human label for a new workspace of this skin. */
export async function newWorkspaceIdentity(
  entry: WorkspaceEntry,
): Promise<WorkspaceIdentity> {
  const root = await workspaceRoot();
  const now = new Date();
  const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  const base = `${entry.id}__${stamp}`;
  let attempt = 1;
  let id = base;
  while (await exists(await join(root, id))) {
    attempt += 1;
    id = `${base}-${attempt}`;
  }
  const suffix = attempt > 1 ? ` #${attempt}` : "";
  return {
    id,
    label: `${entry.display || entry.name || entry.id} · ${shortTime(now)}${suffix}`,
    createdAt: now.toISOString(),
  };
}

/**
 * Every workspace under `<cache>/workspace`, newest first.
 *
 * Old versions wrote one folder per skin (`c_AlvaSSR01`); those manifests have
 * no `workspace_id`/`label`, so the folder name and `created_at` are the
 * fallbacks. They remain selectable and are never overwritten by new exports.
 */
export async function listWorkspaces(): Promise<WorkspaceSummary[]> {
  if (!isTauri) return [];
  const root = await workspaceRoot();
  const dirs = await readDir(root).catch(() => []);
  const out: WorkspaceSummary[] = [];
  for (const item of dirs) {
    if (!item.isDirectory) continue;
    const dir = await join(root, item.name);
    const manifestPath = await join(dir, "workspace.json");
    if (!(await exists(manifestPath))) continue;
    try {
      const manifest = JSON.parse(await readTextFile(manifestPath)) as WorkspaceManifest;
      const entry = entryOf(manifest.entry, item.name);
      if (!entry.id) continue;
      const createdAt = String(manifest.created_at ?? "");
      const id = String(manifest.workspace_id || item.name);
      const meshNames = (manifest.meshes ?? [])
        .map((mesh) => String(mesh.mesh ?? ""))
        .filter(Boolean);
      const textureNames = (manifest.textures ?? [])
        .map((tex) => String(tex.name ?? ""))
        .filter(Boolean);
      out.push({
        id,
        // Regenerate the label from created_at so old full-date manifests and
        // new ones look the same in the dropdown (seconds must stay visible).
        label: createdAt
          ? labelFor(entry, createdAt)
          : String(manifest.label || entry.display || item.name),
        dir,
        entry,
        createdAt,
        meshNames,
        textureNames,
        meshCount: meshNames.length,
        textureCount: textureNames.length,
      });
    } catch {
      // one broken manifest must not hide the other workspaces
    }
  }
  out.sort(
    (a, b) => b.createdAt.localeCompare(a.createdAt) || b.id.localeCompare(a.id),
  );
  return out;
}

/**
 * The workspace id selected in a previous session.
 *
 * `EXMTConfig.json` wins. If it does not exist yet (first run after this
 * feature), fall back to the old `<cache>/workspace/current.json` so the
 * previous workspace is not lost.
 */
export async function readActiveWorkspaceId(): Promise<string | null> {
  if (!isTauri) return null;
  const path = await configPath();
  if (await exists(path)) {
    try {
      const cfg = JSON.parse(await readTextFile(path)) as ExmtConfig;
      return cfg.workspace?.id ?? null;
    } catch {
      return null;
    }
  }
  const current = await join(await workspaceRoot(), "current.json");
  if (!(await exists(current))) return null;
  try {
    const info = JSON.parse(await readTextFile(current)) as { id?: string };
    return info.id ?? null;
  } catch {
    return null;
  }
}

/**
 * Write the active workspace to AppData (for Blender) and keep the legacy
 * `current.json` pointer in the cache folder in sync.
 */
export async function writeActiveWorkspace(
  workspace: WorkspaceSummary | null,
): Promise<void> {
  if (!isTauri) return;
  const root = await workspaceRoot();
  const dir = await appDataDir();
  await mkdir(dir, { recursive: true }).catch(() => undefined);

  const payload: ExmtConfig = {
    format: "exmt-config-1",
    app: "EXMT Modbox",
    updated_at: new Date().toISOString(),
    workspace_root: root,
    bone_names: await boneNamesIndexPath().catch(() => ""),
    workspace: workspace
      ? {
          id: workspace.id,
          label: workspace.label,
          dir: workspace.dir,
          entry: workspace.entry,
          created_at: workspace.createdAt,
        }
      : null,
  };
  await writeTextFile(await configPath(), JSON.stringify(payload, null, 1));

  const current = await join(root, "current.json");
  if (workspace) {
    await writeTextFile(
      current,
      JSON.stringify(
        {
          id: workspace.id,
          dir: workspace.dir,
          label: workspace.label,
          entry: workspace.entry,
          updated_at: payload.updated_at,
        },
        null,
        1,
      ),
    );
  } else {
    await remove(current).catch(() => undefined);
  }
}
