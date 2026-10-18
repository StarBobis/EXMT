<script setup lang="ts">
const tools = [
  {
    icon: "\uE790",
    title: "贴图替换",
    desc: "解析 bundle 内的 Texture2D，替换为纯色或指定图片，并保持其余资源不动。",
  },
  {
    icon: "\uE8B7",
    title: "包结构查看",
    desc: "查看 UnityFS 头、块表、节点表与包内资源清单。",
  },
  {
    icon: "\uE943",
    title: "运行时注入",
    desc: "预留：通过 Frida 在游戏进程中定位资源加载点。",
  },
  {
    icon: "\uE72C",
    title: "校验与修复",
    desc: "预留：对比备份、恢复被修改的文件、检查包完整性。",
  },
];
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">工具</h1>
        <p class="page-subtitle">贴图、模型与打包</p>
      </div>
      <div class="page-actions">
        <span class="chip">
          <span class="glyph">&#xE946;</span>
          4 个模块待接入
        </span>
      </div>
    </header>

    <div class="grid grid--2">
      <article v-for="item in tools" :key="item.title" class="card feature">
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
  </section>
</template>
