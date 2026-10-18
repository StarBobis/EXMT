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

interface CatalogEntry {
  id: string;
  name: string;
  display?: string;
  variant: string;
  bundle: string;
  parts: Record<string, string>;
  part_bundles?: Record<string, string[]>;
  portrait?: { bundle: string; texture: string } | null;
  splash?: string;
  splash_source?: string;
  splash_kind?: string;
  splash_size?: number[];
}

const catalog = ref<CatalogEntry[]>(catalogData.characters as CatalogEntry[]);
const query = ref("");
const selected = ref<CatalogEntry | null>(catalog.value[0] ?? null);
const busy = ref("");
const log = ref<{ kind: "info" | "ok" | "err"; text: string }[]>([]);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return catalog.value;
  return catalog.value.filter((c) => {
    const d = (c.display ?? "").toLowerCase();
    return c.id.toLowerCase().includes(q) || c.name.toLowerCase().includes(q) || d.includes(q);
  });
});

const parts = computed(() => Object.entries(selected.value?.parts ?? {}));

function label(entry: CatalogEntry) {
  return entry.display || entry.name;
}

function push(kind: "info" | "ok" | "err", text: string) {
  log.value.unshift({ kind, text });
  if (log.value.length > 40) log.value.pop();
}

function portraitUrl(entry: CatalogEntry) {
  return `portraits/${entry.id}.png`;
}

/** Skin label derived from the internal skin code (the game keeps localized names elsewhere). */
function skinName(entry: CatalogEntry) {
  const code = entry.id.includes("_") ? entry.id.split("_")[1] : entry.id;
  let suffix = entry.name && code.startsWith(entry.name) ? code.slice(entry.name.length) : code;
  if (!suffix) return "默认";
  if (/^dorm/i.test(suffix)) return "宿舍装";
  suffix = suffix.replace(/^SSR0?/i, "SSR ").replace(/^SR0?/i, "SR ");
  return suffix;
}

function initials(entry: CatalogEntry) {
  return label(entry).slice(0, 1) || entry.id.slice(2, 3);
}

