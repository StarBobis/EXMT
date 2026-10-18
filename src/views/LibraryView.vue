<script setup lang="ts">
const planned = [
  { icon: "\uE8B7", label: "游戏目录", desc: "记录一个或多个游戏安装路径" },
  { icon: "\uE8F1", label: "资源清单", desc: "扫描 AssetBundle 并建立索引" },
  { icon: "\uE74E", label: "备份区", desc: "改动前自动保存原始文件" },
];
</script>

<template>
  <section class="page">
    <header class="page-header">
      <div>
        <h1 class="page-title">资源库</h1>
        <p class="page-subtitle">游戏资源与 Mod 包</p>
      </div>
      <div class="page-actions">
        <button class="btn" type="button" disabled>导入 Mod 包</button>
        <button class="btn btn--accent" type="button" disabled>选择游戏目录</button>
      </div>
    </header>

    <div class="card">
      <div class="empty">
        <div class="empty__icon">
          <span class="glyph">&#xE8B7;</span>
        </div>
        <h3 class="empty__title">尚未连接任何资源目录</h3>
        <p class="empty__text">
          选中游戏安装目录后，这里会列出可改写的 AssetBundle 与已经安装的 Mod。
          目录读取与写入权限已在本窗口中放开。
        </p>
        <div class="empty__actions">
          <button class="btn" type="button" disabled>选择目录</button>
          <button class="btn" type="button" disabled>扫描资源</button>
        </div>
      </div>
    </div>

    <div class="grid grid--3" style="margin-top: 16px">
      <article v-for="item in planned" :key="item.label" class="card feature">
        <div class="feature__icon">
          <span class="glyph">{{ item.icon }}</span>
        </div>
        <h3 class="feature__title">{{ item.label }}</h3>
        <p class="feature__desc">{{ item.desc }}</p>
      </article>
    </div>
  </section>
</template>
