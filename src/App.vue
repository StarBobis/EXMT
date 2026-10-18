<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
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
import { checkGamePath, gamePath, gameReady } from "./lib/settings";

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
/** 这三页都要读游戏资源，没设游戏路径前不给进。 */
const gated = computed(() => ["models", "library", "tools"].includes(current.value));

// 启动时先验证已存的游戏路径，否则直接从导航进模型页会误判成"没设置"。
onMounted(() => {
  if (gamePath.value) void checkGamePath(gamePath.value);
});
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
        <KeepAlive v-if="!gated || gameReady">
          <component :is="currentView" :key="current" />
        </KeepAlive>
        <section v-else class="gate">
          <div class="gate__icon glyph">&#xE7BA;</div>
          <h2 class="gate__title">{{ t("gate.title") }}</h2>
          <p class="gate__body">{{ t("gate.body") }}</p>
          <button class="btn btn--accent" type="button" @click="current = 'settings'">
            {{ t("gate.action") }}
          </button>
        </section>
      </main>
    </div>
  </div>
</template>
