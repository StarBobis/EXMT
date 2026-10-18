import { computed, ref, watch } from "vue";

export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "exmt.theme";

function readMode(): ThemeMode {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    /* localStorage can be unavailable in restricted webviews */
  }
  return "system";
}

const media =
  typeof window !== "undefined"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

const mode = ref<ThemeMode>(readMode());
const systemDark = ref<boolean>(media?.matches ?? false);

media?.addEventListener?.("change", (event) => {
  systemDark.value = event.matches;
});

const resolved = computed<ResolvedTheme>(() =>
  mode.value === "system" ? (systemDark.value ? "dark" : "light") : mode.value,
);

function apply(theme: ResolvedTheme) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

watch(resolved, apply, { immediate: true });

watch(mode, (value) => {
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    /* ignore */
  }
});

export function useTheme() {
  return {
    mode,
    resolved,
    setMode(value: ThemeMode) {
      mode.value = value;
    },
  };
}
