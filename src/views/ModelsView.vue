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
  type PreviewGeometry,
  type SkinMesh,
  type SkinPhase,
  type TextureEntry,
} from "../lib/preview";
import { characterOfEntry, label, skinLabel, type CatalogEntry } from "../lib/roster";
import { selectedSkin } from "../lib/selection";

// ------------------------------------------------------------------------- //
// studio: every mesh + every texture of the selected skin, one 3D scene
// ------------------------------------------------------------------------- //
const meshes = ref<SkinMesh[]>([]);
const geometries = ref<PreviewGeometry[]>([]);
const hidden = ref<string[]>([]);
const pickedMeshes = ref<string[]>([]);
const pickedTextures = ref<string[]>([]);
const textures = ref<TextureEntry[]>([]);
const maps = ref<Record<string, string>>({});
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

const hint = computed(
  () =>
    busy.value ||
    note.value ||
    (pickedMeshes.value.length
      ? cacheHint.value
      : t("models.studio.nothingPicked")),
);

function shortName(name: string) {
  const prefix = skinKey ? `${skinKey}_` : "";
  return prefix && name.startsWith(prefix) ? name.slice(prefix.length) : name;
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
  pickedMeshes.value = pickedMeshes.value.includes(name)
    ? pickedMeshes.value.filter((item) => item !== name)
    : [...pickedMeshes.value, name];
}

function togglePickTexture(name: string) {
  pickedTextures.value = pickedTextures.value.includes(name)
    ? pickedTextures.value.filter((item) => item !== name)
    : [...pickedTextures.value, name];
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
    if (!data.meshes.length) note.value = t("models.studio.noMesh");
    else if (!data.textures.length) note.value = t("models.studio.noTex");
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
  const exportTextures = textures.value.filter(
    (tex) => tex.png && pickedTextures.value.includes(tex.name),
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
              <button class="meshline__body" type="button" @click="togglePickMesh(mesh.name)">
                <span class="meshline__name mono">{{ shortName(mesh.name) }}</span>
                <span class="meshline__meta">
                  {{ t("models.studio.verts", { n: compact(mesh.verts), m: compact(mesh.tris) }) }}
                </span>
              </button>
            </div>
            <p v-if="!meshes.length && !busy" class="studio__empty">
              {{ t("models.studio.noMesh") }}
            </p>
          </div>
        </section>

        <div class="studio__viewer">
          <MeshViewer
            :meshes="geometries"
            :hidden="hidden"
            :selected="pickedMeshes"
            :maps="maps"
            :loading="!!busy"
          />
        </div>

        <section class="studio__textures">
          <div class="studio__head">
            <span>{{ t("models.studio.textures") }}</span>
            <span class="chip">{{ textures.length }}</span>
          </div>
          <div class="texgrid">
            <button
              v-for="tex in textures"
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
                <span
                  v-if="pickedTextures.includes(tex.name)"
                  class="texcard__tick"
                  aria-hidden="true"
                  >✓</span
                >
              </span>
              <span class="texcard__caption mono">{{ shortName(tex.name) }}</span>
            </button>
            <p v-if="!textures.length && !busy" class="studio__empty">
              {{ t("models.studio.noTex") }}
            </p>
          </div>
        </section>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* ---------- 工作台：网格 / 3D / 贴图 ---------- */
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

.studio__grid {
  display: grid;
  flex: 1;
  grid-template-columns: 232px minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr) 172px;
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

.studio__meshes,
.studio__textures {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.studio__meshes {
  grid-column: 1;
  grid-row: 1;
}

.studio__viewer {
  grid-column: 2;
  grid-row: 1;
  min-width: 0;
  min-height: 0;
}

.studio__textures {
  grid-column: 1 / -1;
  grid-row: 2;
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

/* 勾选 = 会导出；没勾的保持灰色，导出的那批在 3D 里也是这个蓝色 */
.meshline--picked {
  border-color: rgba(76, 194, 255, 0.55);
  background: rgba(76, 194, 255, 0.08);
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
  opacity: 0.5;
  white-space: nowrap;
}

/* 贴图是横向胶片条：正方形格子，1:1 由 padding-top 固定 */
.texgrid {
  display: flex;
  flex: 1;
  gap: 8px;
  min-height: 0;
  overflow-x: auto;
  overflow-y: hidden;
  padding: 2px 4px 8px 2px;
  align-items: flex-start;
}

.texcard {
  flex: 0 0 100px;
  margin: 0;
  padding: 0;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  overflow: hidden;
  background:
    repeating-conic-gradient(rgba(255, 255, 255, 0.05) 0% 25%, transparent 0% 50%) 0 0 / 12px 12px,
    rgba(255, 255, 255, 0.02);
  color: inherit;
  text-align: left;
  cursor: pointer;
  opacity: 0.72;
}

/* 选中的贴图加边框，未选中的压暗 */
.texcard--picked {
  opacity: 1;
  border-color: var(--accent, #4cc2ff);
  box-shadow: 0 0 0 1px var(--accent, #4cc2ff) inset;
}

.texcard__frame {
  position: relative;
  display: block;
  width: 100%;
  padding-top: 100%;
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

.texcard__tick {
  position: absolute;
  right: 4px;
  top: 4px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--accent, #4cc2ff);
  color: #04121f;
  font-size: 11px;
  line-height: 16px;
  font-weight: 700;
  text-align: center;
}

.texcard__caption {
  display: block;
  font-size: 8.5px;
  padding: 3px 5px;
  opacity: 0.65;
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

@media (max-width: 1120px) {
  .studio__grid {
    grid-template-columns: 200px minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) 164px;
  }

  .studio__code {
    display: none;
  }

  .studio__hint {
    max-width: 220px;
  }
}
</style>
