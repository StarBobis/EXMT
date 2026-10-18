<script setup lang="ts">
import { onMounted, ref } from "vue";
import { appVersions } from "../lib/tauri";
import type { MessageKey } from "../lib/i18n";
import { t } from "../lib/i18n";

const version = ref("0.1.0");
const tauri = ref("—");

const features: { icon: string; titleKey: MessageKey; descKey: MessageKey }[] = [
  { icon: "\uE8B7", titleKey: "home.f1.title", descKey: "home.f1.desc" },
  { icon: "\uE790", titleKey: "home.f2.title", descKey: "home.f2.desc" },
  { icon: "\uE943", titleKey: "home.f3.title", descKey: "home.f3.desc" },
];

onMounted(async () => {
  const v = await appVersions();
  version.value = v.app;
  tauri.value = v.tauri;
});
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">{{ t("home.title") }}</h1>
        <p class="page-subtitle">{{ t("home.subtitle") }}</p>
      </div>
      <div class="page-actions">
        <span class="chip chip--accent">
          <span class="glyph">&#xE946;</span>
          {{ t("home.chip") }}
        </span>
      </div>
    </header>

    <div class="hero card">
      <div class="hero__mark">
        <span class="glyph">&#xE90F;</span>
      </div>
      <div>
        <h2 class="hero__title">{{ t("home.hero.title") }}</h2>
        <p class="hero__text">{{ t("home.hero.text") }}</p>
      </div>
      <div style="margin-left: auto">
        <button class="btn btn--accent" type="button" disabled>
          {{ t("home.openGame") }}
        </button>
      </div>
    </div>

    <div class="grid grid--3">
      <article v-for="item in features" :key="item.titleKey" class="card feature">
        <div class="feature__icon">
          <span class="glyph">{{ item.icon }}</span>
        </div>
        <h3 class="feature__title">{{ t(item.titleKey) }}</h3>
        <p class="feature__desc">{{ t(item.descKey) }}</p>
        <div class="feature__footer">
          <span class="chip">{{ t("home.planned") }}</span>
        </div>
      </article>
    </div>

    <div class="card">
      <h3 class="card__title">{{ t("home.env.title") }}</h3>
      <p class="card__desc" style="margin-bottom: 14px">{{ t("home.env.desc") }}</p>
      <div class="kv">
        <div class="kv__k">{{ t("home.env.app") }}</div>
        <div class="kv__v mono">{{ version }}</div>
        <div class="kv__k">Tauri</div>
        <div class="kv__v mono">{{ tauri }}</div>
        <div class="kv__k">{{ t("home.env.frontend") }}</div>
        <div class="kv__v mono">Vue 3 · TypeScript · Vite</div>
        <div class="kv__k">{{ t("home.env.shell") }}</div>
        <div class="kv__v mono">Tauri 2 · Rust</div>
      </div>
    </div>
  </section>
</template>
