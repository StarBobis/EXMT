<script setup lang="ts">
import { computed, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { basename, join } from "@tauri-apps/api/path";
import { runCore } from "../lib/core";
import {
  bundlePath,
  cacheDir,
  ensureBackup,
  installBundle,
  restoreBackup,
  writeMod,
} from "../lib/modbox";
import catalogData from "../data/catalog.json";
import artData from "../data/art.json";
import { t } from "../lib/i18n";

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
const busy = ref("");
const status = ref<{ kind: "info" | "ok" | "err"; text: string } | null>(null);
let statusTimer: number | undefined;

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

const parts = computed(() => Object.entries(selected.value?.parts ?? {}));

function label(entry: CatalogEntry) {
  return entry.display || entry.name;
}

function push(kind: "info" | "ok" | "err", text: string) {
  status.value = { kind, text };
  window.clearTimeout(statusTimer);
  if (kind !== "info") statusTimer = window.setTimeout(() => (status.value = null), 6000);
}

function portraitUrl(entry: CatalogEntry) {
  return `portraits/${entry.id}.png`;
}

async function exportAllParts(entry: CatalogEntry) {
  busy.value = entry.id;
  try {
    const dir = await open({ title: t("models.dialog.exportAll"), directory: true });
    if (!dir) return;
    push("info", t("models.status.exportAll"));
    const res = await runCore(["extract-all", bundlePath(entry.bundle), dir as string]);
    if (res.code !== 0) throw new Error(res.stderr || res.stdout);
    const lines = res.stdout.trim().split("\n").filter(Boolean).length;
    push("ok", t("models.status.exportedAll", { name: label(entry), n: lines, dir: String(dir) }));
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function exportPart(entry: CatalogEntry) {
  busy.value = entry.id;
  try {
    const part = entry.parts.cloth || entry.parts.body || Object.values(entry.parts)[0];
    const out = await save({
      title: t("models.dialog.exportOne"),
      defaultPath: `${part}.exmtmesh`,
      filters: [{ name: "EXMT Mesh", extensions: ["exmtmesh"] }],
    });
    if (!out) return;
    const res = await runCore(["extract", bundlePath(entry.bundle), part, out]);
    if (res.code !== 0) throw new Error(res.stderr || res.stdout);
    push("ok", res.stdout.trim() || t("models.status.exportedOne", { path: String(out) }));
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function installMod(entry: CatalogEntry, target: string) {
  busy.value = entry.id;
  try {
    const file = await open({
      title: t("models.dialog.install", { target }),
      multiple: false,
      filters: [{ name: "EXMT Mesh", extensions: ["exmtmesh"] }],
    });
    if (!file) return;
    // the same mesh can live in several bundles; patch every copy so the game
    // picks up the change whichever one it loads
    const bundles = entry.part_bundles?.[target]?.length
      ? entry.part_bundles[target]
      : [entry.bundle];
    const mesh = entry.parts[target] ?? target;
    for (const hash of bundles) {
      push("info", t("models.status.packing", { target, hash: hash.slice(0, 8) }));
      await ensureBackup(hash);
      const tmp = await join(await cacheDir(), `${entry.id}-${target}-${hash.slice(0, 8)}.bundle`);
      const res = await runCore(["pack", bundlePath(hash), mesh, file as string, tmp]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      await installBundle(hash, tmp);
    }
    await writeMod({
      id: `${entry.id}-${target}`,
      preset: entry.id,
      character: label(entry),
      mesh,
      bundle: bundles.join(","),
      source: file as string,
      installed_at: new Date().toISOString(),
      kind: "mesh",
    });
    push("ok", t("models.status.installed", { name: label(entry), target, n: bundles.length }));
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function applyHide(entry: CatalogEntry, part: string) {
  busy.value = entry.id;
  try {
    const bundles = entry.part_bundles?.[part]?.length
      ? entry.part_bundles[part]
      : [entry.bundle];
    const mesh = entry.parts[part] ?? part;
    for (const hash of bundles) {
      push("info", t("models.status.hiding", { part, hash: hash.slice(0, 8) }));
      await ensureBackup(hash);
      const tmp = await join(await cacheDir(), `${entry.id}-${part}-hidden-${hash.slice(0, 8)}.bundle`);
      const res = await runCore(["hide-mesh", bundlePath(hash), mesh, tmp]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      await installBundle(hash, tmp);
    }
    await writeMod({
      id: `${entry.id}-${part}-hidden`,
      preset: entry.id,
      character: label(entry),
      mesh,
      bundle: bundles.join(","),
      installed_at: new Date().toISOString(),
      kind: "hidden",
    });
    push("ok", t("models.status.hidden", { name: label(entry), part }));
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function importBundle(entry: CatalogEntry) {
  busy.value = entry.id;
  try {
    const file = await open({
      title: t("models.dialog.bundle"),
      multiple: false,
      filters: [{ name: "UnityFS Bundle", extensions: ["bundle"] }],
    });
    if (!file) return;
    await ensureBackup(entry.bundle);
    await installBundle(entry.bundle, file as string);
    await writeMod({
      id: `${entry.id}-bundle`,
      preset: entry.id,
      character: label(entry),
      bundle: entry.bundle,
      source: file as string,
      installed_at: new Date().toISOString(),
      kind: "bundle",
    });
    push("ok", t("models.status.bundleInstalled", { file: await basename(file as string) }));
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function restore(entry: CatalogEntry) {
  busy.value = entry.id;
  try {
    await restoreBackup(entry.bundle);
    push("ok", t("models.status.restored", { name: label(entry) }));
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">{{ t("models.title") }}</h1>
        <p class="page-subtitle">{{ t("models.subtitle") }}</p>
      </div>
      <div class="page-actions">
        <input v-model="query" class="input" type="search" :placeholder="t('models.search')" />
        <span class="chip chip--accent">{{ t("models.chars", { n: characters.length }) }}</span>
      </div>
    </header>

    <!-- 卡牌视图：圆角方块选角色 -->
    <div class="models">
      <!-- 卡牌视图：圆角方块选角色 -->
      <aside class="models__list">
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

    <article v-if="selected" class="card" style="margin-top: 14px">
      <div class="model-head">
        <div class="feature__icon">
          <span class="glyph">&#xE8F1;</span>
        </div>
        <div style="flex: 1 1 auto; min-width: 0">
          <h3 class="card__title">{{ label(selected) }}</h3>
          <p class="card__desc">{{ selected.id }}</p>
        </div>
        <span class="chip mono">{{ selected.bundle.slice(0, 12) }}…</span>
      </div>

      <div class="kv" style="margin: 16px 0 4px">
        <div class="kv__k">bundle</div>
        <div class="kv__v mono">{{ selected.bundle }}</div>
        <div class="kv__k">{{ t("models.parts") }}</div>
        <div class="kv__v mono">{{ parts.map(([k]) => k).join(" · ") }}</div>
      </div>

      <div class="parts">
        <div v-for="[key, mesh] in parts" :key="key" class="parts__row">
          <span class="parts__key">{{ key }}</span>
          <span class="parts__mesh mono">{{ mesh }}</span>
          <div class="parts__actions">
            <button
              class="btn"
              type="button"
              :disabled="busy === selected.id"
              @click="installMod(selected, key)"
            >
              {{ t("models.replace") }}
            </button>
            <button
              class="btn"
              type="button"
              :title="t('models.hideTitle')"
              :disabled="busy === selected.id"
              @click="applyHide(selected, key)"
            >
              {{ t("models.hide") }}
            </button>
          </div>
        </div>
      </div>
      <div class="model-actions">
        <button
          class="btn btn--accent"
          type="button"
          :disabled="busy === selected.id"
          @click="exportAllParts(selected)"
        >
          <span class="glyph">&#xE896;</span>
          {{ t("models.exportAll") }}
        </button>
        <button
          class="btn"
          type="button"
          :disabled="busy === selected.id"
          @click="exportPart(selected)"
        >
          {{ t("models.exportOne") }}
        </button>
        <button
          class="btn"
          type="button"
          :disabled="busy === selected.id"
          @click="importBundle(selected)"
        >
          {{ t("models.installBundle") }}
        </button>
        <button class="btn" type="button" :disabled="busy === selected.id" @click="restore(selected)">
          <span class="glyph">&#xE777;</span>
          {{ t("models.restore") }}
        </button>
        <span v-if="busy === selected.id" class="status">
          <span class="status__dot status__dot--warn" />{{ t("models.working") }}
        </span>
      </div>
      <p v-if="status" class="status-line" :class="'status-line--' + status.kind">
        {{ status.kind === "ok" ? "✓" : status.kind === "err" ? "✗" : "·" }} {{ status.text }}
      </p>
    </article>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* ---------- 卡牌视图 + 详情 双栏 ---------- */
.models {
  display: grid;
  grid-template-columns: 300px minmax(0, 1fr);
  gap: 14px;
  align-items: start;
  min-width: 0;
}

.models__list {
  max-height: calc(100vh - 190px);
  overflow-y: auto;
  padding-right: 6px;
  border-right: 1px solid var(--stroke, rgba(255, 255, 255, 0.06));
}

.models__detail {
  min-width: 0;
  overflow-x: hidden;
}

.roster {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(88px, 1fr));
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
  width: 56px;
  height: 56px;
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
  font-size: 22px;
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
  margin-top: 12px;
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
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  justify-content: start;
  gap: 8px;
}

.artrow__item {
  position: relative;
  aspect-ratio: 505 / 1000;
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
  padding-bottom: 34px;
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
  font-size: 34px;
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
  padding: 16px 8px 7px;
  background: linear-gradient(to top, rgba(6, 10, 18, 0.94), rgba(6, 10, 18, 0.6) 55%, transparent);
}

.artrow__name {
  font-size: 11px;
  font-weight: 600;
  color: #eef4ff;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.artrow__code {
  font-size: 8.5px;
  color: rgba(214, 230, 255, 0.6);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.skinchips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-top: 12px;
}

.skinchips__hint {
  font-size: 11px;
  opacity: 0.55;
  margin-right: 2px;
}

.skinchips__item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 9px;
  border-radius: 999px;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.1));
  background: rgba(255, 255, 255, 0.04);
  font-size: 11px;
  color: inherit;
  cursor: pointer;
}

.skinchips__item--active {
  border-color: var(--accent, #4cc2ff);
  background: rgba(76, 194, 255, 0.14);
}

.skinchips__item .mono {
  font-size: 9px;
  opacity: 0.55;
}

.status-line {
  margin: 12px 0 0;
  font-size: 12px;
  line-height: 18px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
}

.status-line--ok {
  color: #8ee6a8;
}

.status-line--err {
  color: #ff9b9b;
}

.parts {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
}

.parts__row {
  display: grid;
  grid-template-columns: 62px minmax(0, 1fr);
  grid-template-areas: "key mesh" "actions actions";
  align-items: center;
  gap: 4px 12px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.03);
}

.parts__actions {
  grid-area: actions;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.parts__key {
  grid-area: key;
  font-size: 12px;
  opacity: 0.7;
}

.parts__mesh {
  grid-area: mesh;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
