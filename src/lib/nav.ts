/** Navigation model — single source of truth for the sidebar and the view switch. */

export type ViewKey = "home" | "models" | "library" | "tools" | "logs" | "settings";

export interface NavItem {
  key: ViewKey;
  /** Label shown in the sidebar. */
  label: string;
  /** Segoe Fluent Icons code point. */
  icon: string;
  /** Short description used by the page header when the view has no custom one. */
  hint: string;
}

export const NAV_PRIMARY: NavItem[] = [
  { key: "home", label: "首页", icon: "\uE80F", hint: "概览与最近操作" },
  { key: "models", label: "模型", icon: "\uE8F1", hint: "角色预设与模型替换" },
  { key: "library", label: "资源库", icon: "\uE8B7", hint: "游戏资源与 Mod 包" },
  { key: "tools", label: "工具", icon: "\uE90F", hint: "贴图、模型与打包" },
  { key: "logs", label: "日志", icon: "\uE8A5", hint: "运行与诊断输出" },
];

export const NAV_FOOTER: NavItem[] = [
  { key: "settings", label: "设置", icon: "\uE713", hint: "应用、路径与权限" },
];

export const ALL_NAV_ITEMS: NavItem[] = [...NAV_PRIMARY, ...NAV_FOOTER];
