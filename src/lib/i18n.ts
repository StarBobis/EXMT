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
  "gate.title": "先设置游戏路径",
  "gate.body": "模型、资源库、工具都要读游戏资源。到设置里指定一次游戏主程序，程序会把原始模型备份到缓存目录，之后只用这份原始资源。",
  "gate.action": "去设置",
  "nav.home": "首页",
  "nav.models": "模型",
  "nav.library": "资源库",
  "nav.tools": "工具",
  "nav.logs": "日志",
  "nav.settings": "设置",
  "nav.hint.home": "角色与皮肤",
  "nav.hint.models": "子网格、贴图与 3D 预览",
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

  "roster.search": "搜索角色…",
  "roster.chars": "{n} 个角色",
  "roster.titleNoArt": "{name}（无立绘）",
  "roster.artboard": "{art} 张立绘 · {skins} 个皮肤",
  "roster.noArt": "无立绘",
  "roster.noArtHint": "这个角色没有可用的皮肤。",
  "roster.open": "进入模型页",

  "models.changeSkin": "换皮肤",
  "models.studio.meshes": "子网格",
  "models.studio.textures": "贴图",
  "models.studio.counts": "{meshes} 个子网格 · {textures} 张贴图",
  "models.studio.verts": "{n} 顶点 · {m} 面",
  "models.studio.texCount": "{n} 贴图",
  "models.studio.loading": "加载中…",
  "models.studio.geometry": "正在解出全部子网格…",
  "models.studio.toggle": "点一下显示/隐藏",
  "models.studio.indexing": "正在建立贴图索引（首次约 10 秒）…",
  "models.studio.noMesh": "这个皮肤没有可用的子网格",
  "models.studio.noTex": "没有找到贴图，可能与其他皮肤共用",
  "models.studio.browser": "浏览器预览不加载 3D 与贴图，请用桌面版",
  "models.studio.cached": "本地缓存 · {time}",
  "models.studio.pick": "勾选后才会导出",
  "models.studio.selectAll": "全选",
  "models.studio.clearAll": "取消全选",
  "models.studio.selectAllHint": "把所有子网格和贴图都勾上，导出整套",
  "models.studio.pickOne": "至少勾选一个子网格再开始制作",
  "models.studio.pickSubmesh": "点左侧子网格，查看它关联的贴图",
  "models.studio.viewing": "正在看 {name} · {n} 张贴图",
  "models.studio.viewingShared": "共享贴图 · {n} 张",
  "models.studio.noSubmeshTex": "这个子网格没有匹配到贴图",
  "models.studio.shared": "共享贴图",
  "models.studio.sharedHint": "没有归到子网格的贴图（常见于脸、眼睛）",
  "models.studio.texScope": "贴图按名称前缀归到子网格；勾选 = 导出，至少勾一个子网格才能制作",
  "viewer.reset": "重置视角",
  "viewer.hint": "滚轮缩放 · 拖拽旋转 · 双击复位",
  "viewer.zoomIn": "放大",
  "viewer.zoomOut": "缩小",
  "models.make": "开始制作",
  "models.making": "正在导出到工作空间…",
  "models.made": "已导出 {meshes} 个子网格、{textures} 张贴图 → {dir}",

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
  "settings.gameDesc": "填游戏主程序（GF2_Exilium.exe）或安装目录；只认这个路径下的资源，换了就在这里改。",
  "settings.gamePlaceholder": "例如 C:\\GF2Exilium\\GF2 Game\\GF2_Exilium.exe",
  "settings.gameNone": "未设置游戏路径",
  "settings.gameReady": "路径可用，其余功能已解锁",
  "settings.gameBad": "路径无效：找不到 GF2_Exilium_Data\\LocalCache\\Data\\AssetBundles_Windows",
  "settings.gamePick": "选择游戏主程序",
  "settings.gameApply": "应用",
  "settings.gameDetect": "自动查找",
  "settings.gameDetecting": "正在查找…",
  "settings.gameBrowse": "浏览…",
  "settings.gameOpenFolder": "打开文件夹",
  "settings.backupTitle": "原始资源备份",
  "settings.backupDesc": "只备份模型/贴图用到的资源包；模型页只读这份备份，装过 Mod 也不会串味。",
  "settings.backupState": "{n} 个资源包 · {gb} GB",
  "settings.backupEmpty": "还没有备份",
  "settings.backupStart": "备份原始模型",
  "settings.backupRunning": "正在备份…",
  "settings.backupDone": "备份完成：{n} 个资源包，{s} 秒",
  "settings.backupRestore": "恢复原始模型",
  "settings.backupOpen": "打开备份",
  "settings.restoreRunning": "正在恢复…",
  "settings.restoreDone": "已恢复 {n} 个资源包（{s} 秒）",
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
  "gate.title": "Set the game folder first",
  "gate.body": "Models, library and tools all read game assets. Pick the game executable once in Settings; the app backs up the original assets into the cache folder and only uses that copy.",
  "gate.action": "Open settings",

  "nav.home": "Home",
  "nav.models": "Models",
  "nav.library": "Library",
  "nav.tools": "Tools",
  "nav.logs": "Logs",
  "nav.settings": "Settings",
  "nav.hint.home": "Characters and skins",
  "nav.hint.models": "Submeshes, textures and 3D preview",
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

  "roster.search": "Search characters…",
  "roster.chars": "{n} characters",
  "roster.titleNoArt": "{name} (no art)",
  "roster.artboard": "{art} illustrations · {skins} skins",
  "roster.noArt": "No art",
  "roster.noArtHint": "This character has no skins.",
  "roster.open": "Open model page",

  "models.changeSkin": "Change skin",
  "models.studio.meshes": "Submeshes",
  "models.studio.textures": "Textures",
  "models.studio.counts": "{meshes} submeshes · {textures} textures",
  "models.studio.verts": "{n} verts · {m} tris",
  "models.studio.texCount": "{n} textures",
  "models.studio.loading": "Loading…",
  "models.studio.geometry": "Decoding every submesh…",
  "models.studio.toggle": "Click to show or hide",
  "models.studio.indexing": "Building the texture index (about 10 s the first time)…",
  "models.studio.noMesh": "No submeshes for this skin",
  "models.studio.noTex": "No textures found; this skin may share them",
  "models.studio.browser": "3D preview and textures need the desktop app",
  "models.studio.cached": "Local cache · {time}",
  "models.studio.pick": "Only picked assets are exported",
  "models.studio.selectAll": "Select all",
  "models.studio.clearAll": "Clear all",
  "models.studio.selectAllHint": "Pick every submesh and texture for a full export",
  "models.studio.pickOne": "Pick at least one submesh before building",
  "models.studio.pickSubmesh": "Click a submesh on the left to see its textures",
  "models.studio.viewing": "Viewing {name} · {n} textures",
  "models.studio.viewingShared": "Shared textures · {n}",
  "models.studio.noSubmeshTex": "No textures matched this submesh",
  "models.studio.shared": "Shared textures",
  "models.studio.sharedHint": "Textures not tied to a submesh (often face and eyes)",
  "models.studio.texScope": "Grouped by name prefix; picked = exported; pick at least one submesh to build",
  "viewer.reset": "Reset view",
  "viewer.hint": "Wheel to zoom · drag to orbit · double-click to reset",
  "viewer.zoomIn": "Zoom in",
  "viewer.zoomOut": "Zoom out",
  "models.make": "Start building",
  "models.making": "Exporting to the workspace…",
  "models.made": "Exported {meshes} submeshes and {textures} textures → {dir}",

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
  "settings.gameDesc": "Point at GF2_Exilium.exe (or its install folder); everything reads from here.",
  "settings.gamePlaceholder": "e.g. C:\\GF2Exilium\\GF2 Game\\GF2_Exilium.exe",
  "settings.gameNone": "No game folder set",
  "settings.gameReady": "Path OK — other pages are unlocked",
  "settings.gameBad": "Invalid path: GF2_Exilium_Data\\LocalCache\\Data\\AssetBundles_Windows not found",
  "settings.gamePick": "Choose the game executable",
  "settings.gameApply": "Apply",
  "settings.gameDetect": "Auto-detect",
  "settings.gameDetecting": "Searching…",
  "settings.gameBrowse": "Browse…",
  "settings.gameOpenFolder": "Open folder",
  "settings.backupTitle": "Original asset backup",
  "settings.backupDesc": "Only the bundles used by models/textures; the models page reads this copy, so an old mod can't leak in.",
  "settings.backupState": "{n} bundles · {gb} GB",
  "settings.backupEmpty": "Not backed up yet",
  "settings.backupStart": "Back up originals",
  "settings.backupRunning": "Backing up…",
  "settings.backupDone": "Backup done: {n} bundles in {s}s",
  "settings.backupRestore": "Restore originals",
  "settings.backupOpen": "Open backup",
  "settings.restoreRunning": "Restoring…",
  "settings.restoreDone": "Restored {n} bundles ({s}s)",
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
