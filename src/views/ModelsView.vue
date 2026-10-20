<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from "vue";
import { openPath } from "@tauri-apps/plugin-opener";
import { t } from "../lib/i18n";
import Dropdown from "../components/Dropdown.vue";
import MeshViewer from "../components/MeshViewer.vue";
import type { DropdownOption } from "../lib/dropdown";
import { navigate } from "../lib/nav";
import { isTauri } from "../lib/tauri";
import {
  animationsFor,
  ensureAnimIndex,
  loadSkin,
  makeWorkspace,
  skinKeyOf,
  textureRole,
  texturesForMesh,
  type AnimEntry,
  type PreviewGeometry,
  type SkinMesh,
  type SkinPhase,
  type TextureEntry,
} from "../lib/preview";
import {
  characterOfEntry,
  findEntry,
  label,
  skinLabel,
  type CatalogEntry,
} from "../lib/roster";
import { selectedSkin, selectSkin } from "../lib/selection";
import {
  applyMods,
  listMods,
  readGeneratedMod,
  setModEnabled,
  writeMod,
  type GeneratedMod,
} from "../lib/modbox";
import {
  listWorkspaces,
  readActiveWorkspaceId,
  writeActiveWorkspace,
  type WorkspaceSummary,
} from "../lib/workspaces";

// ------------------------------------------------------------------------- //
// studio: every submesh + every texture of the selected skin, one 3D scene
// ------------------------------------------------------------------------- //
const meshes = ref<SkinMesh[]>([]);
const geometries = ref<PreviewGeometry[]>([]);
const hidden = ref<string[]>([]);
const pickedMeshes = ref<string[]>([]);
const pickedTextures = ref<string[]>([]);
const textures = ref<TextureEntry[]>([]);
const maps = ref<Record<string, string>>({});
/** 这个皮肤的动作：只做参考（Mod 不打包动作），导出给 Blender 播来看蒙皮。 */
const animations = ref<AnimEntry[]>([]);
const pickedAnims = ref<string[]>([]);
const animsOpen = ref(false);
const animQuery = ref("");
const animBusy = ref(false);
/** 全库动作索引很大（9613 个包 / 4 万个名字），只在用户展开时才建。 */
const animsIndexed = ref(false);
/** 当前查看贴图的子网格；null 时看「共享贴图」或空态。 */
const activeMesh = ref<string | null>(null);
const activeShared = ref(false);
/** 工作空间：一个工作空间只属于一个立绘，一个立绘可以有多个工作空间。 */
const workspaces = ref<WorkspaceSummary[]>([]);
const activeWorkspace = ref<WorkspaceSummary | null>(null);
/** Blender 侧「生成 Mod」写出的 <工作空间>/GeneratedMod 内容。 */
const generated = ref<GeneratedMod | null>(null);
const installedAt = ref("");
let pendingWorkspace: WorkspaceSummary | null = null;
let workspaceInit = false;
/** Serialize EXMTConfig writes so a late "clear" can't overwrite a newer pick. */
let workspaceWrite: Promise<void> = Promise.resolve();
const busy = ref("");
const note = ref("");
const cacheHint = ref("");
const status = ref<{ kind: "info" | "ok" | "err"; text: string } | null>(null);
let token = 0;
let loadedId = "";
/** `c_AlvaSSR01` — the asset-name prefix shared by this skin's meshes and textures. */
let skinKey = "";

const character = computed(() => characterOfEntry(selectedSkin.value));
const title = computed(() => character.value?.display ?? "");
const skinName = computed(() =>
  selectedSkin.value ? skinLabel(selectedSkin.value) : "",
);
const skinCode = computed(() => selectedSkin.value?.name ?? "");

/** mesh name → textures whose names share its part stem. */
const skinGroups = ref<Record<string, string[]>>({});

const textureGroups = computed(() => {
  const groups = new Map<string, TextureEntry[]>();
  const byName = new Map(textures.value.map((tex) => [tex.name, tex]));
  const decided = skinGroups.value;
  for (const mesh of meshes.value) {
    const names = decided[mesh.name];
    if (names) {
      // the core paired this submesh: trust it, including "matched nothing"
      groups.set(
        mesh.name,
        names.map((name) => byName.get(name)).filter((tex): tex is TextureEntry => !!tex),
      );
    } else {
      groups.set(mesh.name, texturesForMesh(mesh.name, textures.value));
    }
  }
  return groups;
});

/** Textures no submesh claims (face/eye maps are often here). */
const sharedTextures = computed(() => {
  const assigned = new Set<string>();
  for (const list of textureGroups.value.values()) for (const tex of list) assigned.add(tex.name);
  return textures.value.filter((tex) => !assigned.has(tex.name));
});

const activeTextures = computed(() => {
  if (activeShared.value) return sharedTextures.value;
  if (!activeMesh.value) return [];
  return textureGroups.value.get(activeMesh.value) ?? [];
});

const activeLabel = computed(() => {
  if (activeShared.value) return t("models.studio.shared");
  return activeMesh.value ? shortName(activeMesh.value) : "";
});

/** 全部子网格 + 全部贴图都已纳入导出。 */
const allPicked = computed(
  () =>
    (meshes.value.length > 0 || textures.value.length > 0) &&
    pickedMeshes.value.length === meshes.value.length &&
    pickedTextures.value.length === textures.value.length,
);

/** 动作列表：按搜索词过滤（`c_<皮肤>_` 前缀已在 shortName 里去掉了）。 */
const visibleAnims = computed(() => {
  const q = animQuery.value.trim().toLowerCase();
  if (!q) return animations.value;
  return animations.value.filter((clip) => clip.name.toLowerCase().includes(q));
});

