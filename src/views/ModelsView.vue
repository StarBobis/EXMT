<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { openPath } from "@tauri-apps/plugin-opener";
import { t } from "../lib/i18n";
import MeshViewer from "../components/MeshViewer.vue";
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
import catalogData from "../data/catalog.json";
import artData from "../data/art.json";

interface CatalogEntry {
  id: string;
  name: string;
  display?: string;
  variant: string;
  bundle: string;
  parts: Record<string, string>;
  part_bundles?: Record<string, string[]>;
  portrait?: { bundle: string; texture: string } | null;
}

interface ArtItem {
  source: string;
  file: string;
  code: string;
  rarity: string;
  entry: string | null;
  entry_label?: string | null;
  label: string;
  size: number[];
}

interface CharArt {
  display: string;
  default: ArtItem | null;
  skins: ArtItem[];
}

interface Character {
  key: string;
  display: string;
  entries: CatalogEntry[];
  portraitEntry: CatalogEntry;
  art: CharArt | null;
}

const RARITY = /^(.*?)(SSR|SR|R)(\d*)$/;

/** VepleySSR01 -> Vepley ; VepleyDorm -> Vepley */
function charKeyOf(name: string) {
  if (name.endsWith("Dorm")) return name.slice(0, -4);
  const m = RARITY.exec(name);
  return m ? m[1] : name;
}

const catalog = ref<CatalogEntry[]>(catalogData.characters as CatalogEntry[]);
const art = artData as Record<string, CharArt>;

const characters = computed<Character[]>(() => {
  const map = new Map<string, Character>();
  for (const e of catalog.value) {
    const key = charKeyOf(e.name);
    let c = map.get(key);
    if (!c) {
      c = { key, display: e.display || key, entries: [], portraitEntry: e, art: art[key] ?? null };
      map.set(key, c);
    }
    c.entries.push(e);
    // prefer a slg entry, and the entry that owns the default illustration
    const better =
      (e.variant === "slg" && c.portraitEntry.variant !== "slg") ||
      (c.art?.default?.entry === e.id && c.art?.default?.entry !== c.portraitEntry.id);
    if (better) {
      c.portraitEntry = e;
      if (e.display) c.display = e.display;
    }
  }
  for (const c of map.values()) c.display = c.display.replace(/\s*·\s*宿舍$/, "");
  const list = [...map.values()];
  list.sort((a, b) => a.display.localeCompare(b.display, "zh-Hans-CN"));
  return list;
});

const query = ref("");
const selectedChar = ref<Character | null>(characters.value[0] ?? null);
const selected = ref<CatalogEntry | null>(selectedChar.value?.portraitEntry ?? null);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return characters.value;
  return characters.value.filter((c) => {
    if (c.key.toLowerCase().includes(q)) return true;
    if (c.display.toLowerCase().includes(q)) return true;
    return c.entries.some((e) => e.id.toLowerCase().includes(q) || e.name.toLowerCase().includes(q));
  });
});

function entryById(id: string | null | undefined) {
  if (!id) return null;
  return catalog.value.find((e) => e.id === id) ?? null;
}

function pickChar(c: Character) {
  selectedChar.value = c;
  const preferred =
    entryById(c.art?.default?.entry) ??
    c.entries.find((e) => e.variant === "slg") ??
    c.entries[0] ??
    null;
  selected.value = preferred;
}

/** every skin of the character gets a card; skins without art show a placeholder card */
const artCards = computed(() => {
  const c = selectedChar.value;
  if (!c) return [] as { entry: CatalogEntry; art: ArtItem | null }[];
  const byEntry = new Map<string, ArtItem>();
  const def = c.art?.default ?? null;
  if (def?.entry) byEntry.set(def.entry, def);
  for (const s of c.art?.skins ?? []) if (s.entry) byEntry.set(s.entry, s);
  const cards = c.entries.map((e) => ({ entry: e, art: byEntry.get(e.id) ?? null }));
  const rank = (x: { entry: CatalogEntry; art: ArtItem | null }) =>
    x.art && x.art === def ? 0 : x.entry.variant === "slg" ? 1 : 2;
  cards.sort((a, b) => rank(a) - rank(b));
  return cards;
});

function label(entry: CatalogEntry) {
  return entry.display || entry.name;
}

function portraitUrl(entry: CatalogEntry) {
  return `portraits/${entry.id}.png`;
}

// ------------------------------------------------------------------------- //
// studio: every mesh + every texture of the selected skin, one 3D scene
// ------------------------------------------------------------------------- //
const meshes = ref<SkinMesh[]>([]);
const geometries = ref<PreviewGeometry[]>([]);
const hidden = ref<string[]>([]);
const textures = ref<TextureEntry[]>([]);
const busy = ref("");
const note = ref("");
const cacheHint = ref("");
const status = ref<{ kind: "info" | "ok" | "err"; text: string } | null>(null);
let token = 0;
/** `c_AlvaSSR01` — the asset-name prefix shared by this skin's meshes and textures. */
let skinKey = "";

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

