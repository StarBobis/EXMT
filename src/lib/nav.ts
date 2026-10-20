/** Navigation model — single source of truth for the sidebar and the view switch. */
import { ref } from "vue";
import type { MessageKey } from "./i18n";

export type ViewKey = "home" | "models" | "library" | "settings";

/** Shared so pages can hand off to each other (home ⇄ studio). */
export const currentView = ref<ViewKey>("home");

export function navigate(key: ViewKey) {
  currentView.value = key;
}

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
];

export const NAV_FOOTER: NavItem[] = [
  { key: "settings", labelKey: "nav.settings", hintKey: "nav.hint.settings", icon: "\uE713" },
];

export const ALL_NAV_ITEMS: NavItem[] = [...NAV_PRIMARY, ...NAV_FOOTER];