async function exportAllParts(entry: CatalogEntry) {
  busy.value = entry.id;
  try {
    const dir = await open({ title: "选择导出目录（每个部位一个 .exmtmesh）", directory: true });
    if (!dir) return;
    push("info", `导出全部部位（.exmtmesh，含完整类型树）…`);
    const res = await runCore(["extract-all", bundlePath(entry.bundle), dir as string]);
    if (res.code !== 0) throw new Error(res.stderr || res.stdout);
    const lines = res.stdout.trim().split("\n").filter(Boolean).length;
    push("ok", `${label(entry)}：导出 ${lines} 个部位 → ${dir}`);
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function exportGlb(entry: CatalogEntry) {
  busy.value = entry.id;
  try {
    const out = await save({
      title: "导出 glTF（仅用于预览）",
      defaultPath: `${entry.id}.glb`,
      filters: [{ name: "glTF 二进制", extensions: ["glb"] }],
    });
    if (!out) return;
    const res = await runCore(["glb", bundlePath(entry.bundle), out]);
    if (res.code !== 0) throw new Error(res.stderr || res.stdout);
    const info = JSON.parse(res.stdout.trim().split("\n").pop() || "{}");
    push("ok", `glTF（预览用，不含骨骼哈希/碰撞等数据）：${info.meshes} 个网格 → ${out}`);
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function exportModel(entry: CatalogEntry, format: "glb" | "exmtmesh") {
  busy.value = entry.id;
  try {
    const part = entry.parts.cloth || entry.parts.body || Object.values(entry.parts)[0];
    const out = await save({
      title: format === "glb" ? "导出 glTF（含骨架）" : "导出 .exmtmesh",
      defaultPath: format === "glb" ? `${entry.id}.glb` : `${part}.exmtmesh`,
      filters:
        format === "glb"
          ? [{ name: "glTF 二进制", extensions: ["glb"] }]
          : [{ name: "EXMT Mesh", extensions: ["exmtmesh"] }],
    });
    if (!out) return;
    if (format === "glb") {
      const res = await runCore(["glb", bundlePath(entry.bundle), out]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      const info = JSON.parse(res.stdout.trim().split("\n").pop() || "{}");
      push("ok", `${label(entry)}：${info.meshes} 个网格 / ${info.skins} 套骨架 → ${out}`);
    } else {
      const res = await runCore(["extract", bundlePath(entry.bundle), part, out]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      push("ok", res.stdout.trim() || `已导出 → ${out}`);
    }
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
      title: `选择要替换 ${target} 的 .exmtmesh`,
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
      push("info", `打包 ${target} → ${hash.slice(0, 8)}…`);
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
    push("ok", `已安装：${label(entry)} · ${target}（${bundles.length} 个包，原文件已备份）`);
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function applyBuiltin(entry: CatalogEntry, part: string, mode: "basebody" | "hide") {
  busy.value = entry.id;
  try {
    const bundles = entry.part_bundles?.[part]?.length
      ? entry.part_bundles[part]
      : [entry.bundle];
    const mesh = entry.parts[part] ?? part;
    for (const hash of bundles) {
      push("info", `${mode === "hide" ? "隐藏" : "素体"} ${part} → ${hash.slice(0, 8)}…`);
      await ensureBackup(hash);
      const tmp = await join(await cacheDir(), `${entry.id}-${part}-${mode}-${hash.slice(0, 8)}.bundle`);
      const res = await runCore([
        mode === "hide" ? "hide-mesh" : "base-body",
        bundlePath(hash),
        mesh,
        tmp,
      ]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      await installBundle(hash, tmp);
    }
    await writeMod({
      id: `${entry.id}-${part}-${mode}`,
      preset: entry.id,
      character: label(entry),
      mesh,
      bundle: bundles.join(","),
      installed_at: new Date().toISOString(),
      kind: mode === "hide" ? "hidden" : "basebody",
    });
    push("ok", `${label(entry)} · ${part}：${mode === "hide" ? "已隐藏" : "已换成素体"}`);
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
      title: "选择要安装的 .bundle",
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
    push("ok", `已安装 bundle：${await basename(file as string)}`);
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
    push("ok", `${label(entry)}：已还原原始文件`);
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
        <h1 class="page-title">模型</h1>
        <p class="page-subtitle">选角色 · 导出改 · 装回来</p>
      </div>
      <div class="page-actions">
        <input v-model="query" class="input" type="search" placeholder="搜索角色…" />
        <span class="chip chip--accent">{{ catalog.length }} 个角色</span>
      </div>
    </header>

    <div class="roster">
      <button
        v-for="entry in filtered"
        :key="entry.id"
        class="roster__item"
        :class="{
          'roster__item--active': selected?.id === entry.id,
          'roster__item--plain': entry.splash_kind !== 'skinpic',
        }"
        type="button"
        @click="selected = entry"
      >
        <div class="roster__avatar">
          <img
            v-if="entry.splash"
            class="roster__img"
            :class="entry.splash_kind === 'skinpic' ? 'roster__img--splash' : 'roster__img--crop'"
            :src="entry.splash"
            :alt="label(entry)"
            loading="lazy"
          />
          <img
            v-else-if="entry.portrait"
            class="roster__img roster__img--crop"
            :src="portraitUrl(entry)"
            :alt="label(entry)"
            loading="lazy"
          />
          <span v-else class="roster__ph">{{ initials(entry) }}</span>
          <div class="roster__badge">{{ entry.variant }}</div>
        </div>
        <div class="roster__name">{{ label(entry) }}</div>
        <div class="roster__skin mono">{{ skinName(entry) }}</div>
      </button>
    </div>

    <article v-if="selected" class="card" style="margin-top: 16px">
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
        <div class="kv__k">可导出部位</div>
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
              替换…
            </button>
            <button
              class="btn"
              type="button"
              title="用内置素体替换这个部位"
              :disabled="busy === selected.id"
              @click="applyBuiltin(selected, key, 'basebody')"
            >
              素体
            </button>
            <button
              class="btn"
              type="button"
              title="隐藏这个部位"
              :disabled="busy === selected.id"
              @click="applyBuiltin(selected, key, 'hide')"
            >
              隐藏
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
          导出全部部位（.exmtmesh）
        </button>
        <button
          class="btn"
          type="button"
          :disabled="busy === selected.id"
          @click="exportGlb(selected)"
        >
          <span class="glyph">&#xE74E;</span>
          导出 glTF（仅预览）
        </button>
        <button
          class="btn"
          type="button"
          :disabled="busy === selected.id"
          @click="exportModel(selected, 'exmtmesh')"
        >
          导出单个部位
        </button>
        <button
          class="btn"
          type="button"
          :disabled="busy === selected.id"
          @click="importBundle(selected)"
        >
          安装 .bundle
        </button>
        <button class="btn" type="button" :disabled="busy === selected.id" @click="restore(selected)">
          <span class="glyph">&#xE777;</span>
          还原原版
        </button>
        <span v-if="busy === selected.id" class="status">
          <span class="status__dot status__dot--warn" />处理中…
        </span>
      </div>
    </article>

    <div class="card">
      <h3 class="card__title">操作日志</h3>
      <div class="terminal" style="margin-top: 12px">
        <div class="terminal__bar"><span class="terminal__dot" /><span>pipeline</span></div>
        <div class="terminal__body" style="min-height: 140px">
          <div v-if="log.length === 0" class="terminal__line--dim">
            选角色 → 导出 glTF → Blender 修改 → 导出 .exmtmesh → 这里「替换…」装回来
          </div>
          <div
            v-for="(line, i) in log"
            :key="i"
            :class="{ 'terminal__line--dim': line.kind === 'info' }"
          >
            {{ line.kind === "ok" ? "✓" : line.kind === "err" ? "✗" : "·" }} {{ line.text }}
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.roster {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  align-items: start;
  gap: 12px;
}

.roster__item {
  position: relative;
  display: block;
  padding: 0;
  aspect-ratio: 505 / 1000;
  border-radius: 12px;
  overflow: hidden;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.07));
  background: linear-gradient(170deg, #223049 0%, #16202f 55%, #0d131d 100%);
  cursor: pointer;
  transition: 120ms ease;
}

.roster__item:hover {
  transform: translateY(-2px);
  border-color: rgba(255, 255, 255, 0.22);
}

/* 所有卡片尺寸统一：没有立绘的皮肤用同一张卡片，图按比例居中显示 */
.roster__item--plain .roster__avatar {
  background: radial-gradient(120% 80% at 50% 30%, #2a3a55 0%, #131b28 70%, #0d131d 100%);
}

.roster__item--active {
  border-color: var(--accent, #4cc2ff);
  box-shadow: 0 0 0 1px var(--accent, #4cc2ff) inset, 0 8px 24px rgba(0, 0, 0, 0.35);
}

.roster__avatar {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(160deg, #24334a, #101724);
}

.roster__img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

/* 皮肤立绘：整张竖图完整显示，人物站在卡片底部 */
.roster__img--splash {
  object-fit: contain;
  object-position: 50% 100%;
}

/* 头像 / 半身图（没有立绘的皮肤）：整张图按比例居中，卡片尺寸和立绘卡一致 */
.roster__img--crop {
  object-fit: contain;
  object-position: 50% 32%;
}

.roster__ph {
  font-size: 40px;
  font-weight: 600;
  opacity: 0.7;
}

.roster__name {
  position: absolute;
  left: 10px;
  right: 10px;
  bottom: 26px;
  font-size: 13px;
  font-weight: 600;
  color: #f2f6ff;
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.roster__skin {
  position: absolute;
  left: 10px;
  right: 10px;
  bottom: 9px;
  font-size: 10px;
  color: rgba(224, 236, 255, 0.72);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* bottom scrim so the overlaid labels always read */
.roster__item::after {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 42%;
  background: linear-gradient(to top, rgba(6, 10, 18, 0.92), rgba(6, 10, 18, 0.55) 45%, transparent);
  pointer-events: none;
}

.roster__badge {
  position: absolute;
  right: 8px;
  top: 8px;
  z-index: 2;
  padding: 1px 7px;
  border-radius: 999px;
  font-size: 9px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: #dbe7ff;
  background: rgba(8, 14, 24, 0.66);
  backdrop-filter: blur(4px);
}

.parts {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
}

.parts__row {
  display: grid;
  grid-template-columns: 70px 1fr auto;
  align-items: center;
  gap: 12px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.03);
}

.parts__actions {
  display: flex;
  gap: 6px;
}

.parts__key {
  font-size: 12px;
  opacity: 0.7;
}

.parts__mesh {
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
