<script setup lang="ts">
import { onMounted, ref } from "vue";
import { appVersions } from "../lib/tauri";
import { locale, t } from "../lib/i18n";

const lines = ref<string[]>([]);

onMounted(async () => {
  const v = await appVersions();
  const stamp = new Date().toLocaleTimeString(locale.value, { hour12: false });
  lines.value = [
    t("logs.banner", { time: stamp, app: v.app, tauri: v.tauri }),
    t("logs.ready", { time: stamp }),
    t("logs.idle", { time: stamp }),
  ];
});
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">{{ t("logs.title") }}</h1>
        <p class="page-subtitle">{{ t("logs.subtitle") }}</p>
      </div>
      <div class="page-actions">
        <button class="btn" type="button" disabled>{{ t("logs.clear") }}</button>
        <button class="btn" type="button" disabled>{{ t("logs.export") }}</button>
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
          {{ t("logs.hint") }}<span class="terminal__cursor" />
        </div>
      </div>
    </div>
  </section>
</template>
