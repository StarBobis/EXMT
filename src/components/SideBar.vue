<script setup lang="ts">
import type { NavItem, ViewKey } from "../lib/nav";
import { NAV_FOOTER, NAV_PRIMARY } from "../lib/nav";

defineProps<{ selected: ViewKey; collapsed: boolean }>();
const emit = defineEmits<{
  (e: "select", key: ViewKey): void;
  (e: "toggle"): void;
}>();

function select(item: NavItem) {
  emit("select", item.key);
}
</script>

<template>
  <nav class="sidebar" :class="{ 'sidebar--collapsed': collapsed }" aria-label="主导航">
    <!-- NavigationView 的 PaneToggleButton -->
    <div class="sidebar__head">
      <button
        type="button"
        class="nav-toggle"
        :title="collapsed ? '展开导航' : '折叠导航'"
        :aria-expanded="!collapsed"
        @click="emit('toggle')"
      >
        <span class="glyph">&#xE700;</span>
      </button>
      <span class="sidebar__label">工具箱</span>
    </div>

    <ul class="nav-list">
      <li v-for="item in NAV_PRIMARY" :key="item.key">
        <button
          type="button"
          class="nav-item"
          :class="{ 'is-selected': selected === item.key }"
          :aria-current="selected === item.key ? 'page' : undefined"
          :title="item.label"
          @click="select(item)"
        >
          <span class="nav-item__icon glyph">{{ item.icon }}</span>
          <span class="nav-item__label">{{ item.label }}</span>
        </button>
      </li>
    </ul>

    <div class="sidebar__spacer" />

    <div class="sidebar__footer">
      <button
        v-for="item in NAV_FOOTER"
        :key="item.key"
        type="button"
        class="nav-item"
        :class="{ 'is-selected': selected === item.key }"
        :aria-current="selected === item.key ? 'page' : undefined"
        :title="item.label"
        @click="select(item)"
      >
        <span class="nav-item__icon glyph">{{ item.icon }}</span>
        <span class="nav-item__label">{{ item.label }}</span>
      </button>
    </div>
  </nav>
</template>
