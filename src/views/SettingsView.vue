<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useTheme, type ThemeMode } from "../lib/theme";
import { appVersions, isTauri } from "../lib/tauri";

const { mode, setMode } = useTheme();

const options: { value: ThemeMode; label: string }[] = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

const version = ref("0.1.0");
const tauri = ref("—");

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
        <h1 class="page-title">设置</h1>
        <p class="page-subtitle">应用、路径与权限</p>
      </div>
    </header>

    <div class="section">
      <div class="section__title">外观</div>
      <div class="setting-row">
        <div class="setting-row__text">
          <div class="setting-row__title">主题</div>
          <div class="setting-row__desc">浅色 / 深色，或跟随 Windows 的个性化设置。</div>
        </div>
        <div class="setting-row__control">
          <div class="segmented" role="group" aria-label="主题">
            <button
              v-for="option in options"
              :key="option.value"
              type="button"
              class="segmented__item"
              :class="{ 'is-active': mode === option.value }"
              :aria-pressed="mode === option.value"
              @click="setMode(option.value)"
            >
              {{ option.label }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="section">
      <div class="section__title">路径与权限</div>
      <div class="setting-row">
        <div class="setting-row__text">
          <div class="setting-row__title">文件系统</div>
          <div class="setting-row__desc">
            已放开任意盘符、任意目录的读取与写入，便于直接改写游戏资源。
          </div>
        </div>
        <div class="setting-row__control">
          <span class="status"><span class="status__dot" />已放行</span>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__text">
          <div class="setting-row__title">网络请求</div>
          <div class="setting-row__desc">
            允许访问任意 http / https 地址，用于拉取更新与远端清单。
          </div>
        </div>
        <div class="setting-row__control">
          <span class="status"><span class="status__dot" />已放行</span>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__text">
          <div class="setting-row__title">游戏目录</div>
          <div class="setting-row__desc">尚未设置，资源库接入后可在此修改。</div>
        </div>
        <div class="setting-row__control">
          <span class="status"><span class="status__dot status__dot--warn" />未设置</span>
        </div>
      </div>
    </div>

    <div class="section">
      <div class="section__title">关于</div>
      <div class="setting-row">
        <div class="setting-row__text">
          <div class="setting-row__title">EXMT Modbox</div>
          <div class="setting-row__desc">本地资源与 Mod 工作台</div>
        </div>
        <div class="setting-row__control mono">{{ version }}</div>
      </div>
      <div class="setting-row">
        <div class="setting-row__text">
          <div class="setting-row__title">Tauri / 运行环境</div>
          <div class="setting-row__desc">
            {{ isTauri ? "桌面外壳已连接" : "浏览器预览模式" }}
          </div>
        </div>
        <div class="setting-row__control mono">{{ tauri }}</div>
      </div>
    </div>
  </section>
</template>
