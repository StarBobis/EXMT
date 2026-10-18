<script setup lang="ts">
import { onMounted, ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTheme, type ThemeMode } from "../lib/theme";
import { appVersions, isTauri } from "../lib/tauri";
import { locale, setLocale, t } from "../lib/i18n";
import {
  cacheFolder,
  chooseCacheFolder,
  openCacheFolder as openCacheFolderInExplorer,
  useCacheFolder,
} from "../lib/settings";
import AppLogo from "../components/AppLogo.vue";

const { mode, setMode } = useTheme();
const themeModes: { value: ThemeMode; key: "settings.themeSystem" | "settings.themeLight" | "settings.themeDark" }[] = [
  { value: "system", key: "settings.themeSystem" },
  { value: "light", key: "settings.themeLight" },
  { value: "dark", key: "settings.themeDark" },
];

const version = ref("0.1.0");
const tauri = ref("—");
const cacheError = ref("");

onMounted(async () => {
  const v = await appVersions();
  version.value = v.app;
  tauri.value = v.tauri;
});

async function changeCacheFolder() {
  try {
    const picked = await chooseCacheFolder(t("settings.cachePick"));
    if (!picked) return;
    await useCacheFolder(picked);
    cacheError.value = "";
  } catch (e) {
    cacheError.value = t("settings.cacheFail", { error: String(e) });
  }
}

async function openCacheFolder() {
  try {
    await openCacheFolderInExplorer();
    cacheError.value = "";
  } catch (e) {
    cacheError.value = t("settings.cacheFail", { error: String(e) });
  }
}

/** 外链集中在这里，方便以后替换成正式地址 */
const LINKS = {
  docs: "http://127.0.0.1:5200/",
  guide: "http://127.0.0.1:5200/guide/replace-model",
  sponsor: "https://github.com/StarBobis/EXMT",
  repo: "https://github.com/StarBobis/EXMT",
};