function stamp(iso: string | null) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

async function openStudio(entry: CatalogEntry) {
  const mine = ++token;
  skinKey = skinKeyOf(entry);
  meshes.value = [];
  geometries.value = [];
  hidden.value = [];
  textures.value = [];
  note.value = "";
  cacheHint.value = "";
  status.value = null;
  if (!isTauri) {
    note.value = t("models.studio.browser");
    return;
  }

  const phase = (p: SkinPhase) =>
    p === "geometry" ? t("models.studio.geometry") : p === "textures" ? t("models.studio.indexing") : t("models.studio.loading");

  busy.value = t("models.studio.loading");
  try {
    const data = await loadSkin(entry, (p) => {
      if (mine === token) busy.value = phase(p);
    });
    if (mine !== token) return;
    meshes.value = data.meshes;
    geometries.value = data.geometry;
    textures.value = data.textures;
    if (data.cached) cacheHint.value = t("models.studio.cached", { time: stamp(data.builtAt) });
    if (!data.meshes.length) note.value = t("models.studio.noMesh");
    else if (!data.textures.length) note.value = t("models.studio.noTex");
  } catch (e) {
    if (mine === token) note.value = String(e);
  } finally {
    if (mine === token) busy.value = "";
  }
}

async function startMaking() {
  const entry = selected.value;
  if (!entry || !meshes.value.length) return;
  busy.value = t("models.making");
  status.value = null;
  try {
    const ready = textures.value.filter((tex) => tex.png);
    const result = await makeWorkspace(entry, label(entry), meshes.value, ready);
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

watch(
  selected,
  (entry) => {
    if (entry) void openStudio(entry);
  },
  { immediate: true },
);
</script>

<template>
  <section class="page">
    <div class="models">
      <aside class="models__list">
        <div class="roster__tools">
          <input
            v-model="query"
            class="input roster__search"
            type="search"
            :placeholder="t('models.search')"
          />
          <span class="chip chip--accent">{{ t("models.chars", { n: filtered.length }) }}</span>
        </div>
        <div class="roster">
          <button
            v-for="c in filtered"
            :key="c.key"
            class="roster__item"
            :class="{ 'roster__item--active': selectedChar?.key === c.key }"
            type="button"
            :title="c.display"
            @click="pickChar(c)"
          >
            <div class="roster__avatar">
              <img
                v-if="c.portraitEntry.portrait"
                class="roster__img"
                :src="portraitUrl(c.portraitEntry)"
                :alt="c.display"
                loading="lazy"
              />
              <span v-else class="roster__ph">{{ c.display.slice(0, 1) }}</span>
            </div>
            <div class="roster__name">{{ c.display }}</div>
            <div class="roster__skin mono">
              {{
                c.art
                  ? t("models.roster", {
                      art: (c.art.default ? 1 : 0) + c.art.skins.length,
                      skins: c.entries.length,
                    })
                  : t("models.rosterNoArt", { skins: c.entries.length })
              }}
            </div>
          </button>
        </div>
      </aside>

      <div class="models__detail">
        <!-- 选中角色的立绘：默认立绘 + 各皮肤立绘 -->
        <section v-if="selectedChar" class="artboard">
          <div class="artboard__head">
            <h3 class="artboard__title">{{ selectedChar.display }}</h3>
            <span class="chip">
              {{ t("models.artboard", { art: artCards.filter((c) => c.art).length, skins: selectedChar.entries.length }) }}
            </span>
          </div>

          <div v-if="artCards.length" class="artrow">
            <button
              v-for="card in artCards"
              :key="card.entry.id"
              class="artrow__item"
              :class="{
                'artrow__item--active': selected?.id === card.entry.id,
                'artrow__item--empty': !card.art,
              }"
              type="button"
              :title="card.art ? card.art.label : t('models.titleNoArt', { name: label(card.entry) })"
              @click="selected = card.entry"
            >
              <img
                v-if="card.art"
                class="artrow__img"
                :src="card.art.file"
                :alt="card.art.label"
                loading="lazy"
              />
              <div v-else class="artrow__placeholder">
                <img
                  v-if="card.entry.portrait"
                  class="artrow__head"
                  :src="portraitUrl(card.entry)"
                  :alt="label(card.entry)"
                  loading="lazy"
                />
                <span v-else class="artrow__initial">{{ label(card.entry).slice(0, 1) }}</span>
                <span class="artrow__nopic">{{ t("models.noArt") }}</span>
              </div>

              <div class="artrow__label">
                <span class="artrow__name">{{ card.art ? card.art.label : label(card.entry) }}</span>
                <span class="artrow__code mono">
                  {{ card.art ? card.art.entry_label || card.art.code : card.entry.variant }}
                </span>
              </div>
            </button>
          </div>
          <p v-else class="artboard__empty">{{ t("models.noArtHint") }}</p>
        </section>

        <!-- 工作台：网格 / 3D 预览 / 贴图 -->
        <div class="studio__actions">
          <button
            class="btn btn--accent"
            type="button"
            :disabled="!!busy || !meshes.length"
            @click="startMaking"
          >
            <span class="glyph">&#xE768;</span>
            {{ t("models.make") }}
          </button>
          <span class="studio__hint">{{ busy || note || cacheHint }}</span>
        </div>
        <p v-if="status" class="status-line" :class="'status-line--' + status.kind">
          {{ status.kind === "ok" ? "✓" : status.kind === "err" ? "✗" : "·" }} {{ status.text }}
        </p>

        <section class="studio">
          <div class="studio__meshes">
            <div class="studio__head">
              <span>{{ t("models.studio.meshes") }}</span>
              <span class="chip">{{ meshes.length }}</span>
            </div>
            <div class="meshline-list">
              <button
                v-for="mesh in meshes"
                :key="mesh.name"
                type="button"
                class="meshline"
                :class="{ 'meshline--off': hidden.includes(mesh.name) }"
                :title="t('models.studio.toggle')"
                @click="toggleMesh(mesh.name)"
              >
                <span class="meshline__eye" aria-hidden="true" />
                <span class="meshline__body">
                  <span class="meshline__name mono">{{ shortName(mesh.name) }}</span>
                  <span class="meshline__meta">
                    {{ t("models.studio.verts", { n: compact(mesh.verts), m: compact(mesh.tris) }) }}
                  </span>
                </span>
              </button>
              <p v-if="!meshes.length && !busy" class="studio__empty">
                {{ t("models.studio.noMesh") }}
              </p>
            </div>
          </div>

          <div class="studio__viewer">
            <MeshViewer :meshes="geometries" :hidden="hidden" :loading="!!busy" />
          </div>

          <div class="studio__textures">
            <div class="studio__head">
              <span>{{ t("models.studio.textures") }}</span>
              <span class="chip">{{ textures.length }}</span>
            </div>
            <div class="texgrid">
              <figure v-for="tex in textures" :key="tex.name" class="texcard" :title="tex.name">
                <div class="texcard__frame">
                  <img
                    v-if="tex.png"
                    class="texcard__img"
                    :src="`data:image/png;base64,${tex.png}`"
                    :alt="tex.name"
                  />
                  <div v-else class="texcard__ph">?</div>
                </div>
                <figcaption class="texcard__caption mono">{{ shortName(tex.name) }}</figcaption>
              </figure>
              <p v-if="!textures.length && !busy" class="studio__empty">
                {{ t("models.studio.noTex") }}
              </p>
            </div>
          </div>
        </section>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* ---------- 卡牌视图 + 详情 双栏 ---------- */
.models {
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  gap: 14px;
  align-items: start;
  min-width: 0;
}

.models__list {
  max-height: calc(100vh - 120px);
  overflow-y: auto;
  padding-right: 6px;
  border-right: 1px solid var(--stroke, rgba(255, 255, 255, 0.06));
}

.roster__tools {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.roster__search {
  flex: 1;
  min-width: 0;
}

.roster__tools .chip {
  flex: none;
  white-space: nowrap;
}

.models__detail {
  min-width: 0;
  overflow-x: hidden;
}

.roster {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(78px, 1fr));
  align-items: start;
  gap: 8px;
}

.roster__item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 5px;
  padding: 6px 4px 8px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: transparent;
  cursor: pointer;
  transition: 120ms ease;
}

