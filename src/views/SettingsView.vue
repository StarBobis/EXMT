<script setup lang="ts">
import { onMounted, ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTheme, type ThemeMode } from "../lib/theme";
import { appVersions, isTauri } from "../lib/tauri";
import AppLogo from "../components/AppLogo.vue";

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
        <h1 class="page-title">设置</h1>
        <p class="page-subtitle">应用、路径与权限</p>
      </div>
    </header>

    <div class="settings">
      <!-- 左：设置项 7 -->
      <div class="settings__main">
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
              <div class="setting-row__desc">允许访问任意 http / https 地址，用于拉取更新与远端清单。</div>
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
          <div class="section__title">运行环境</div>
          <div class="setting-row">
            <div class="setting-row__text">
              <div class="setting-row__title">Tauri</div>
              <div class="setting-row__desc">
                {{ isTauri ? "桌面外壳已连接" : "浏览器预览模式" }}
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
        <p class="about__desc">
          面向《少女前线2：追放》的离线模型工具箱：选角色、导出 <code>.exmtmesh</code>、
          在 Blender 里改完，一键打包回游戏。
        </p>

        <div class="about__actions">
          <button class="btn btn--accent" type="button" @click="openLink(LINKS.docs)">
            <span class="glyph">&#xE82D;</span>
            使用文档
          </button>
          <button class="btn" type="button" @click="openLink(LINKS.guide)">替换模型教程</button>
          <button class="btn" type="button" @click="openLink(LINKS.sponsor)">
            <span class="glyph">&#xEB51;</span>
            赞助与主页
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
