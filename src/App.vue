<script setup lang="ts">
import { computed, ref, watch } from "vue";
import TitleBar from "./components/TitleBar.vue";
import SideBar from "./components/SideBar.vue";
import HomeView from "./views/HomeView.vue";
import ModelsView from "./views/ModelsView.vue";
import LibraryView from "./views/LibraryView.vue";
import ToolsView from "./views/ToolsView.vue";
import LogsView from "./views/LogsView.vue";
import SettingsView from "./views/SettingsView.vue";
import type { ViewKey } from "./lib/nav";
import { t } from "./lib/i18n";

const STORAGE_KEY = "exmt.nav.collapsed";

const current = ref<ViewKey>("home");
const collapsed = ref(localStorage.getItem(STORAGE_KEY) === "1");

watch(collapsed, (v) => localStorage.setItem(STORAGE_KEY, v ? "1" : "0"));

const views = {
  home: HomeView,
  models: ModelsView,
  library: LibraryView,
  tools: ToolsView,
  logs: LogsView,
  settings: SettingsView,
} as const;

const currentView = computed(() => views[current.value]);
</script>

<template>
  <div class="app">
    <TitleBar title="EXMT Modbox" :badge="t('app.preview')" />
    <div class="app__body">
      <SideBar
        :selected="current"
        :collapsed="collapsed"
        @select="current = $event"
        @toggle="collapsed = !collapsed"
      />
      <main class="app__content">
        <component :is="currentView" :key="current" />
      </main>
    </div>
  </div>
</template>