const allAnimsPicked = computed(
  () =>
    animations.value.length > 0 &&
    pickedAnims.value.length === animations.value.length,
);

/** 按钮上的计数：选了就报选了几个，没选就报这个皮肤一共有几个。 */
const animCountLabel = computed(() =>
  pickedAnims.value.length
    ? t("models.anims.picked", { n: pickedAnims.value.length, total: animations.value.length })
    : String(animations.value.length),
);

const selectedWorkspaceId = computed(() => activeWorkspace.value?.id ?? "");

const workspaceOptions = computed<DropdownOption[]>(() => [
  {
    value: "",
    label: workspaces.value.length
      ? t("models.workspace.none")
      : t("models.workspace.empty"),
  },
  ...workspaces.value.map((ws) => ({ value: ws.id, label: ws.label })),
]);

const hint = computed(() => {
  if (busy.value) return busy.value;
  if (note.value) return note.value;
  if (activeShared.value) {
    return t("models.studio.viewingShared", { n: sharedTextures.value.length });
  }
  if (activeMesh.value) {
    return t("models.studio.viewing", {
      name: shortName(activeMesh.value),
      n: activeTextures.value.length,
    });
  }
  return cacheHint.value || t("models.studio.pickSubmesh");
});

function shortName(name: string) {
  const prefix = skinKey ? `${skinKey}_` : "";
  return prefix && name.startsWith(prefix) ? name.slice(prefix.length) : name;
}

function textureCount(name: string) {
  return textureGroups.value.get(name)?.length ?? 0;
}

function textureMeta(tex: TextureEntry) {
  const size = tex.width && tex.height ? `${tex.width}×${tex.height}` : "";
  const role = textureRole(tex.name).toUpperCase();
  return [size, role].filter(Boolean).join(" · ");
}

function activateMesh(name: string) {
  activeShared.value = false;
  activeMesh.value = name;
}

function activateShared() {
  activeMesh.value = null;
  activeShared.value = true;
}

function compact(n: number) {
  if (n >= 10000) return `${(n / 1000).toFixed(1)}k`;
  if (n >= 1000) return `${(n / 1000).toFixed(2)}k`;
  return String(n);
}

function toggleMesh(name: string) {
  hidden.value = hidden.value.includes(name)
    ? hidden.value.filter((item) => item !== name)
    : [...hidden.value, name];
}

/** 勾选 = 会导出；默认全不勾，用户只勾想改的那几个。 */
function togglePickMesh(name: string) {
  if (pickedMeshes.value.includes(name)) {
    pickedMeshes.value = pickedMeshes.value.filter((item) => item !== name);
    return;
  }
  pickedMeshes.value = [...pickedMeshes.value, name];
  // 勾选子网格时顺便展开它的贴图，少点一次
  activateMesh(name);
}

function togglePickTexture(name: string) {
  if (pickedTextures.value.includes(name)) {
    pickedTextures.value = pickedTextures.value.filter((item) => item !== name);
    return;
  }
  pickedTextures.value = [...pickedTextures.value, name];
  // 勾贴图就意味着要改这个子网格；顺手把它纳入导出，避免“只勾贴图不能制作”
  if (activeMesh.value && !activeShared.value && !pickedMeshes.value.includes(activeMesh.value)) {
    pickedMeshes.value = [...pickedMeshes.value, activeMesh.value];
  }
}

/** 一键全选：所有子网格 + 所有贴图；再点一次清空。 */
function toggleSelectAll() {
  if (allPicked.value) {
    pickedMeshes.value = [];
    pickedTextures.value = [];
    return;
  }
  pickedMeshes.value = meshes.value.map((mesh) => mesh.name);
  pickedTextures.value = textures.value.map((tex) => tex.name);
  // 没在看任何贴图时，顺手打开第一个有贴图的子网格，避免中间还是空的
  if (!activeMesh.value && !activeShared.value && meshes.value.length) {
    const first =
      meshes.value.find((mesh) => textureCount(mesh.name) > 0) ?? meshes.value[0];
    activateMesh(first.name);
  }
}

// ------------------------------------------------------------------------- //
// animations: this skin's clips, exported as reference for watching skinning
// ------------------------------------------------------------------------- //
/**
 * 懒加载：动作名要扫一遍游戏目录（9613 个包 / 63 秒，只解元数据），所以不跟着
 * 皮肤一起加载，等用户真的展开「动作」才建索引；之后走 `<cache>/animindex.json`。
 */
async function loadAnimations() {
  if (!skinKey || !isTauri) return;
  const mine = token;
  animBusy.value = true;
  if (!animsIndexed.value) note.value = t("models.anims.indexing");
  try {
    const index = await ensureAnimIndex();
    animsIndexed.value = true;
    if (mine !== token) return;
    animations.value = animationsFor(index, skinKey);
    // 换皮肤后旧勾选必须丢掉，否则会把别的皮肤的动作名传进导出
    pickedAnims.value = pickedAnims.value.filter((name) =>
      animations.value.some((clip) => clip.name === name),
    );
  } catch (e) {
    if (mine === token) status.value = { kind: "err", text: String(e) };
  } finally {
    if (mine === token) {
      animBusy.value = false;
      note.value = "";
    }
  }
}

async function toggleAnimsPanel() {
  animsOpen.value = !animsOpen.value;
  if (animsOpen.value && !animsIndexed.value) await loadAnimations();
}

function togglePickAnim(name: string) {
  pickedAnims.value = pickedAnims.value.includes(name)
    ? pickedAnims.value.filter((item) => item !== name)
    : [...pickedAnims.value, name];
}

