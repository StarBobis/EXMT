<script setup lang="ts">
import { onMounted, ref } from "vue";
import { appVersions } from "../lib/tauri";

const lines = ref<string[]>([]);

onMounted(async () => {
  const v = await appVersions();
  const stamp = new Date().toLocaleTimeString("zh-CN", { hour12: false });
  lines.value = [
    `${stamp}  EXMT Modbox ${v.app} · Tauri ${v.tauri}`,
    `${stamp}  shell ready — sidebar / titlebar mounted`,
    `${stamp}  no background tasks yet; tool output will stream here`,
  ];
});
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">日志</h1>
        <p class="page-subtitle">运行与诊断输出</p>
      </div>
      <div class="page-actions">
        <button class="btn" type="button" disabled>清空</button>
        <button class="btn" type="button" disabled>导出</button>
      </div>
    </header>

    <div class="terminal">
      <div class="terminal__bar">
        <span class="terminal__dot" />
        <span class="terminal__dot" />
        <span class="terminal__dot" />
        <span style="margin-left: 8px">output</span>
      </div>
      <div class="terminal__body">
        <div v-for="(line, index) in lines" :key="index">{{ line }}</div>
        <div class="terminal__line--dim">
          // 任务接入后，这里显示实时输出与错误堆栈<span class="terminal__cursor" />
        </div>
      </div>
    </div>
  </section>
</template>
