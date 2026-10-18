/** Thin wrappers around the Tauri runtime so the app also runs in a plain browser. */

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function tauriWindow() {
  if (!isTauri) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export async function appVersions(): Promise<{ app: string; tauri: string }> {
  if (!isTauri) return { app: "0.1.0", tauri: "—" };
  try {
    const app = await import("@tauri-apps/api/app");
    const [version, tauri] = await Promise.all([
      app.getVersion(),
      app.getTauriVersion(),
    ]);
    return { app: version, tauri };
  } catch {
    return { app: "0.1.0", tauri: "—" };
  }
}