function toggleAllAnims() {
  pickedAnims.value = allAnimsPicked.value
    ? []
    : animations.value.map((clip) => clip.name);
}

// ------------------------------------------------------------------------- //
// workspaces: the exported folders Blender imports
// ------------------------------------------------------------------------- //
async function refreshWorkspaces() {
  if (!isTauri) return;
  try {
    const list = await listWorkspaces();
    workspaces.value = list;
    if (activeWorkspace.value) {
      const current = list.find((ws) => ws.id === activeWorkspace.value?.id);
      if (current) {
        // keep the label/counts in sync with the manifest on disk
        activeWorkspace.value = current;
      } else {
        activeWorkspace.value = null;
        void persistWorkspace(null);
      }
    }
  } catch {
    // keep the previous list; the selector still works with what we had
  }
}

async function persistWorkspace(ws: WorkspaceSummary | null): Promise<boolean> {
  if (!isTauri) return true;
  const run = workspaceWrite.then(() => writeActiveWorkspace(ws));
  workspaceWrite = run.catch(() => undefined);
  try {
    await run;
    return true;
  } catch (e) {
    status.value = {
      kind: "err",
      text: t("models.workspace.writeFail", { error: String(e) }),
    };
    return false;
  }
}

async function setActiveWorkspace(ws: WorkspaceSummary | null): Promise<boolean> {
  activeWorkspace.value = ws;
  await refreshGenerated();
  return persistWorkspace(ws);
}

async function clearActiveWorkspace() {
  pendingWorkspace = null;
  await setActiveWorkspace(null);
}

// ------------------------------------------------------------------------- //
// generated mod: Blender packs the edits into <workspace>/GeneratedMod
// ------------------------------------------------------------------------- //
/** Look for a staged Mod next to the active workspace and its installed state. */
async function refreshGenerated() {
  generated.value = null;
  installedAt.value = "";
  const ws = activeWorkspace.value;
  if (!isTauri || !ws) return;
  generated.value = await readGeneratedMod(ws.dir).catch(() => null);
  if (!generated.value) return;
  const records = await listMods().catch(() => []);
  const hit = records.find((rec) => rec.workspace === ws.id);
  // a disabled record still exists, but the button has to say 「加载到游戏」
  installedAt.value = hit && hit.enabled !== false ? hit.installed_at ?? "" : "";
}

/** Local `YYYY-MM-DD HH:MM` for tooltips (the stored ISO is UTC). */
function localStamp(iso: string) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * 生成 Mod → 加载到游戏。
 *
 * 不再"直接覆盖游戏文件"：把这份 Mod 登记进 Mod 库并启用，然后由整合器把**所有启用
 * 的 Mod** 一起打进一份新的包。这样两个 Mod 改到同一个包时不会互相覆盖。
 */
async function loadIntoGame() {
  const ws = activeWorkspace.value;
  if (!ws) return;
  busy.value = t("models.loading");
  status.value = null;
  // re-read mod.json: the user may have pressed 生成 Mod in Blender after this page
  // loaded, and a stale patch list silently drops whatever changed (e.g. textures)
  const mod = await readGeneratedMod(ws.dir);
  generated.value = mod;
  if (!mod) {
    busy.value = "";
    status.value = { kind: "err", text: t("models.noGenerated") };
    return;
  }
  try {
    const at = new Date().toISOString();
    const id = `ws-${ws.id}`;
    await writeMod({
      id,
      kind: "workspace",
      character: ws.entry.display || ws.entry.name || ws.label,
      mesh: `${mod.mesh_count ?? 0} 个网格`,
      bundle: mod.bundles[0]?.hash,
      bundles: mod.bundles.map((b) => b.hash),
      workspace: ws.id,
      source: mod.dir,
      installed_at: at,
      patches: mod.patches ?? [],
      textures: mod.textures ?? [],
      enabled: true,
    });
    const result = await applyMods();
    installedAt.value = at;
    status.value = {
      kind: result.errors.length ? "err" : "ok",
      text: result.errors.length
        ? t("models.loadFail", { error: result.errors.join("；") })
        : t("models.loaded", {
            mods: result.mods,
            bundles: result.bundles,
            meshes: result.meshes,
            textures: result.textures,
          }),
    };
  } catch (e) {
    status.value = { kind: "err", text: t("models.loadFail", { error: String(e) }) };
  } finally {
    busy.value = "";
  }
}

/** 卸下：停用这份 Mod，整合器会把没人再改的包还原成原始文件。 */
async function unloadFromGame() {
  const ws = activeWorkspace.value;
  if (!ws) return;
  busy.value = t("models.unloading");
  try {
    const id = `ws-${ws.id}`;
    const result = await setModEnabled(id, false);
    installedAt.value = "";
    status.value = {
      kind: "ok",
      text: t("models.unloaded", { n: result.restored, mods: result.mods }),
    };
  } catch (e) {
    status.value = { kind: "err", text: String(e) };
  } finally {
    busy.value = "";
  }
}

/** Restore the workspace's exported mesh/texture selection once its skin is loaded. */
function applyWorkspaceSelection(ws: WorkspaceSummary) {
  if (!meshes.value.length || loadedId !== ws.entry.id) {
    pendingWorkspace = ws;
    return;
  }
  pendingWorkspace = null;
  const meshSet = new Set(meshes.value.map((mesh) => mesh.name));
  const texSet = new Set(textures.value.map((tex) => tex.name));
  pickedMeshes.value = ws.meshNames.filter((name) => meshSet.has(name));
  pickedTextures.value = ws.textureNames.filter((name) => texSet.has(name));
  const first =
    pickedMeshes.value.find((name) => textureCount(name) > 0) ??
    pickedMeshes.value[0] ??
    null;
  activeMesh.value = first;
  activeShared.value = false;
}

