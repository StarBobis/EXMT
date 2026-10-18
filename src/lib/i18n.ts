/**
 * Tiny type-checked i18n for the shell.
 *
 * A dictionary instead of vue-i18n on purpose: there is one window and ~110
 * strings, and `Record<keyof typeof zh, string>` makes TypeScript reject a
 * missing or misspelled English entry at build time. JSON messages would
 * silently fall back to the key at runtime instead.
 */
import { ref, watch } from "vue";

export type Locale = "zh-CN" | "en-US";

const STORAGE_KEY = "exmt.locale";

/** Source of truth: every other locale must cover exactly these keys. */
const zh = {
  "app.preview": "预览",

  "nav.home": "首页",
  "nav.models": "模型",
  "nav.library": "资源库",
  "nav.tools": "工具",
  "nav.logs": "日志",
  "nav.settings": "设置",
  "nav.hint.home": "概览与最近操作",
  "nav.hint.models": "角色预设与模型替换",
  "nav.hint.library": "游戏资源与 Mod 包",
  "nav.hint.tools": "贴图、模型与打包",
  "nav.hint.logs": "运行与诊断输出",
  "nav.hint.settings": "应用、路径与权限",
  "nav.expand": "展开导航",
  "nav.collapse": "折叠导航",
  "nav.toolbox": "工具箱",
  "nav.aria": "主导航",

  "win.minimize": "最小化",
  "win.maximize": "最大化",
  "win.restore": "向下还原",
  "win.close": "关闭",

  "home.title": "首页",
  "home.subtitle": "本地资源与 Mod 工作台",
  "home.chip": "界面骨架",
  "home.hero.title": "欢迎使用 EXMT Modbox",
  "home.hero.text":
    "侧边导航与自定义标题栏已经就绪；工具模块尚未接入，接下来会从“资源包浏览 + 贴图替换”开始。",
  "home.openGame": "打开游戏目录",
  "home.f1.title": "资源包",
  "home.f1.desc": "浏览游戏目录、定位目标 bundle，并把原始文件备份到本地工作区。",
  "home.f2.title": "贴图替换",
  "home.f2.desc": "按资源名替换贴图并重新打包，支持批量与预览。",
  "home.f3.title": "模型与脚本",
  "home.f3.desc": "预留的模型注入与运行时 hook 面板，需配合外挂进程。",
  "home.planned": "计划中",
  "home.env.title": "运行环境",
  "home.env.desc": "当前界面运行在 Tauri 2 + Rust 外壳中，前端为 Vue 3 + TypeScript。",
  "home.env.app": "应用版本",
  "home.env.frontend": "前端",
  "home.env.shell": "外壳",

  "models.title": "模型",
  "models.subtitle": "选角色 · 导出改 · 装回来",
  "models.search": "搜索角色…",
  "models.chars": "{n} 个角色",
  "models.roster": "{art} 立绘 · {skins} 皮肤",
  "models.rosterNoArt": "无立绘 · {skins} 皮肤",
  "models.titleNoArt": "{name}（无立绘）",
  "models.artboard": "{art} 张立绘 · {skins} 个皮肤",
  "models.noArt": "无立绘",
  "models.noArtHint": "这个角色没有立绘资源，下面直接选皮肤模型。",
  "models.studio.meshes": "网格",
  "models.studio.textures": "贴图",
  "models.studio.verts": "{n} 顶点 · {m} 面",
  "models.studio.loading": "加载中…",
  "models.studio.geometry": "正在解出全部网格…",
  "models.studio.toggle": "点一下显示/隐藏",
  "models.studio.indexing": "正在建立贴图索引（首次约 10 秒）…",
  "models.studio.noMesh": "这个皮肤没有可用的网格",
  "models.studio.noTex": "没有找到贴图，可能与其他皮肤共用",
  "models.studio.browser": "浏览器预览不加载 3D 与贴图，请用桌面版",
  "models.make": "开始制作",
  "models.making": "正在导出到工作空间…",
  "models.made": "已导出 {meshes} 个网格、{textures} 张贴图 → {dir}",

  "library.title": "资源库",
  "library.subtitle": "Mod 管理与原始文件备份",
  "library.refresh": "刷新",
  "library.mods": "已安装 Mod",
  "library.count": "{n} 个",
  "library.modsDesc":
    "每次安装都会先备份原始 bundle，再覆盖游戏文件。删掉记录不会还原文件，请用下方备份。",
  "library.modsEmpty": "还没有安装过 Mod。到「模型」页选角色导出/安装即可。",
  "library.restore": "还原",
  "library.delete": "删除记录",
  "library.backups": "原始文件备份",
  "library.backupsPath": "备份位置：",
  "library.backupsEmpty": "还没有备份。第一次安装 Mod 时会自动生成。",
  "library.restoreToGame": "还原到游戏",
  "library.restored": "已还原 {hash}…",
  "library.deleted": "已删除 Mod 记录（游戏文件未变，可用备份还原）",

  "tools.title": "工具",
  "tools.subtitle": "贴图、模型与打包",
  "tools.pending": "{n} 个模块待接入",
  "tools.t1.title": "贴图替换",
  "tools.t1.desc": "解析 bundle 内的 Texture2D，替换为纯色或指定图片，并保持其余资源不动。",
  "tools.t2.title": "包结构查看",
  "tools.t2.desc": "查看 UnityFS 头、块表、节点表与包内资源清单。",
  "tools.t3.title": "运行时注入",
  "tools.t3.desc": "预留：通过 Frida 在游戏进程中定位资源加载点。",
  "tools.t4.title": "校验与修复",
  "tools.t4.desc": "预留：对比备份、恢复被修改的文件、检查包完整性。",
  "tools.planned": "计划中",

  "logs.title": "日志",
  "logs.subtitle": "运行与诊断输出",
  "logs.clear": "清空",
  "logs.export": "导出",
  "logs.banner": "{time}  EXMT Modbox {app} · Tauri {tauri}",
  "logs.ready": "{time}  外壳已就绪 — 侧栏 / 标题栏已挂载",
  "logs.idle": "{time}  暂无后台任务；工具输出会显示在这里",
  "logs.hint": "// 任务接入后，这里显示实时输出与错误堆栈",

  "settings.title": "设置",
  "settings.subtitle": "应用、路径与权限",
  "settings.general": "常规",
  "settings.language": "语言 / Language",
  "settings.languageDesc": "切换界面语言，立即生效。",
  "settings.appearance": "外观",
  "settings.theme": "主题",
  "settings.themeDesc": "浅色 / 深色，或跟随 Windows 的个性化设置。",
  "settings.themeSystem": "跟随系统",
  "settings.themeLight": "浅色",
  "settings.themeDark": "深色",
  "settings.paths": "路径与权限",
  "settings.cache": "缓存文件夹",
  "settings.cacheDesc": "临时打包、解包与备份文件都放在这里。",
  "settings.cacheChange": "更改…",
  "settings.cacheOpen": "打开",
  "settings.cachePick": "选择缓存文件夹",
  "settings.cacheFail": "无法使用该文件夹：{error}",
  "settings.fs": "文件系统",
  "settings.fsDesc": "已放开任意盘符、任意目录的读取与写入，便于直接改写游戏资源。",
  "settings.net": "网络请求",
  "settings.netDesc": "允许访问任意 http / https 地址，用于拉取更新与远端清单。",
  "settings.game": "游戏目录",
  "settings.gameDesc": "尚未设置，资源库接入后可在此修改。",
  "settings.allowed": "已放行",
  "settings.notSet": "未设置",
  "settings.runtime": "运行环境",
  "settings.envConnected": "桌面外壳已连接",
  "settings.envBrowser": "浏览器预览模式",
  "settings.aboutDesc":
    "面向《少女前线2：追放》的离线模型工具箱：选角色、导出 .exmtmesh、在 Blender 里改完，一键打包回游戏。",
  "settings.aboutDocs": "使用文档",
  "settings.aboutGuide": "替换模型教程",
  "settings.aboutSponsor": "赞助与主页",
} as const;

