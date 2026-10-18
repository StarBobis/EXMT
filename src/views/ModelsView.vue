<script setup lang="ts">
import { ref } from "vue";
import { copyFile, exists, mkdir, writeTextFile } from "@tauri-apps/plugin-fs";
import { open, save } from "@tauri-apps/plugin-dialog";
import { appDataDir, join } from "@tauri-apps/api/path";
import {
  bundlePath,
  PRESETS,
  runCore,
  type CharacterPreset,
} from "../lib/core";

const busy = ref("");
const log = ref<{ kind: "info" | "ok" | "err"; text: string }[]>([]);

function push(kind: "info" | "ok" | "err", text: string) {
  log.value.unshift({ kind, text });
  if (log.value.length > 40) log.value.pop();
}

async function backupPath(hash: string): Promise<string> {
  const base = await appDataDir();
  const dir = await join(base, "backups");
  if (!(await exists(dir))) await mkdir(dir, { recursive: true });
  return join(dir, `${hash}.bundle`);
}

async function ensureBackup(hash: string) {
  const bak = await backupPath(hash);
  if (!(await exists(bak))) {
    await copyFile(bundlePath(hash), bak);
    push("info", `已备份原包 → ${bak}`);
  }
  return bak;
}

async function extractModel(preset: CharacterPreset) {
  busy.value = preset.id;
  try {
    const out = await save({
      title: "导出模型（.glb 含骨架 / .exmtmesh 可回填）",
      defaultPath: `${preset.mesh}.glb`,
      filters: [
        { name: "glTF 二进制（含骨架）", extensions: ["glb"] },
        { name: "EXMT Mesh（可打包回游戏）", extensions: ["exmtmesh"] },
      ],
    });
    if (!out) return;
    if (out.toLowerCase().endsWith(".glb")) {
      push("info", `导出 glTF（全部网格 + 骨架）…`);
      const res = await runCore(["glb", bundlePath(preset.bundle), out]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      const info = JSON.parse(res.stdout.trim().split("\n").pop() || "{}");
      push("ok", `glTF 已导出：${info.meshes} 个网格 / ${info.skins} 套骨架 → ${out}`);
    } else {
      push("info", `提取 ${preset.mesh} …`);
      const res = await runCore(["extract", bundlePath(preset.bundle), preset.mesh, out]);
      if (res.code !== 0) throw new Error(res.stderr || res.stdout);
      push("ok", res.stdout.trim() || `已导出 → ${out}`);
    }
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function installModel(preset: CharacterPreset) {
  busy.value = preset.id;
  try {
    const file = await open({
      title: "选择要替换的 .exmtmesh",
      multiple: false,
      filters: [{ name: "EXMT Mesh", extensions: ["exmtmesh"] }],
    });
    if (!file) return;
    const bak = await ensureBackup(preset.bundle);
    const tmp = bak.replace(/\.bundle$/, ".mod.bundle");
    push("info", `打包 ${preset.mesh} …`);
    const res = await runCore([
      "pack",
      bundlePath(preset.bundle),
      preset.mesh,
      file as string,
      tmp,
    ]);
    if (res.code !== 0) throw new Error(res.stderr || res.stdout);
    await copyFile(tmp, bundlePath(preset.bundle));

    // record the installed mod (a minimal "Mod 包" manifest)
    const base = await appDataDir();
    const modDir = await join(base, "mods");
    if (!(await exists(modDir))) await mkdir(modDir, { recursive: true });
    const manifest = {
      preset: preset.id,
      character: preset.name,
      mesh: preset.mesh,
      bundle: preset.bundle,
      source: file,
      installed_at: new Date().toISOString(),
    };
    await writeTextFile(
      await join(modDir, `${preset.id}.json`),
      JSON.stringify(manifest, null, 2),
    );
    push("ok", `已替换并安装：${preset.mesh}（已写入 Mod 清单）`);
  } catch (e) {
    push("err", String(e));
  } finally {
    busy.value = "";
  }
}

async function restore(preset: CharacterPreset) {
  busy.value = preset.id;
  try {
    const bak = await backupPath(preset.bundle);
    if (!(await exists(bak))) {
      push("err", "没有找到备份，无法还原");
      return;
    }
    await copyFile(bak, bundlePath(preset.bundle));
    push("ok", "已还原原始 bundle");
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
        <p class="page-subtitle">角色预设 · 导出到 Blender · 打包回游戏</p>
      </div>
      <div class="page-actions">
        <span class="chip chip--accent">
          <span class="glyph">&#xE7B8;</span>
          {{ PRESETS.length }} 个预设
        </span>
      </div>
    </header>

    <article v-for="preset in PRESETS" :key="preset.id" class="card">
      <div class="model-head">
        <div class="feature__icon">
          <span class="glyph">&#xE7B8;</span>
        </div>
        <div style="flex: 1 1 auto; min-width: 0">
          <h3 class="card__title">{{ preset.name }}</h3>
          <p class="card__desc">{{ preset.note }}</p>
        </div>
        <span class="chip mono">{{ preset.mesh }}</span>
      </div>

      <div class="kv" style="margin: 16px 0 4px">
        <div class="kv__k">模型 bundle</div>
        <div class="kv__v mono">{{ preset.bundle }}</div>
        <div class="kv__k">贴图 bundle</div>
        <div class="kv__v mono">{{ preset.textureBundle }}</div>
        <div class="kv__k">可替换贴图</div>
        <div class="kv__v mono">{{ preset.textures.join(" · ") }}</div>
      </div>

      <div class="model-actions">
        <button
          class="btn"
          type="button"
          :disabled="busy === preset.id"
          @click="extractModel(preset)"
        >
          <span class="glyph">&#xE896;</span>
          导出模型
        </button>
        <button
          class="btn btn--accent"
          type="button"
          :disabled="busy === preset.id"
          @click="installModel(preset)"
        >
          <span class="glyph">&#xE74E;</span>
          导入并安装
        </button>
        <button class="btn" type="button" :disabled="busy === preset.id" @click="restore(preset)">
          <span class="glyph">&#xE777;</span>
          还原原版
        </button>
        <span v-if="busy === preset.id" class="status">
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
            导出 → 在 Blender 里用 EXMT 插件导入 → 修改 → 一键导出打包
          </div>
          <div v-for="(line, i) in log" :key="i" :class="{
            'terminal__line--dim': line.kind === 'info',
          }">
            {{ line.kind === "ok" ? "✓" : line.kind === "err" ? "✗" : "·" }} {{ line.text }}
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.model-head {
  display: flex;
  align-items: center;
  gap: 14px;
}

.model-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 18px;
}
</style>