/** Selecting a workspace is also selecting its skin (立绘). */
async function chooseWorkspace(id: string) {
  const ws = workspaces.value.find((item) => item.id === id);
  if (!ws) return;
  const entry = findEntry(ws.entry.id);
  if (!entry) {
    status.value = {
      kind: "err",
      text: t("models.workspace.missingSkin", {
        name: ws.entry.display || ws.entry.name || ws.entry.id,
      }),
    };
    return;
  }
  activeWorkspace.value = ws;
  await persistWorkspace(ws);
  if (selectedSkin.value?.id !== entry.id) {
    pendingWorkspace = ws;
    selectSkin(entry);
    return;
  }
  applyWorkspaceSelection(ws);
}

async function onWorkspaceChange(id: string) {
  if (!id) {
    await clearActiveWorkspace();
    return;
  }
  await chooseWorkspace(id);
}

/** Load the workspace list and restore the last active one on first activation. */
async function initWorkspaces() {
  await refreshWorkspaces();
  if (workspaceInit) {
    // already restored: just re-check what Blender may have written meanwhile
    await refreshGenerated();
    return;
  }
  workspaceInit = true;
  if (!isTauri || activeWorkspace.value) {
    await refreshGenerated();
    return;
  }
  const id = await readActiveWorkspaceId();
  if (!id || activeWorkspace.value) {
    await refreshGenerated();
    return;
  }
  const ws = workspaces.value.find((item) => item.id === id) ?? null;
  if (!ws) {
    await persistWorkspace(null);
    await refreshGenerated();
    return;
  }
  if (ws.entry.id !== selectedSkin.value?.id) {
    // Don't let a workspace from another skin override the home selection.
    await persistWorkspace(null);
    await refreshGenerated();
    return;
  }
  activeWorkspace.value = ws;
  await persistWorkspace(ws);
  applyWorkspaceSelection(ws);
  // only now is there a workspace whose GeneratedMod can be looked up
  await refreshGenerated();
}

function stamp(iso: string | null) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function clearStudio() {
  meshes.value = [];
  geometries.value = [];
  hidden.value = [];
  pickedMeshes.value = [];
  pickedTextures.value = [];
  textures.value = [];
  skinGroups.value = {};
  maps.value = {};
  activeMesh.value = null;
  activeShared.value = false;
  // 动作跟着皮肤走：列表和勾选都清掉，展开了就重新查这个皮肤的
  animations.value = [];
  pickedAnims.value = [];
  note.value = "";
  cacheHint.value = "";
  status.value = null;
}

async function openStudio(entry: CatalogEntry) {
  const mine = ++token;
  loadedId = entry.id;
  skinKey = skinKeyOf(entry);
  clearStudio();
  if (!isTauri) {
    note.value = t("models.studio.browser");
    return;
  }

  const phase = (p: SkinPhase) =>
    p === "geometry"
      ? t("models.studio.geometry")
      : p === "textures"
        ? t("models.studio.indexing")
        : t("models.studio.loading");

  busy.value = t("models.studio.loading");
  try {
    const data = await loadSkin(entry, (p) => {
      if (mine === token) busy.value = phase(p);
    });
    if (mine !== token) return;
    meshes.value = data.meshes;
    geometries.value = data.geometry;
    textures.value = data.textures;
    skinGroups.value = data.groups ?? {};
    maps.value = data.maps;
    // 默认全不选：用户只勾自己要改的那几个，导出和回包都只动它们
    pickedMeshes.value = [];
    pickedTextures.value = [];
    if (pendingWorkspace) {
      if (pendingWorkspace.entry.id === entry.id) {
        applyWorkspaceSelection(pendingWorkspace);
      } else {
        pendingWorkspace = null;
      }
    }
    if (data.cached) cacheHint.value = t("models.studio.cached", { time: stamp(data.builtAt) });
    if (animsOpen.value) void loadAnimations();
    if (!data.meshes.length) {
      note.value = t("models.studio.noMesh");
      // 没有子网格时至少让共享贴图可见，方便单独导出
      if (data.textures.length) activeShared.value = true;
    } else if (!data.textures.length) {
      note.value = t("models.studio.noTex");
    }
  } catch (e) {
    if (mine === token) note.value = String(e);
  } finally {
    if (mine === token) busy.value = "";
  }
}

/** Only reload when the studio is actually visible: home may click through many cards. */
function sync(entry: CatalogEntry | null) {
  if (!entry) {
    loadedId = "";
    token += 1;
    skinKey = "";
    clearStudio();
    activeWorkspace.value = null;
    pendingWorkspace = null;
    return;
  }
  if (entry.id === loadedId) return;
  void openStudio(entry);
}

let active = false;
onActivated(() => {
  active = true;
  void initWorkspaces();
  sync(selectedSkin.value);
});
onDeactivated(() => {
  active = false;
});
// Blender writes GeneratedMod while this window is in the background: pick it up
// when the user comes back instead of making them re-select the workspace.
function onWindowFocus() {
  if (active) void refreshGenerated();
}
onMounted(() => window.addEventListener("focus", onWindowFocus));
onUnmounted(() => window.removeEventListener("focus", onWindowFocus));
watch(selectedSkin, (entry) => {
  if (!entry) {
    activeWorkspace.value = null;
    pendingWorkspace = null;
  } else if (
    activeWorkspace.value &&
    activeWorkspace.value.entry.id !== entry.id
  ) {
    // The user changed skin elsewhere; the old workspace no longer matches.
    void clearActiveWorkspace();
  }
  if (active) sync(entry);
});

