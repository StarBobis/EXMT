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

function initials(entry: CatalogEntry) {
  return label(entry).slice(0, 1) || entry.id.slice(2, 3);
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
        :class="{ 'roster__item--active': selected?.id === entry.id }"
        type="button"
        @click="selected = entry"
      >
        <div class="roster__avatar">
          <img
            v-if="entry.portrait"
            class="roster__img"
            :src="portraitUrl(entry)"
            :alt="label(entry)"
            loading="lazy"
          />
          <span v-else class="roster__ph">{{ initials(entry) }}</span>
        </div>
        <div class="roster__name">{{ label(entry) }}</div>
        <div class="roster__badge">{{ entry.variant }}</div>
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
          @click="exportModel(selected, 'glb')"
        >
          <span class="glyph">&#xE896;</span>
          导出 glTF（全部网格 + 骨架）
        </button>
        <button
          class="btn"
          type="button"
          :disabled="busy === selected.id"
          @click="exportModel(selected, 'exmtmesh')"
        >
          <span class="glyph">&#xE74E;</span>
          导出 .exmtmesh
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
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 10px;
}

.roster__item {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 10px 6px 8px;
  border-radius: 10px;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.07));
  background: var(--card-subtle, rgba(255, 255, 255, 0.03));
  cursor: pointer;
  transition: 120ms ease;
}

.roster__item:hover {
  background: rgba(255, 255, 255, 0.07);
}

.roster__item--active {
  border-color: var(--accent, #4cc2ff);
  box-shadow: 0 0 0 1px var(--accent, #4cc2ff) inset;
}

.roster__avatar {
  width: 64px;
  height: 64px;
  border-radius: 50%;
  overflow: hidden;
  background: linear-gradient(135deg, #2d3f5a, #1b2536);
  display: flex;
  align-items: center;
  justify-content: center;
}

.roster__img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.roster__ph {
  font-size: 24px;
  font-weight: 600;
}

.roster__name {
  font-size: 12px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.roster__badge {
  font-size: 10px;
  opacity: 0.5;
  text-transform: uppercase;
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
