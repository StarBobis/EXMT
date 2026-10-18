<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  deleteMod,
  listBackups,
  listMods,
  restoreBackup,
  type BackupEntry,
  type ModRecord,
} from "../lib/modbox";

const backups = ref<BackupEntry[]>([]);
const mods = ref<ModRecord[]>([]);
const busy = ref("");
const message = ref("");

async function refresh() {
  try {
    backups.value = await listBackups();
    mods.value = await listMods();
  } catch (e) {
    message.value = String(e);
  }
}

async function doRestore(hash: string) {
  busy.value = hash;
  try {
    await restoreBackup(hash);
    message.value = `已还原 ${hash.slice(0, 12)}…`;
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = "";
    await refresh();
  }
}

async function doDelete(id: string) {
  busy.value = id;
  try {
    await deleteMod(id);
    message.value = "已删除 Mod 记录（游戏文件未变，可用备份还原）";
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = "";
    await refresh();
  }
}

function fmtTime(s?: string) {
  if (!s) return "—";
  return s.replace("T", " ").slice(0, 19);
}

function fmtSize(n: number) {
  if (!n) return "—";
  return (n / 1024 / 1024).toFixed(1) + " MB";
}

onMounted(refresh);
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">资源库</h1>
        <p class="page-subtitle">Mod 管理与原始文件备份</p>
      </div>
      <div class="page-actions">
        <button class="btn" type="button" :disabled="!!busy" @click="refresh">
          <span class="glyph">&#xE72C;</span>
          刷新
        </button>
      </div>
    </header>

    <div v-if="message" class="card" style="margin-bottom: 16px">
      <div class="card__desc">{{ message }}</div>
    </div>

    <article class="card">
      <div class="row" style="justify-content: space-between; align-items: center">
        <h3 class="card__title">已安装 Mod</h3>
        <span class="chip">{{ mods.length }} 个</span>
      </div>
      <p class="card__desc" style="margin-top: 6px">
        每次安装都会先备份原始 bundle，再覆盖游戏文件。删掉记录不会还原文件，请用下方备份。
      </p>

      <div v-if="mods.length === 0" class="empty" style="padding: 28px 0">
        <p class="empty__text">还没有安装过 Mod。到「模型」页选角色导出/安装即可。</p>
      </div>

      <div v-else class="list" style="margin-top: 12px">
        <div v-for="m in mods" :key="m.id" class="list__row">
          <div class="list__main">
            <div class="list__title">{{ m.character || m.preset || m.id }}</div>
            <div class="list__meta mono">
              {{ m.mesh }} · {{ m.bundle?.slice(0, 12) }}… · {{ fmtTime(m.installed_at) }}
            </div>
          </div>
          <div class="list__actions">
            <button
              class="btn"
              type="button"
              :disabled="!!busy || !m.bundle"
              @click="doRestore(m.bundle!)"
            >
              还原
            </button>
            <button class="btn" type="button" :disabled="!!busy" @click="doDelete(m.id)">
              删除记录
            </button>
          </div>
        </div>
      </div>
    </article>

    <article class="card" style="margin-top: 16px">
      <div class="row" style="justify-content: space-between; align-items: center">
        <h3 class="card__title">原始文件备份</h3>
        <span class="chip">{{ backups.length }} 个</span>
      </div>
      <p class="card__desc" style="margin-top: 6px">
        备份位置：<span class="mono">%APPDATA%\com.exmt.modbox\backups\</span>
      </p>

      <div v-if="backups.length === 0" class="empty" style="padding: 28px 0">
        <p class="empty__text">还没有备份。第一次安装 Mod 时会自动生成。</p>
      </div>

      <div v-else class="list" style="margin-top: 12px">
        <div v-for="b in backups" :key="b.hash" class="list__row">
          <div class="list__main">
            <div class="list__title mono">{{ b.hash }}</div>
            <div class="list__meta">{{ fmtSize(b.size) }} · {{ fmtTime(b.created_at) }}</div>
          </div>
          <div class="list__actions">
            <button class="btn" type="button" :disabled="!!busy" @click="doRestore(b.hash)">
              还原到游戏
            </button>
          </div>
        </div>
      </div>
    </article>
  </section>
</template>

<style scoped>
.list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.list__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 14px;
  border-radius: 8px;
  background: var(--card-subtle, rgba(255, 255, 255, 0.03));
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.06));
}

.list__title {
  font-weight: 600;
}

.list__meta {
  font-size: 12px;
  opacity: 0.65;
  margin-top: 2px;
}

.list__actions {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
}
</style>
