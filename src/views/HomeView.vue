<script setup lang="ts">
import { onMounted, ref } from "vue";
import { appVersions } from "../lib/tauri";

const version = ref("0.1.0");
const tauri = ref("—");

const features = [
  {
    icon: "\uE8B7",
    title: "资源包",
    desc: "浏览游戏目录、定位目标 bundle，并把原始文件备份到本地工作区。",
  },
  {
    icon: "\uE790",
    title: "贴图替换",
    desc: "按资源名替换贴图并重新打包，支持批量与预览。",
  },
  {
    icon: "\uE943",
    title: "模型与脚本",
    desc: "预留的模型注入与运行时 hook 面板，需配合外挂进程。",
  },
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
        <h1 class="page-title">首页</h1>
        <p class="page-subtitle">本地资源与 Mod 工作台</p>
      </div>
      <div class="page-actions">
        <span class="chip chip--accent">
          <span class="glyph">&#xE946;</span>
          界面骨架
        </span>
      </div>
    </header>

    <div class="hero card">
      <div class="hero__mark">
        <span class="glyph">&#xE90F;</span>
      </div>
      <div>
        <h2 class="hero__title">欢迎使用 EXMT Modbox</h2>
        <p class="hero__text">
          侧边导航与自定义标题栏已经就绪；工具模块尚未接入，接下来会从“资源包浏览 +
          贴图替换”开始。
        </p>
      </div>
      <div style="margin-left: auto">
        <button class="btn btn--accent" type="button" disabled>打开游戏目录</button>
      </div>
    </div>

    <div class="grid grid--3">
      <article v-for="item in features" :key="item.title" class="card feature">
        <div class="feature__icon">
          <span class="glyph">{{ item.icon }}</span>
        </div>
        <h3 class="feature__title">{{ item.title }}</h3>
        <p class="feature__desc">{{ item.desc }}</p>
        <div class="feature__footer">
          <span class="chip">计划中</span>
        </div>
      </article>
    </div>

    <div class="card">
      <h3 class="card__title">运行环境</h3>
      <p class="card__desc" style="margin-bottom: 14px">
        当前界面运行在 Tauri 2 + Rust 外壳中，前端为 Vue 3 + TypeScript。
      </p>
      <div class="kv">
        <div class="kv__k">应用版本</div>
        <div class="kv__v mono">{{ version }}</div>
        <div class="kv__k">Tauri</div>
        <div class="kv__v mono">{{ tauri }}</div>
        <div class="kv__k">前端</div>
        <div class="kv__v mono">Vue 3 · TypeScript · Vite</div>
        <div class="kv__k">外壳</div>
        <div class="kv__v mono">Tauri 2 · Rust</div>
      </div>
    </div>
  </section>
</template>