async function startMaking() {
  const entry = selectedSkin.value;
  if (!entry || !meshes.value.length) return;
  const exportMeshes = meshes.value.filter((mesh) => pickedMeshes.value.includes(mesh.name));
  // 缩略图解码失败不代表贴图不能导出；能不能写出来交给核心的 failed_textures
  const exportTextures = textures.value.filter((tex) =>
    pickedTextures.value.includes(tex.name),
  );
  // 动作只是参考：勾了就导出到 animations/，没勾就完全不动（也不影响打包）
  const exportAnims = animations.value.filter((clip) =>
    pickedAnims.value.includes(clip.name),
  );
  if (!exportMeshes.length) {
    status.value = { kind: "err", text: t("models.studio.pickOne") };
    return;
  }
  busy.value = t("models.making");
  status.value = null;
  generated.value = null;
  installedAt.value = "";
  try {
    const result = await makeWorkspace(
      entry,
      label(entry),
      exportMeshes,
      exportTextures,
      exportAnims,
      // 带动作时骨架要按这个皮肤全部网格的并集建（动作驱动整副骨架）
      exportAnims.length ? meshes.value : exportMeshes,
      skinGroups.value,
    );
    await refreshWorkspaces();
    const summary: WorkspaceSummary =
      workspaces.value.find((ws) => ws.id === result.id) ?? {
        id: result.id,
        label: result.label,
        dir: result.dir,
        entry: { id: entry.id, name: entry.name, display: label(entry) },
        createdAt: new Date().toISOString(),
        meshNames: exportMeshes.map((mesh) => mesh.name),
        textureNames: exportTextures.map((tex) => tex.name),
        meshCount: result.meshes,
        textureCount: result.textures,
      };
    const persisted = await setActiveWorkspace(summary);
    if (persisted) {
      status.value = {
        kind: "ok",
        text: t("models.made", {
          meshes: result.meshes,
          textures: result.textures,
          anims: result.animations ?? 0,
          bones: result.bones ?? 0,
          named: result.named_bones ?? 0,
          label: result.label,
          dir: result.dir,
        }),
      };
    }
    // 导出完成后直接打开工作空间，省得用户自己找路径
    await openPath(result.dir).catch(() => undefined);
  } catch (e) {
    status.value = { kind: "err", text: String(e) };
  } finally {
    busy.value = "";
  }
}
</script>