.roster__item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.roster__item--active {
  background: rgba(76, 194, 255, 0.12);
  border-color: rgba(76, 194, 255, 0.55);
}

.roster__avatar {
  width: 52px;
  height: 52px;
  border-radius: 14px;
  overflow: hidden;
  background: linear-gradient(160deg, #24334a, #101724);
  display: flex;
  align-items: center;
  justify-content: center;
}

.roster__img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  object-position: 50% 12%;
}

.roster__ph {
  font-size: 20px;
  font-weight: 600;
  opacity: 0.75;
}

.roster__name {
  font-size: 11px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.roster__skin {
  font-size: 9px;
  opacity: 0.5;
}

/* ---------- 立绘卡片 ---------- */
.artboard {
  margin-top: 2px;
}

.artboard__head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  margin-bottom: 8px;
}

.artboard__title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.artboard__empty {
  margin: 6px 0 0;
  font-size: 12px;
  opacity: 0.6;
}

.artrow {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(74px, 1fr));
  justify-content: start;
  gap: 8px;
  max-width: 100%;
}

.artrow__item {
  position: relative;
  aspect-ratio: 505 / 1000;
  max-height: 160px;
  padding: 0;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
  border-radius: 12px;
  overflow: hidden;
  background: radial-gradient(120% 70% at 50% 20%, #2a3a55 0%, #131b28 70%, #0d131d 100%);
  cursor: pointer;
  transition: 120ms ease;
}

.artrow__item:hover {
  transform: translateY(-2px);
  border-color: rgba(255, 255, 255, 0.24);
}

.artrow__item--active {
  border-color: var(--accent, #4cc2ff);
  box-shadow: 0 0 0 1px var(--accent, #4cc2ff) inset, 0 8px 22px rgba(0, 0, 0, 0.4);
}

.artrow__img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  object-position: 50% 100%;
}

/* 没有立绘的皮肤：同一张卡片位，显示头像 + 「无立绘」标记 */
.artrow__item--empty {
  background: repeating-linear-gradient(
      135deg,
      rgba(255, 255, 255, 0.022) 0 10px,
      transparent 10px 20px
    ),
    radial-gradient(120% 70% at 50% 25%, #202b3d 0%, #121a26 70%, #0d131d 100%);
}

.artrow__placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding-bottom: 30px;
}

.artrow__head {
  width: 54%;
  aspect-ratio: 1;
  border-radius: 50%;
  object-fit: cover;
  opacity: 0.55;
  filter: grayscale(0.35);
}

.artrow__initial {
  font-size: 30px;
  font-weight: 600;
  opacity: 0.4;
}

.artrow__nopic {
  font-size: 10px;
  letter-spacing: 0.08em;
  color: rgba(206, 224, 255, 0.45);
  border: 1px solid rgba(206, 224, 255, 0.18);
  border-radius: 999px;
  padding: 1px 8px;
}

.artrow__label {
  position: absolute;
  inset: auto 0 0 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 14px 7px 6px;
  background: linear-gradient(to top, rgba(6, 10, 18, 0.94), rgba(6, 10, 18, 0.6) 55%, transparent);
}

.artrow__name {
  font-size: 10.5px;
  font-weight: 600;
  color: #eef4ff;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.artrow__code {
  font-size: 8px;
  color: rgba(214, 230, 255, 0.6);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---------- 工作台 ---------- */
.studio {
  display: grid;
  grid-template-columns: 196px minmax(0, 1fr);
  grid-template-rows: 240px auto;
  gap: 12px;
  margin-top: 10px;
  min-width: 0;
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
  grid-row: 1;
}

.studio__textures {
  grid-column: 1 / -1;
}

.meshline-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 100%;
  overflow-y: auto;
  padding-right: 4px;
}

.meshline {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 8px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: rgba(255, 255, 255, 0.03);
  text-align: left;
  cursor: pointer;
  transition: 120ms ease;
}

.meshline:hover {
  background: rgba(255, 255, 255, 0.07);
}

.meshline__body {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

/* 显示/隐藏指示：实心点 = 显示中 */
.meshline__eye {
  flex: none;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--accent, #4cc2ff);
  box-shadow: 0 0 6px rgba(76, 194, 255, 0.45);
}

.meshline--off {
  opacity: 0.45;
}

.meshline--off .meshline__eye {
  background: transparent;
  border: 1px solid var(--text-3, #7d8fa6);
  box-shadow: none;
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

.studio__viewer {
  height: 240px;
  min-width: 0;
}

.texgrid {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  overflow-y: hidden;
  padding: 2px 4px 8px 2px;
  align-items: flex-start;
}

.texcard {
  flex: 0 0 88px;
  margin: 0;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
  border-radius: 8px;
  overflow: hidden;
  background:
    repeating-conic-gradient(rgba(255, 255, 255, 0.05) 0% 25%, transparent 0% 50%) 0 0 / 12px 12px,
    rgba(255, 255, 255, 0.02);
}

/* 游戏贴图都是正方形：用 padding-top 撑成 1:1，不依赖 aspect-ratio */
.texcard__frame {
  position: relative;
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

.texcard__caption {
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

.studio__actions {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 12px;
}

.studio__hint {
  font-size: 11.5px;
  color: var(--text-2);
  opacity: 0.8;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-line {
  margin: 10px 0 0;
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
  .studio {
    grid-template-columns: 150px minmax(0, 1fr);
    grid-template-rows: 220px auto;
  }

  .studio__viewer {
    height: 220px;
  }
}
</style>
