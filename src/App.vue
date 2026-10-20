<script setup lang="ts">
import { computed, onErrorCaptured, onMounted, ref, watch } from "vue";
import TitleBar from "./components/TitleBar.vue";
import SideBar from "./components/SideBar.vue";
import HomeView from "./views/HomeView.vue";
import ModelsView from "./views/ModelsView.vue";
import LibraryView from "./views/LibraryView.vue";
import SettingsView from "./views/SettingsView.vue";
import SetupView from "./views/SetupView.vue";
import type { ViewKey } from "./lib/nav";
import { currentView } from "./lib/nav";
import { t } from "./lib/i18n";
import { boot, bootError, bootState } from "./lib/boot";
import { gameCache } from "./lib/gamecache";

const STORAGE_KEY = "exmt.nav.collapsed";

const collapsed = ref(localStorage.getItem(STORAGE_KEY) === "1");

watch(collapsed, (v) => localStorage.setItem(STORAGE_KEY, v ? "1" : "0"));

const views = {
  home: HomeView,
  models: ModelsView,
  library: LibraryView,
  settings: SettingsView,
} as const;

const viewComponent = computed(() => views[currentView.value]);
/**
 * Pages that read game assets need the analysed cache; the setup wizard is the
 * only way to get one, so this gate now points at the cache, not the path.
 */
const needsCache = computed(() => ["models", "library"].includes(currentView.value));
const cacheReady = computed(() => bootState.value === "ready" && !!gameCache.value);

function selectView(key: ViewKey) {
  currentView.value = key;
}

/**
 * One page failing must not leave the previous page's DOM on screen (that is
 * what a silent Vue patch abort looks like: you click Settings and nothing
 * happens). Show the error instead, so it can be reported.
 */
const viewError = ref("");

onErrorCaptured((error) => {
  viewError.value = String((error as Error)?.stack || error);
  return false;
});

watch(currentView, () => {
  viewError.value = "";
});

onMounted(() => void boot());
</script>

<template>
  <div class="app">
    <TitleBar title="EXMT Modbox" :badge="t('app.preview')" />
    <div v-if="bootState === 'loading'" class="app__boot">
      <span class="app__spinner" aria-hidden="true" />
      {{ t("app.loading") }}
    </div>
    <div v-else-if="bootState === 'error'" class="app__boot app__boot--error">
      <p>{{ bootError }}</p>
      <button class="btn" type="button" @click="boot()">{{ t("settings.cacheRebuild") }}</button>
    </div>
    <SetupView v-else-if="bootState === 'setup'" class="app__setup" />
    <div v-else class="app__body">
      <SideBar
        :selected="currentView"
        :collapsed="collapsed"
        @select="selectView"
        @toggle="collapsed = !collapsed"
        @collapse="collapsed = true"
      />
      <main class="app__content">
        <pre v-if="viewError" class="app__viewerror">{{ viewError }}</pre>
        <KeepAlive v-if="!needsCache || cacheReady">
          <component :is="viewComponent" :key="currentView" />
        </KeepAlive>
        <section v-else class="gate">
          <div class="gate__icon glyph">&#xE7BA;</div>
          <h2 class="gate__title">{{ t("gate.cacheTitle") }}</h2>
          <p class="gate__body">{{ t("gate.cacheBody") }}</p>
          <button class="btn btn--accent" type="button" @click="selectView('settings')">
            {{ t("gate.action") }}
          </button>
        </section>
      </main>
    </div>
  </div>
</template>