<template>
  <section class="page">
    <div class="studio-page">
      <header class="studio__bar">
        <div class="studio__id">
          <h2 class="studio__title">{{ title }}</h2>
          <span v-if="skinName && skinName !== title" class="chip chip--accent">
            {{ skinName }}
          </span>
          <span class="chip">
            {{ t("models.studio.counts", { meshes: meshes.length, textures: textures.length }) }}
          </span>
          <span v-if="skinCode" class="studio__code mono">{{ skinCode }}</span>
          <span class="workspace-pick" :title="t('models.workspace.hint')">
            <span class="workspace-pick__label">{{ t("models.workspace.label") }}</span>
            <span class="workspace-pick__control">
              <Dropdown
                :model-value="selectedWorkspaceId"
                :options="workspaceOptions"
                :disabled="!!busy || !workspaces.length"
                :aria-label="t('models.workspace.label')"
                :title="activeWorkspace?.label ?? t('models.workspace.none')"
                @change="onWorkspaceChange"
              />
            </span>
          </span>
        </div>
        <span class="studio__hint" :title="hint">{{ hint }}</span>
        <div class="studio__bar-actions">
          <button
            class="btn"
            type="button"
            :title="t('models.studio.selectAllHint')"
            :disabled="!!busy || (!meshes.length && !textures.length)"
            @click="toggleSelectAll"
          >
            {{ allPicked ? t("models.studio.clearAll") : t("models.studio.selectAll") }}
          </button>
          <button class="btn" type="button" @click="navigate('home')">
            <span class="glyph">&#xE72B;</span>
            {{ t("models.changeSkin") }}
          </button>
          <button
            v-if="installedAt"
            class="btn"
            type="button"
            :title="t('models.unloadHint', { time: localStamp(installedAt) })"
            :disabled="!!busy"
            @click="unloadFromGame"
          >
            <span class="glyph">&#xE777;</span>
            {{ t("models.unload") }}
          </button>
          <button
            v-else
            class="btn"
            type="button"
            :title="generated ? t('models.loadHint') : t('models.loadEmpty')"
            :disabled="!!busy || !generated"
            @click="loadIntoGame"
          >
            <span class="glyph">&#xE896;</span>
            {{ t("models.load") }}
          </button>
          <button
            class="btn btn--accent"
            type="button"
            :title="t('models.makeHint')"
            :disabled="!!busy || !pickedMeshes.length"
            @click="startMaking"
          >
            <span class="glyph">&#xE768;</span>
            {{ t("models.make") }}
          </button>
        </div>
      </header>

      <p v-if="status" class="status-line" :class="'status-line--' + status.kind">
        {{ status.kind === "ok" ? "✓" : status.kind === "err" ? "✗" : "·" }} {{ status.text }}
      </p>

      <div class="studio__grid">
        <!-- 左：子网格。点一行 = 看它的贴图；复选框 = 纳入导出 -->
        <section class="studio__meshes">
          <div class="studio__head">
            <span>{{ t("models.studio.meshes") }}</span>
            <span class="chip">{{ meshes.length }}</span>
          </div>
          <div class="meshline-list">
            <div
              v-for="mesh in meshes"
              :key="mesh.name"
              class="meshline"
              :class="{
                'meshline--off': hidden.includes(mesh.name),
                'meshline--picked': pickedMeshes.includes(mesh.name),
                'meshline--active': activeMesh === mesh.name,
              }"
            >
              <input
                class="meshline__check"
                type="checkbox"
                :checked="pickedMeshes.includes(mesh.name)"
                :aria-label="shortName(mesh.name)"
                :title="t('models.studio.pick')"
                @change="togglePickMesh(mesh.name)"
              />
              <button
                class="meshline__eye"
                type="button"
                :title="t('models.studio.toggle')"
                :aria-label="t('models.studio.toggle')"
                @click="toggleMesh(mesh.name)"
              >
                <span class="glyph">{{ hidden.includes(mesh.name) ? "\uED1A" : "\uE890" }}</span>
              </button>
              <button
                class="meshline__body"
                type="button"
                :aria-pressed="activeMesh === mesh.name"
                @click="activateMesh(mesh.name)"
              >
                <span class="meshline__name mono">{{ shortName(mesh.name) }}</span>
                <span class="meshline__meta">
                  {{ t("models.studio.verts", { n: compact(mesh.verts), m: compact(mesh.tris) }) }}
                  · {{ t("models.studio.texCount", { n: textureCount(mesh.name) }) }}
                </span>
              </button>
            </div>

            <template v-if="sharedTextures.length">
              <div class="meshline-sep" role="separator"></div>
              <button
                class="meshline meshline--shared"
                :class="{ 'meshline--active': activeShared }"
                type="button"
                :title="t('models.studio.sharedHint')"
                :aria-pressed="activeShared"
                @click="activateShared"
              >
                <span class="glyph meshline__shared-icon">&#xE8B9;</span>
                <span class="meshline__name">{{ t("models.studio.shared") }}</span>
                <span class="meshline__texcount">{{ sharedTextures.length }}</span>
              </button>
            </template>

            <p v-if="!meshes.length && !busy" class="studio__empty">
              {{ t("models.studio.noMesh") }}
            </p>
          </div>
        </section>

        <!-- 中：当前子网格关联的贴图，勾选 = 导出 -->
        <section class="studio__textures">
          <div class="studio__head">
            <span class="studio__head-main">
              {{ activeShared ? t("models.studio.shared") : t("models.studio.textures") }}
              <span v-if="!activeShared && activeLabel" class="studio__active mono">
                · {{ activeLabel }}
              </span>
            </span>
            <span class="chip">{{ activeTextures.length }}</span>
          </div>

          <div v-if="activeTextures.length" class="texgrid">
            <button
              v-for="tex in activeTextures"
              :key="tex.name"
              type="button"
              class="texcard"
              :class="{ 'texcard--picked': pickedTextures.includes(tex.name) }"
              :title="tex.name"
              :aria-label="tex.name"
              :aria-pressed="pickedTextures.includes(tex.name)"
              @click="togglePickTexture(tex.name)"
            >
              <span class="texcard__frame">
                <img
                  v-if="tex.png"
                  class="texcard__img"
                  :src="`data:image/png;base64,${tex.png}`"
                  :alt="tex.name"
                />
                <span v-else class="texcard__ph">?</span>
                <span class="texcard__check" aria-hidden="true">
                  {{ pickedTextures.includes(tex.name) ? "✓" : "" }}
                </span>
              </span>
              <span class="texcard__caption">
                <span class="texcard__name mono">{{ shortName(tex.name) }}</span>
                <span class="texcard__meta">{{ textureMeta(tex) }}</span>
              </span>
            </button>
          </div>

          <p v-else-if="busy" class="studio__empty">{{ busy }}</p>
          <p v-else-if="!activeMesh && !activeShared" class="studio__empty studio__empty--center">
            {{ t("models.studio.pickSubmesh") }}
          </p>
          <p v-else class="studio__empty">{{ t("models.studio.noSubmeshTex") }}</p>

          <p v-if="activeTextures.length" class="studio__note">
            {{ t("models.studio.texScope") }}
          </p>
        </section>

        <!-- 右：9:16 竖版 3D，人物站立时占满高度 -->
        <div class="studio__viewer">
          <MeshViewer
            :meshes="geometries"
            :hidden="hidden"
            :selected="pickedMeshes"
            :active="activeMesh"
            :maps="maps"
            :loading="!!busy"
          />
        </div>
      </div>

      <!-- 动作：皮肤级参考素材。勾了就随「开始制作」导出，Blender 里播来看蒙皮 -->
      <section class="studio__anims" :class="{ 'studio__anims--open': animsOpen }">
        <div class="studio__anims-bar">
          <button
            class="studio__anims-toggle"
            type="button"
            :aria-expanded="animsOpen"
            :title="t('models.anims.hint')"
            @click="toggleAnimsPanel"
          >
            <span class="studio__anims-caret" aria-hidden="true">{{ animsOpen ? "▾" : "▸" }}</span>
            {{ t("models.anims") }}
          </button>
          <span v-if="animsIndexed" class="chip">{{ animCountLabel }}</span>
          <span v-else class="studio__anims-note">{{ t("models.anims.lazy") }}</span>
          <template v-if="animsOpen">
            <input
              v-model="animQuery"
              class="studio__anims-search"
              type="search"
              :placeholder="t('models.anims.search')"
              :aria-label="t('models.anims.search')"
            />
            <button
              class="btn studio__anims-all"
              type="button"
              :disabled="animBusy || !animations.length"
              @click="toggleAllAnims"
            >
              {{ allAnimsPicked ? t("models.anims.clear") : t("models.anims.all") }}
            </button>
          </template>
        </div>
        <div v-if="animsOpen" class="studio__anims-list">
          <button
            v-for="clip in visibleAnims"
            :key="clip.name"
            type="button"
            class="animchip"
            :class="{ 'animchip--picked': pickedAnims.includes(clip.name) }"
            :aria-pressed="pickedAnims.includes(clip.name)"
            :title="clip.name"
            @click="togglePickAnim(clip.name)"
          >
            <span class="animchip__tick" aria-hidden="true">{{ pickedAnims.includes(clip.name) ? "✓" : "" }}</span>
            <span class="animchip__name mono">{{ shortName(clip.name) }}</span>
          </button>
          <p v-if="animBusy" class="studio__empty">{{ t("models.anims.loading") }}</p>
          <p v-else-if="animsIndexed && !animations.length" class="studio__empty">
            {{ t("models.anims.empty") }}
          </p>
          <p v-else-if="!visibleAnims.length" class="studio__empty">
            {{ t("models.anims.noMatch") }}
          </p>
        </div>
      </section>
    </div>
  </section>
