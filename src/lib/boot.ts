/**
 * Startup: decide between the setup wizard and the app.
 *
 * Nothing below `bootState === "ready"` may touch the game folder — the roster,
 * the texture index, the clip index and the bone names all come from the
 * analysed cache. That is the whole point of the setup wizard: one scan, then
 * every page is instant.
 */
import { ref } from "vue";
import { cacheState, dropGameCache, loadGameCache, markSetupDone, setupDone } from "./gamecache";
import { hydrateRoster } from "./roster";
import { restoreSelection } from "./selection";
import { checkGamePath, gamePath, refreshBackup } from "./settings";

export type BootState = "loading" | "setup" | "ready" | "error";

export const bootState = ref<BootState>("loading");
export const bootError = ref("");
/** Why the wizard is on screen, so it can open at the right step. */
export const setupReason = ref<"first-run" | "no-game" | "no-cache" | "stale-cache">("first-run");

export async function boot(): Promise<void> {
  bootState.value = "loading";
  bootError.value = "";
  try {
    if (!setupDone.value) {
      setupReason.value = "first-run";
      bootState.value = "setup";
      return;
    }
    if (!gamePath.value) {
      setupReason.value = "no-game";
      bootState.value = "setup";
      return;
    }
    if (!(await checkGamePath(gamePath.value))) {
      setupReason.value = "no-game";
      bootState.value = "setup";
      return;
    }
    const { state } = await cacheState();
    if (state !== "ready") {
      setupReason.value =
        state === "elsewhere" || state === "outdated" ? "stale-cache" : "no-cache";
      bootState.value = "setup";
      return;
    }
    const cache = await loadGameCache();
    hydrateRoster(cache.skins);
    restoreSelection();
    // the backup is what the model page reads; it is not needed for the roster
    void refreshBackup();
    bootState.value = "ready";
  } catch (error) {
    bootError.value = String(error);
    bootState.value = "error";
  }
}

/** Called by the wizard once the analysis finished (or by "skip" in dev). */
export async function finishSetup(): Promise<void> {
  markSetupDone(true);
  await boot();
}

/** Cache folder or game folder changed: the analysis no longer applies. */
export function invalidateSetup(): void {
  dropGameCache();
  markSetupDone(false);
  bootState.value = "setup";
  setupReason.value = "no-cache";
}
