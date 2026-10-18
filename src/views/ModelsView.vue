<script setup lang="ts">
import { computed, onActivated, onDeactivated, ref, watch } from "vue";
import { openPath } from "@tauri-apps/plugin-opener";
import { t } from "../lib/i18n";
import MeshViewer from "../components/MeshViewer.vue";
import { navigate } from "../lib/nav";
import { isTauri } from "../lib/tauri";
import {
  loadSkin,
  makeWorkspace,
  skinKeyOf,
  textureRole,
  texturesForMesh,
  unassignedTextures,
  type PreviewGeometry,
  type SkinMesh,
  type SkinPhase,
  type TextureEntry,
} from "../lib/preview";
import { characterOfEntry, label, skinLabel, type CatalogEntry } from "../lib/roster";
import { selectedSkin } from "../lib/selection";

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
/** 当前查看贴图的子网格；null 时看「共享贴图」或空态。 */
const activeMesh = ref<string | null>(null);
const activeShared = ref(false);
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
const textureGroups = computed(() => {
  const groups = new Map<string, TextureEntry[]>();
  for (const mesh of meshes.value) {
    groups.set(mesh.name, texturesForMesh(mesh.name, textures.value));
  }
  return groups;
});

/** Textures no submesh claims (face/eye maps are often here). */
const sharedTextures = computed(() =>
  unassignedTextures(meshes.value, textures.value),
);

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
  maps.value = {};
  activeMesh.value = null;
  activeShared.value = false;
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
    maps.value = data.maps;
    // 默认全不选：用户只勾自己要改的那几个，导出和回包都只动它们
    pickedMeshes.value = [];
    pickedTextures.value = [];
    if (data.cached) cacheHint.value = t("models.studio.cached", { time: stamp(data.builtAt) });
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
    return;
  }
  if (entry.id === loadedId) return;
  void openStudio(entry);
}

let active = false;
onActivated(() => {
  active = true;
  sync(selectedSkin.value);
});
onDeactivated(() => {
  active = false;
});
watch(selectedSkin, (entry) => {
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
  if (!exportMeshes.length) {
    status.value = { kind: "err", text: t("models.studio.pickOne") };
    return;
  }
  busy.value = t("models.making");
  status.value = null;
  try {
    const result = await makeWorkspace(entry, label(entry), exportMeshes, exportTextures);
    status.value = {
      kind: "ok",
      text: t("models.made", { meshes: result.meshes, textures: result.textures, dir: result.dir }),
    };
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
            class="btn btn--accent"
            type="button"
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

  .studio__hint {
    max-width: 220px;
  }

  .texgrid {
    grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  }
}
</style>
