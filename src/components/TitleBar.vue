<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import AppLogo from "./AppLogo.vue";
import { isTauri, tauriWindow } from "../lib/tauri";
import { t } from "../lib/i18n";

withDefaults(
  defineProps<{
    title?: string;
    badge?: string;
  }>(),
  { title: "EXMT Modbox", badge: "" },
);

const maximized = ref(false);
let unlisten: (() => void) | undefined;

async function syncMaximized() {
  const win = await tauriWindow();
  if (!win) return;
  try {
    maximized.value = await win.isMaximized();
  } catch {
    /* window may be closing */
  }
}

async function minimize() {
  const win = await tauriWindow();
  await win?.minimize();
}

async function toggleMaximize() {
  const win = await tauriWindow();
  await win?.toggleMaximize();
}

async function close() {
  const win = await tauriWindow();
  await win?.close();
}

onMounted(async () => {
  if (!isTauri) return;
  await syncMaximized();
  const win = await tauriWindow();
  if (!win) return;
  try {
    unlisten = await win.onResized(() => {
      void syncMaximized();
    });
  } catch {
    /* ignore */
  }
});

onBeforeUnmount(() => {
  unlisten?.();
});
</script>

<template>
  <!-- Double-click on the drag region is handled by Tauri itself
       (core:window:allow-internal-toggle-maximize). -->
  <header class="titlebar" data-tauri-drag-region>
    <div class="titlebar__brand" data-tauri-drag-region>
      <AppLogo class="titlebar__logo" />
      <span class="titlebar__title" data-tauri-drag-region>{{ title }}</span>
      <span v-if="badge" class="titlebar__badge">{{ badge }}</span>
    </div>

    <div class="titlebar__spacer" data-tauri-drag-region />

    <div class="caption-buttons" @dblclick.stop>
      <button class="caption-btn" type="button" :title="t('win.minimize')" @click="minimize">
        <span class="glyph">&#xE921;</span>
      </button>
      <button
        class="caption-btn"
        type="button"
        :title="maximized ? t('win.restore') : t('win.maximize')"
        @click="toggleMaximize"
      >
        <span class="glyph">{{ maximized ? "\uE923" : "\uE922" }}</span>
      </button>
      <button
        class="caption-btn caption-btn--close"
        type="button"
        :title="t('win.close')"
        @click="close"
      >
        <span class="glyph">&#xE8BB;</span>
      </button>
    </div>
  </header>
</template>
