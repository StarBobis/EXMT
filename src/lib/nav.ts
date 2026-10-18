/** Navigation model — single source of truth for the sidebar and the view switch. */
import type { MessageKey } from "./i18n";

export type ViewKey = "home" | "models" | "library" | "tools" | "logs" | "settings";

export interface NavItem {
  key: ViewKey;
  /** i18n key for the label shown in the sidebar. */
  labelKey: MessageKey;
  /** i18n key for the one-line description used under the page title. */
  hintKey: MessageKey;
  /** Segoe Fluent Icons code point. */
  icon: string;
}

export const NAV_PRIMARY: NavItem[] = [
  { key: "home", labelKey: "nav.home", hintKey: "nav.hint.home", icon: "\uE80F" },
  { key: "models", labelKey: "nav.models", hintKey: "nav.hint.models", icon: "\uE8F1" },
  { key: "library", labelKey: "nav.library", hintKey: "nav.hint.library", icon: "\uE8B7" },
  { key: "tools", labelKey: "nav.tools", hintKey: "nav.hint.tools", icon: "\uE90F" },
  { key: "logs", labelKey: "nav.logs", hintKey: "nav.hint.logs", icon: "\uE8A5" },
];

export const NAV_FOOTER: NavItem[] = [
  { key: "settings", labelKey: "nav.settings", hintKey: "nav.hint.settings", icon: "\uE713" },
];

export const ALL_NAV_ITEMS: NavItem[] = [...NAV_PRIMARY, ...NAV_FOOTER];
