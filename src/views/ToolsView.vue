<script setup lang="ts">
import type { MessageKey } from "../lib/i18n";
import { t } from "../lib/i18n";

const tools: { icon: string; titleKey: MessageKey; descKey: MessageKey }[] = [
  { icon: "\uE790", titleKey: "tools.t1.title", descKey: "tools.t1.desc" },
  { icon: "\uE8B7", titleKey: "tools.t2.title", descKey: "tools.t2.desc" },
  { icon: "\uE943", titleKey: "tools.t3.title", descKey: "tools.t3.desc" },
  { icon: "\uE72C", titleKey: "tools.t4.title", descKey: "tools.t4.desc" },
];
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">{{ t("tools.title") }}</h1>
        <p class="page-subtitle">{{ t("tools.subtitle") }}</p>
      </div>
      <div class="page-actions">
        <span class="chip">
          <span class="glyph">&#xE946;</span>
          {{ t("tools.pending", { n: tools.length }) }}
        </span>
      </div>
    </header>

    <div class="grid grid--2">
      <article v-for="item in tools" :key="item.titleKey" class="card feature">
        <div class="feature__icon">
          <span class="glyph">{{ item.icon }}</span>
        </div>
        <h3 class="feature__title">{{ t(item.titleKey) }}</h3>
        <p class="feature__desc">{{ t(item.descKey) }}</p>
        <div class="feature__footer">
          <span class="chip">{{ t("tools.planned") }}</span>
        </div>
      </article>
    </div>
  </section>
</template>