export type MessageKey = keyof typeof zh;

const en: Record<MessageKey, string> = {
  "app.preview": "Preview",

  "nav.home": "Home",
  "nav.models": "Models",
  "nav.library": "Library",
  "nav.tools": "Tools",
  "nav.logs": "Logs",
  "nav.settings": "Settings",
  "nav.hint.home": "Overview and recent activity",
  "nav.hint.models": "Character presets and mesh swaps",
  "nav.hint.library": "Game assets and mod packages",
  "nav.hint.tools": "Textures, meshes and packing",
  "nav.hint.logs": "Run and diagnostic output",
  "nav.hint.settings": "App, paths and permissions",
  "nav.expand": "Expand navigation",
  "nav.collapse": "Collapse navigation",
  "nav.toolbox": "Toolbox",
  "nav.aria": "Main navigation",

  "win.minimize": "Minimize",
  "win.maximize": "Maximize",
  "win.restore": "Restore down",
  "win.close": "Close",

  "home.title": "Home",
  "home.subtitle": "Local assets and the mod workbench",
  "home.chip": "UI shell",
  "home.hero.title": "Welcome to EXMT Modbox",
  "home.hero.text":
    "The sidebar and custom title bar are ready; tool modules are not wired up yet. Next up: asset browsing and texture replacement.",
  "home.openGame": "Open game folder",
  "home.f1.title": "Asset bundles",
  "home.f1.desc": "Browse the game folder, locate the target bundle and back the original up to the local workspace.",
  "home.f2.title": "Texture replacement",
  "home.f2.desc": "Replace textures by asset name and repack, with batch runs and previews.",
  "home.f3.title": "Models and scripts",
  "home.f3.desc": "Planned panels for mesh injection and runtime hooks, working alongside an external process.",
  "home.planned": "Planned",
  "home.env.title": "Environment",
  "home.env.desc": "This UI runs inside a Tauri 2 + Rust shell with a Vue 3 + TypeScript frontend.",
  "home.env.app": "App version",
  "home.env.frontend": "Frontend",
  "home.env.shell": "Shell",

  "models.title": "Models",
  "models.subtitle": "Pick a character, export, edit, install back",
  "models.search": "Search characters…",
  "models.chars": "{n} characters",
  "models.roster": "{art} art · {skins} skins",
  "models.rosterNoArt": "No art · {skins} skins",
  "models.titleNoArt": "{name} (no art)",
  "models.artboard": "{art} illustrations · {skins} skins",
  "models.noArt": "No art",
  "models.noArtHint": "This character has no illustration; pick a skin mesh below.",
  "models.studio.meshes": "Meshes",
  "models.studio.textures": "Textures",
  "models.studio.verts": "{n} verts · {m} tris",
  "models.studio.loading": "Loading…",
  "models.studio.geometry": "Decoding every mesh…",
  "models.studio.toggle": "Click to show or hide",
  "models.studio.indexing": "Building the texture index (about 10 s the first time)…",
  "models.studio.noMesh": "No meshes for this skin",
  "models.studio.noTex": "No textures found; this skin may share them",
  "models.studio.browser": "3D preview and textures need the desktop app",
  "models.make": "Start building",
  "models.making": "Exporting to the workspace…",
  "models.made": "Exported {meshes} meshes and {textures} textures → {dir}",

  "library.title": "Library",
  "library.subtitle": "Mod management and original-file backups",
  "library.refresh": "Refresh",
  "library.mods": "Installed mods",
  "library.count": "{n}",
  "library.modsDesc":
    "Every install backs the original bundle up before overwriting the game file. Deleting a record does not restore the file — use the backup below.",
  "library.modsEmpty": "No mods installed yet. Pick a character on the Models page to export or install one.",
  "library.restore": "Restore",
  "library.delete": "Delete record",
  "library.backups": "Original-file backups",
  "library.backupsPath": "Backup location:",
  "library.backupsEmpty": "No backups yet. The first install creates one automatically.",
  "library.restoreToGame": "Restore to game",
  "library.restored": "Restored {hash}…",
  "library.deleted": "Mod record deleted (game files unchanged; restore from a backup)",

  "tools.title": "Tools",
  "tools.subtitle": "Textures, models and packing",
  "tools.pending": "{n} modules pending",
  "tools.t1.title": "Texture replacement",
  "tools.t1.desc": "Parse the Texture2D inside a bundle, replace it with a solid colour or an image, and leave everything else untouched.",
  "tools.t2.title": "Bundle inspector",
  "tools.t2.desc": "Inspect the UnityFS header, block and node tables and the asset list.",
  "tools.t3.title": "Runtime injection",
  "tools.t3.desc": "Planned: locate asset load points in the game process with Frida.",
  "tools.t4.title": "Verify and repair",
  "tools.t4.desc": "Planned: diff against backups, restore modified files, check bundle integrity.",
  "tools.planned": "Planned",

  "logs.title": "Logs",
  "logs.subtitle": "Run and diagnostic output",
  "logs.clear": "Clear",
  "logs.export": "Export",
  "logs.banner": "{time}  EXMT Modbox {app} · Tauri {tauri}",
  "logs.ready": "{time}  shell ready — sidebar / titlebar mounted",
  "logs.idle": "{time}  no background tasks yet; tool output will stream here",
  "logs.hint": "// live output and stack traces will appear here once tasks are wired up",

  "settings.title": "Settings",
  "settings.subtitle": "App, paths and permissions",
  "settings.general": "General",
  "settings.language": "Language",
  "settings.languageDesc": "Switch the interface language; takes effect immediately.",
  "settings.appearance": "Appearance",
  "settings.theme": "Theme",
  "settings.themeDesc": "Light, dark, or follow the Windows personalisation setting.",
  "settings.themeSystem": "System",
  "settings.themeLight": "Light",
  "settings.themeDark": "Dark",
  "settings.paths": "Paths and permissions",
  "settings.cache": "Cache folder",
  "settings.cacheDesc": "Temporary packed, extracted and backup files live here.",
  "settings.cacheChange": "Change…",
  "settings.cacheOpen": "Open",
  "settings.cachePick": "Choose a cache folder",
  "settings.cacheFail": "Can't use that folder: {error}",
  "settings.fs": "File system",
  "settings.fsDesc": "Read and write are allowed on any drive and folder so game assets can be patched directly.",
  "settings.net": "Network",
  "settings.netDesc": "Any http / https address is allowed, for updates and remote manifests.",
  "settings.game": "Game folder",
  "settings.gameDesc": "Not set yet; editable here once the library is wired up.",
  "settings.allowed": "Allowed",
  "settings.notSet": "Not set",
  "settings.runtime": "Runtime",
  "settings.envConnected": "Desktop shell connected",
  "settings.envBrowser": "Browser preview",
  "settings.aboutDesc":
    "An offline model toolbox for Girls' Frontline 2: Exilium — pick a character, export .exmtmesh, edit it in Blender, then pack it back into the game.",
  "settings.aboutDocs": "Documentation",
  "settings.aboutGuide": "Model replacement guide",
  "settings.aboutSponsor": "Sponsor and homepage",
};

const dictionaries: Record<Locale, Record<MessageKey, string>> = {
  "zh-CN": zh,
  "en-US": en,
};

function readLocale(): Locale {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "zh-CN" || raw === "en-US") return raw;
  } catch {
    /* localStorage can be unavailable in restricted webviews */
  }
  return "zh-CN";
}

export const locale = ref<Locale>(readLocale());

watch(
  locale,
  (value) => {
    try {
      localStorage.setItem(STORAGE_KEY, value);
    } catch {
      /* ignore */
    }
    document.documentElement.lang = value;
  },
  { immediate: true },
);

/** Translate `key`, replacing `{name}` placeholders from `params`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  let text: string = dictionaries[locale.value][key];
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.split(`{${name}}`).join(String(value));
    }
  }
  return text;
}

export function setLocale(value: Locale) {
  locale.value = value;
}

export function useI18n() {
  return { locale, setLocale, t };
}