</template>

<style scoped>
/* ---------- 工作台：子网格 / 贴图 / 竖版 3D ---------- */
.studio-page {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
  height: calc(100vh - var(--titlebar-height) - 50px);
  min-height: 430px;
}

.studio__bar {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 36px;
}

.studio__id {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}

.studio__title {
  font-family: var(--font-display);
  font-size: 18px;
  font-weight: 600;
  line-height: 26px;
  white-space: nowrap;
}

.studio__id .chip {
  align-self: center;
}

.studio__code {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.workspace-pick {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  align-self: center;
}

.workspace-pick__label {
  font-size: 11.5px;
  color: var(--text-2);
  white-space: nowrap;
}

.workspace-pick__control {
  display: block;
  width: 230px;
  min-width: 0;
}

.studio__hint {
  flex: 1 1 auto;
  min-width: 0;
  text-align: right;
  font-size: 11.5px;
  line-height: 18px;
  color: var(--text-2);
  opacity: 0.8;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.studio__bar-actions {
  display: flex;
  flex: none;
  gap: 8px;
}

/* 三栏：子网格 / 贴图 / 9:16 3D */
.studio__grid {
  display: flex;
  flex: 1;
  gap: 12px;
  min-width: 0;
  min-height: 0;
}

.studio__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
  font-size: 12px;
  color: var(--text-2);
}

.studio__head-main {
  display: flex;
  align-items: baseline;
  gap: 6px;
  min-width: 0;
}

.studio__active {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10.5px;
  color: var(--text-3);
}

.studio__meshes,
.studio__textures {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.studio__meshes {
  flex: 0 0 232px;
}

.studio__textures {
  flex: 1 1 auto;
}

/* 高度决定宽度：人物是站立的，画布固定 9:16 */
.studio__viewer {
  flex: none;
  height: 100%;
  aspect-ratio: 9 / 16;
  min-width: 0;
}

.meshline-list {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  min-height: 0;
  overflow-y: auto;
  padding-right: 4px;
}

.meshline {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 6px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: rgba(255, 255, 255, 0.03);
  transition: 120ms ease;
}

.meshline:hover {
  background: rgba(255, 255, 255, 0.07);
}

/* 勾选 = 会导出；没勾的保持灰色 */
.meshline--picked {
  border-color: rgba(76, 194, 255, 0.55);
  background: rgba(76, 194, 255, 0.08);
}

/* 当前正在看的那一行：压一层更亮的底，不等于“勾选导出” */
.meshline--active {
  background: rgba(76, 194, 255, 0.16);
}

.meshline--active:not(.meshline--picked) {
  border-color: rgba(76, 194, 255, 0.32);
}

.meshline--off {
  opacity: 0.45;
}

.meshline__check {
  flex: none;
  width: 13px;
  height: 13px;
  margin: 0;
  accent-color: var(--accent, #4cc2ff);
  cursor: pointer;
}

.meshline__eye {
  flex: none;
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-2, #9fb0c7);
  cursor: pointer;
}

.meshline__eye:hover {
  background: rgba(255, 255, 255, 0.1);
}

.meshline__eye .glyph {
  font-size: 13px;
}

.meshline__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.meshline__name {
  font-size: 11px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meshline__meta {
  font-size: 9px;
  opacity: 0.55;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 没有子网格认领的共享贴图 */
.meshline-sep {
  flex: none;
  height: 1px;
  margin: 4px 2px;
  background: var(--stroke, rgba(255, 255, 255, 0.08));
}

.meshline--shared {
  width: 100%;
  justify-content: flex-start;
  padding: 7px 6px;
  border-color: transparent;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.meshline--shared:hover {
  background: rgba(255, 255, 255, 0.07);
}

.meshline--shared.meshline--active {
  background: rgba(76, 194, 255, 0.16);
  border-color: rgba(76, 194, 255, 0.32);
}

.meshline__shared-icon {
  flex: none;
  width: 20px;
  text-align: center;
  font-size: 13px;
  color: var(--text-2);
}

.meshline__texcount {
  flex: none;
  margin-left: auto;
  min-width: 16px;
  padding: 0 4px;
  border-radius: 999px;
  background: var(--accent-soft, rgba(76, 194, 255, 0.12));
  color: var(--text-2);
  font-size: 9px;
  line-height: 15px;
  text-align: center;
}

/* 贴图卡：正方形，勾选在左上角 */
.texgrid {
  display: grid;
  flex: 1;
  grid-template-columns: repeat(auto-fill, minmax(156px, 1fr));
  justify-content: start;
  gap: 10px;
  align-content: start;
  align-items: start;
  /* 行高必须按内容算：否则固定高度会把卡片压扁、裁掉名称 */
  grid-auto-rows: max-content;
  min-height: 0;
  overflow-y: auto;
  padding: 2px 4px 6px 2px;
}

.texcard {
  width: 100%;
  max-width: 200px;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  opacity: 0.8;
  transition: 120ms ease;
}

.texcard:hover {
  opacity: 1;
}

/* 边框只包住 1:1 的贴图本身，名称在框外；这样“正方形”是硬性的 */
.texcard__frame {
  position: relative;
  display: block;
  width: 100%;
  padding-top: 100%;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  overflow: hidden;
  background:
    repeating-conic-gradient(rgba(255, 255, 255, 0.05) 0% 25%, transparent 0% 50%) 0 0 / 12px 12px,
    rgba(255, 255, 255, 0.02);
  transition: 120ms ease;
}

.texcard:hover .texcard__frame {
  border-color: var(--stroke-strong, rgba(255, 255, 255, 0.16));
}

.texcard--picked {
  opacity: 1;
}

.texcard--picked .texcard__frame {
  border-color: var(--accent, #4cc2ff);
  box-shadow: 0 0 0 1px var(--accent, #4cc2ff) inset;
}

.texcard__img,
.texcard__ph {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}

.texcard__img {
  object-fit: contain;
}

.texcard__ph {
  display: grid;
  place-items: center;
  font-size: 16px;
  opacity: 0.35;
}

.texcard__check {
  position: absolute;
  left: 5px;
  top: 5px;
  width: 18px;
  height: 18px;
  border: 1px solid rgba(255, 255, 255, 0.65);
  border-radius: 4px;
  background: rgba(8, 12, 18, 0.55);
  color: transparent;
  font-size: 11px;
  line-height: 16px;
  font-weight: 700;
  text-align: center;
}

.texcard--picked .texcard__check {
  background: var(--accent, #4cc2ff);
  border-color: var(--accent, #4cc2ff);
  color: var(--accent-contrast, #04121f);
}

.texcard__caption {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 5px 2px 0;
  min-width: 0;
}

.texcard__name {
  font-size: 9.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.texcard__meta {
  font-size: 8.5px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.studio__empty {
  margin: 4px 0 0;
  font-size: 11px;
  opacity: 0.55;
  line-height: 16px;
}

.studio__empty--center {
  flex: 1;
  display: grid;
  place-items: center;
  text-align: center;
  line-height: 1.8;
}

.studio__note {
  flex: none;
  margin: 6px 0 0;
  font-size: 9.5px;
  line-height: 14px;
  color: var(--text-3);
  opacity: 0.7;
}

.status-line {
  margin: 0;
  font-size: 12px;
  line-height: 18px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
  word-break: break-all;
}

.status-line--ok {
  color: #8ee6a8;
}

.status-line--err {
  color: #ff9b9b;
}

/* 鼠标点选不出现 focus 框 */
.meshline__body:focus,
.meshline__body:focus-visible,
.meshline__eye:focus,
.meshline__eye:focus-visible,
.meshline--shared:focus,
.meshline--shared:focus-visible,
.texcard:focus,
.texcard:focus-visible {
  outline: none;
}

@media (max-width: 1120px) {
  .studio__meshes {
    flex-basis: 200px;
  }

  .studio__code {
    display: none;
  }

  .workspace-pick__label {
    display: none;
  }

  .workspace-pick__control {
    width: 190px;
  }

  .studio__hint {
    max-width: 220px;
  }

  .texgrid {
    grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  }
}

/* 动作：默认只占一条 26px 的栏，展开后是横向滚动的名字条 */
.studio__anims {
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  border-top: 1px solid var(--stroke);
  padding-top: 8px;
}

.studio__anims-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.studio__anims-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.studio__anims-toggle:hover,
.studio__anims-toggle:focus-visible {
  outline: none;
  background: var(--fill-subtle-hover);
  color: var(--text-1);
}

.studio__anims-caret {
  width: 10px;
  font-size: 10px;
  color: var(--text-3);
}

.studio__anims-note {
  font-size: 11px;
  color: var(--text-3);
}

.studio__anims-search {
  flex: 1;
  min-width: 120px;
  max-width: 260px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--stroke);
  border-radius: 6px;
  background: var(--fill-subtle-pressed);
  color: var(--text-1);
  font-size: 12px;
}

.studio__anims-search:focus {
  outline: none;
  border-color: var(--accent);
}

.studio__anims-all {
  height: 26px;
  padding: 0 10px;
  font-size: 12px;
}

.studio__anims-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  max-height: 92px;
  overflow-y: auto;
  align-items: flex-start;
}

.animchip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 240px;
  height: 24px;
  padding: 0 8px 0 5px;
  border: 1px solid var(--stroke);
  border-radius: 999px;
  background: var(--fill-subtle-pressed);
  color: var(--text-2);
  font-size: 11px;
  cursor: pointer;
}

.animchip:hover {
  border-color: var(--stroke-strong);
  background: var(--fill-subtle-hover);
  color: var(--text-1);
}

.animchip--picked {
  border-color: var(--accent);
  background: var(--accent-soft-strong);
  color: var(--text-1);
}

.animchip__tick {
  width: 13px;
  height: 13px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  border: 1px solid var(--stroke-strong);
  font-size: 9px;
  line-height: 1;
  color: var(--accent-contrast);
}

.animchip--picked .animchip__tick {
  background: var(--accent);
  border-color: var(--accent);
}

.animchip__name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
</style>