async function openLink(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank");
  }
}
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">{{ t("settings.title") }}</h1>
        <p class="page-subtitle">{{ t("settings.subtitle") }}</p>
      </div>
    </header>

    <div class="settings">
      <!-- 左：设置项 7 -->
      <div class="settings__main">
        <div class="section">
          <div class="section__title">{{ t("settings.general") }}</div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">{{ t("settings.language") }}</div>
              <div class="setting-row__desc">{{ t("settings.languageDesc") }}</div>
            </div>
            <div class="setting-row__control">
              <div class="segmented" role="group" :aria-label="t('settings.language')">
                <button
                  type="button"
                  class="segmented__item"
                  :class="{ 'is-active': locale === 'zh-CN' }"
                  :aria-pressed="locale === 'zh-CN'"
                  @click="setLocale('zh-CN')"
                >
                  简体中文
                </button>
                <button
                  type="button"
                  class="segmented__item"
                  :class="{ 'is-active': locale === 'en-US' }"
                  :aria-pressed="locale === 'en-US'"
                  @click="setLocale('en-US')"
                >
                  English
                </button>
              </div>
            </div>
          </div>
        </div>

        <div class="section">
          <div class="section__title">{{ t("settings.appearance") }}</div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">{{ t("settings.theme") }}</div>
              <div class="setting-row__desc">{{ t("settings.themeDesc") }}</div>
            </div>
            <div class="setting-row__control">
              <div class="segmented" role="group" :aria-label="t('settings.theme')">
                <button
                  v-for="option in themeModes"
                  :key="option.value"
                  type="button"
                  class="segmented__item"
                  :class="{ 'is-active': mode === option.value }"
                  :aria-pressed="mode === option.value"
                  @click="setMode(option.value)"
                >
                  {{ t(option.key) }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <div class="section">
          <div class="section__title">{{ t("settings.paths") }}</div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">{{ t("settings.cache") }}</div>
              <div class="setting-row__desc">{{ t("settings.cacheDesc") }}</div>
              <div class="pathline mono" :title="cacheFolder">{{ cacheFolder }}</div>
              <div v-if="cacheError" class="pathline pathline--error">{{ cacheError }}</div>
            </div>
            <div class="setting-row__control">
              <button
                class="btn"
                type="button"
                :disabled="!isTauri"
                @click="changeCacheFolder"
              >
                {{ t("settings.cacheChange") }}
              </button>
              <button class="btn" type="button" :disabled="!isTauri" @click="openCacheFolder">
                <span class="glyph">&#xE8E5;</span>
                {{ t("settings.cacheOpen") }}
              </button>
            </div>
          </div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">{{ t("settings.fs") }}</div>
              <div class="setting-row__desc">{{ t("settings.fsDesc") }}</div>
            </div>
            <div class="setting-row__control">
              <span class="status"><span class="status__dot" />{{ t("settings.allowed") }}</span>
            </div>
          </div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">{{ t("settings.net") }}</div>
              <div class="setting-row__desc">{{ t("settings.netDesc") }}</div>
            </div>
            <div class="setting-row__control">
              <span class="status"><span class="status__dot" />{{ t("settings.allowed") }}</span>
            </div>
          </div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">{{ t("settings.game") }}</div>
              <div class="setting-row__desc">{{ t("settings.gameDesc") }}</div>
            </div>
            <div class="setting-row__control">
              <span class="status">
                <span class="status__dot status__dot--warn" />{{ t("settings.notSet") }}
              </span>
            </div>
          </div>
        </div>

        <div class="section">
          <div class="section__title">{{ t("settings.runtime") }}</div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">Tauri</div>
              <div class="setting-row__desc">
                {{ isTauri ? t("settings.envConnected") : t("settings.envBrowser") }}
              </div>
            </div>
            <div class="setting-row__control mono">{{ tauri }}</div>
          </div>
        </div>
      </div>

      <!-- 右：关于 3 -->
      <aside class="about">
        <div class="about__logo"><AppLogo class="about__mark" /></div>
        <h2 class="about__name">EXMT Modbox</h2>
        <p class="about__version mono">v{{ version }}</p>
        <p class="about__desc">{{ t("settings.aboutDesc") }}</p>

        <div class="about__actions">
          <button class="btn btn--accent" type="button" @click="openLink(LINKS.docs)">
            <span class="glyph">&#xE82D;</span>
            {{ t("settings.aboutDocs") }}
          </button>
          <button class="btn" type="button" @click="openLink(LINKS.guide)">
            {{ t("settings.aboutGuide") }}
          </button>
          <button class="btn" type="button" @click="openLink(LINKS.sponsor)">
            <span class="glyph">&#xEB51;</span>
            {{ t("settings.aboutSponsor") }}
          </button>
        </div>

        <div class="about__meta">
          <span>Tauri 2 · Vue 3 · Rust</span>
          <span class="mono">{{ tauri }}</span>
        </div>
      </aside>
    </div>
  </section>
</template>

<style scoped>
.settings {
  display: grid;
  grid-template-columns: minmax(0, 7fr) minmax(0, 3fr);
  gap: 16px;
  align-items: start;
}

.settings > * {
  min-width: 0;
}

.settings__main {
  min-width: 0;
}

/* 缓存路径：长路径要能折行，不能把右侧按钮挤掉 */
.pathline {
  margin-top: 6px;
  font-size: 11.5px;
  line-height: 17px;
  color: var(--text-2);
  word-break: break-all;
  user-select: text;
}

.pathline--error {
  color: #ff9b9b;
}

.about {
  position: sticky;
  top: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 22px 18px 18px;
  border: 1px solid var(--stroke);
  border-radius: 12px;
  background: var(--card-subtle, rgba(255, 255, 255, 0.03));
}

.about__logo {
  width: 64px;
  height: 64px;
  border-radius: 16px;
  display: grid;
  place-items: center;
  background: linear-gradient(135deg, #2b6cff, #4cc2ff);
  box-shadow: 0 8px 24px rgba(43, 108, 255, 0.28);
}

.about__mark {
  width: 36px;
  height: 36px;
}

.about__name {
  margin: 14px 0 2px;
  font-size: 17px;
  font-weight: 600;
}

.about__version {
  margin: 0;
  font-size: 12px;
  opacity: 0.6;
}

.about__desc {
  margin: 12px 0 0;
  font-size: 12px;
  line-height: 19px;
  opacity: 0.75;
}

.about__desc code {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 1px 4px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.08);
}

.about__actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  margin-top: 16px;
}

.about__actions .btn {
  justify-content: center;
  width: 100%;
}

.about__meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--stroke);
  width: 100%;
  font-size: 10.5px;
  opacity: 0.5;
}

@media (max-width: 900px) {
  .settings {
    grid-template-columns: 1fr;
  }
}
</style>
