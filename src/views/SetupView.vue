<script setup lang="ts">
/**
 * First-run wizard: language → cache folder → game folder → analysis.
 *
 * The fourth step is the point of the whole thing: until it finishes there is
 * no `GameCache`, so the app would have to scan the game folder on every page —
 * exactly what this replaces. The wizard refuses to continue without it.
 */
import { computed, onMounted, ref } from "vue";
import { locale, setLocale, t, type Locale } from "../lib/i18n";
import {
  analyzeGame,
  cacheState,
  cancelAnalyze,
  isCancelled,
  readAnalyzeProgress,
  verifyCacheFolder,
  type AnalyzeEvent,
  type GameCacheManifest,
} from "../lib/gamecache";
import AnalyzeProgress from "../components/AnalyzeProgress.vue";
import { finishSetup, setupReason } from "../lib/boot";
import {
  cacheFolder,
  chooseCacheFolder,
  chooseGameExe,
  detectGamePath,
  gameBundleDir,
  gameMessage,
  gameReady,
  useCacheFolder,
  useGamePath,
  checkGamePath,
  gamePath,
} from "../lib/settings";

const step = ref(1);
const steps = computed(() => [1, 2, 3, 4]);

// ------------------------------------------------------------------------- //
// step 2: cache folder — proven writable before it is accepted
// ------------------------------------------------------------------------- //
const cacheOk = ref(false);
const cacheError = ref("");
const cacheBusy = ref(false);

async function checkCache(dir = cacheFolder.value): Promise<void> {
  cacheBusy.value = true;
  cacheError.value = "";
  try {
    await verifyCacheFolder(dir);
    cacheOk.value = true;
  } catch (error) {
    cacheOk.value = false;
    cacheError.value = String(error);
  } finally {
    cacheBusy.value = false;
  }
}

async function pickCache(): Promise<void> {
  const picked = await chooseCacheFolder(t("settings.cachePick"));
  if (!picked) return;
  try {
    await useCacheFolder(picked);
    await checkCache();
  } catch (error) {
    cacheOk.value = false;
    cacheError.value = String(error);
  }
}

// ------------------------------------------------------------------------- //
// step 3: game folder
// ------------------------------------------------------------------------- //
const gameBusy = ref(false);

async function pickGame(): Promise<void> {
  const picked = await chooseGameExe(t("settings.gamePick"));
  if (!picked) return;
  await useGamePath(picked);
}

async function detect(): Promise<void> {
  gameBusy.value = true;
  try {
    const hit = await detectGamePath();
    if (hit) await useGamePath(hit);
  } finally {
    gameBusy.value = false;
  }
}

// ------------------------------------------------------------------------- //
// step 4: analysis
// ------------------------------------------------------------------------- //
const analyzed = ref(false);
const analyzeError = ref("");
const progress = ref<AnalyzeEvent | null>(null);
const running = ref(false);
const cancelling = ref(false);
const cancelled = ref(false);
/** seconds since the button was pressed: progress.json only ticks per phase */
const elapsed = ref(0);
let ticker: ReturnType<typeof setInterval> | null = null;

const doneText = ref("");

async function runAnalysis(): Promise<void> {
  running.value = true;
  analyzeError.value = "";
  analyzed.value = false;
  cancelled.value = false;
  cancelling.value = false;
  elapsed.value = 0;
  const started = Date.now();
  ticker = setInterval(() => {
    elapsed.value = Math.round((Date.now() - started) / 1000);
  }, 500);
  progress.value = { state: "running", phase: "names", phases: [], seconds: 0 };
  try {
    const manifest = await analyzeGame({
      onProgress: (event) => {
        progress.value = event;
      },
    });
    analyzed.value = true;
    doneText.value = describe(manifest);
  } catch (error) {
    if (cancelling.value || isCancelled(error)) {
      cancelled.value = true;
      analyzeError.value = "";
    } else {
      analyzeError.value = String(error);
    }
  } finally {
    running.value = false;
    cancelling.value = false;
    if (ticker) clearInterval(ticker);
    ticker = null;
  }
}

async function stopAnalysis(): Promise<void> {
  if (!running.value || cancelling.value) return;
  cancelling.value = true;
  try {
    await cancelAnalyze();
  } catch (error) {
    analyzeError.value = String(error);
    cancelling.value = false;
  }
}

function describe(manifest: GameCacheManifest): string {
  return t("setup.analyzeDone", {
    meshes: manifest.counts.meshes,
    textures: manifest.counts.textures,
    clips: manifest.counts.clips,
    skins: manifest.counts.skins,
    s: manifest.seconds,
  });
}

// ------------------------------------------------------------------------- //
// navigation
// ------------------------------------------------------------------------- //
const canNext = computed(() => {
  if (step.value === 1) return true;
  if (step.value === 2) return cacheOk.value;
  if (step.value === 3) return gameReady.value;
  return analyzed.value;
});

function next(): void {
  if (canNext.value && step.value < 4) step.value += 1;
}

function back(): void {
  if (step.value > 1) step.value -= 1;
}

async function enter(): Promise<void> {
  if (!analyzed.value) return;
  await finishSetup();
}

onMounted(async () => {
  await checkCache();
  // a returning user (cache lost / game moved) keeps the path they already set
  if (gamePath.value) await checkGamePath(gamePath.value);

  // already analysed (an earlier run, or a restart right after it finished):
  // the analysis stays "done" even when the install is not set up, so a lost
  // setting never costs another 6-minute scan
  const { state, manifest } = await cacheState();
  if (state === "ready" && manifest) {
    analyzed.value = true;
    doneText.value = describe(manifest);
  }

  // The install is the one thing `boot()` cannot do without. When it is missing
  // or unverifiable, "进入" goes straight back to this wizard without a spinner
  // or a message, so the page looks frozen. Land on the step that fixes it.
  if (!gamePath.value) {
    step.value = analyzed.value ? 3 : 1;
    return;
  }
  if (!gameReady.value) {
    step.value = 3;
    return;
  }
  if (analyzed.value) {
    step.value = 4;
    return;
  }

  // an interrupted analysis leaves its phases behind: resume on step 4
  const live = await readAnalyzeProgress();
  if (live?.phases?.length) {
    progress.value = live;
    step.value = 4;
  } else if (setupReason.value !== "first-run") {
    step.value = 4;
  }
});

function chooseLanguage(value: Locale): void {
  setLocale(value);
}
</script>

<template>
  <div class="setup">
    <aside class="setup__rail">
      <div class="setup__brand">
        <span class="setup__logo" aria-hidden="true">EX</span>
        <div>
          <div class="setup__name">EXMT Modbox</div>
          <div class="setup__tag">{{ t("app.preview") }}</div>
        </div>
      </div>
      <ol class="setup__steps">
        <li
          v-for="index in steps"
          :key="index"
          class="setup__step"
          :class="{
            'setup__step--active': step === index,
            'setup__step--done': step > index || (index === 4 && analyzed),
            'setup__step--locked': index > step,
          }"
        >
          <span class="setup__step-no">{{ step > index || (index === 4 && analyzed) ? "✓" : index }}</span>
          <span class="setup__step-label">{{ t(`setup.step${index}` as never) }}</span>
        </li>
      </ol>
    </aside>

    <section class="setup__body">
      <h1 class="setup__title">{{ t("setup.title") }}</h1>
      <p class="setup__subtitle">{{ t("setup.subtitle") }}</p>

      <!-- 1 · language -->
      <div v-if="step === 1" class="setup__panel">
        <p class="setup__desc">{{ t("setup.langDesc") }}</p>
        <div class="setup__langs">
          <button
            v-for="item in [{ id: 'zh-CN', label: '简体中文' }, { id: 'en-US', label: 'English' }]"
            :key="item.id"
            type="button"
            class="setup__lang"
            :class="{ 'setup__lang--on': locale === item.id }"
            @click="chooseLanguage(item.id as Locale)"
          >
            {{ item.label }}
          </button>
        </div>
      </div>

      <!-- 2 · cache folder -->
      <div v-else-if="step === 2" class="setup__panel">
        <p class="setup__desc">{{ t("setup.cacheDesc") }}</p>
        <div class="setup__row">
          <code class="setup__path">{{ cacheFolder }}</code>
          <button class="btn" type="button" @click="pickCache">{{ t("settings.cacheChange") }}</button>
          <button class="btn" type="button" :disabled="cacheBusy" @click="checkCache()">
            {{ t("setup.cacheVerify") }}
          </button>
        </div>
        <p v-if="cacheOk" class="setup__ok">✓ {{ t("setup.cacheOk") }}</p>
        <p v-else-if="cacheError" class="setup__err">{{ t("setup.cacheBad", { error: cacheError }) }}</p>
        <p class="setup__hint">{{ t("setup.cacheHint") }}</p>
      </div>

      <!-- 3 · game folder -->
      <div v-else-if="step === 3" class="setup__panel">
        <p class="setup__desc">{{ t("setup.gameDesc") }}</p>
        <div class="setup__row">
          <code class="setup__path">{{ gamePath || t("settings.gameNone") }}</code>
          <button class="btn" type="button" :disabled="gameBusy" @click="detect">
            {{ gameBusy ? t("settings.gameDetecting") : t("setup.gameDetect") }}
          </button>
          <button class="btn" type="button" @click="pickGame">{{ t("setup.gameBrowse") }}</button>
        </div>
        <p v-if="gameReady" class="setup__ok">✓ {{ t("setup.gameOk", { dir: gameBundleDir }) }}</p>
        <p v-else-if="gamePath" class="setup__err">{{ t("setup.gameBad") }}</p>
        <p v-else class="setup__hint">{{ gameMessage }}</p>
      </div>

      <!-- 4 · analysis -->
      <div v-else class="setup__panel">
        <p class="setup__desc">{{ t("setup.analyzeDesc") }}</p>
        <div v-if="!running" class="setup__row">
          <button class="btn btn--accent" type="button" :disabled="!gameReady" @click="runAnalysis">
            {{ analyzed ? t("setup.analyzeAgain") : cancelled ? t("setup.analyzeResume") : t("setup.analyzeStart") }}
          </button>
          <span v-if="cancelled" class="setup__phase">{{ t("setup.analyzeStopped") }}</span>
        </div>
        <AnalyzeProgress
          v-if="running"
          :event="progress"
          :running="running"
          :elapsed="elapsed"
          :cancelling="cancelling"
          @stop="stopAnalysis"
        />
        <p v-if="analyzed" class="setup__ok">✓ {{ doneText }}</p>
        <p v-if="analyzeError" class="setup__err">{{ t("setup.analyzeFail", { error: analyzeError }) }}</p>
      </div>

      <footer class="setup__footer">
        <span class="setup__warn">
          {{ step === 2 && !cacheOk ? t("setup.needCache") : "" }}
          {{ step === 3 && !gameReady ? t("setup.needGame") : "" }}
          {{ step === 4 && !analyzed ? t("setup.needAnalysis") : "" }}
        </span>
        <div class="setup__actions">
          <button v-if="step > 1" class="btn" type="button" @click="back">{{ t("setup.back") }}</button>
          <button v-if="step < 4" class="btn btn--accent" type="button" :disabled="!canNext" @click="next">
            {{ t("setup.next") }}
          </button>
          <button v-else class="btn btn--accent" type="button" :disabled="!analyzed" @click="enter">
            {{ t("setup.enter") }}
          </button>
        </div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.setup {
  display: grid;
  grid-template-columns: 240px 1fr;
  height: 100%;
  background: var(--bg-canvas);
}

.setup__rail {
  display: flex;
  flex-direction: column;
  gap: 28px;
  padding: 22px 16px;
  border-right: 1px solid var(--stroke);
  background: var(--bg-layer);
}

.setup__brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.setup__logo {
  width: 30px;
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background: var(--accent);
  color: var(--accent-contrast);
  font-size: 12px;
  font-weight: 700;
}

.setup__name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
}

.setup__tag {
  font-size: 11px;
  color: var(--text-3);
}

.setup__steps {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.setup__step {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 32px;
  padding: 0 8px;
  border-radius: 6px;
  font-size: 12px;
  color: var(--text-3);
}

.setup__step--active {
  background: var(--fill-selected);
  color: var(--text-1);
  font-weight: 600;
}

.setup__step--done {
  color: var(--text-2);
}

.setup__step-no {
  width: 18px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  border: 1px solid var(--stroke-strong);
  font-size: 10px;
}

.setup__step--active .setup__step-no {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-contrast);
}

.setup__step--done .setup__step-no {
  border-color: var(--accent);
  color: var(--accent);
}

.setup__body {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 28px 32px 16px;
  min-width: 0;
  overflow: auto;
}

.setup__title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  color: var(--text-1);
}

.setup__subtitle {
  margin: 0 0 6px;
  max-width: 760px;
  font-size: 12px;
  line-height: 1.7;
  color: var(--text-2);
}

.setup__panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.setup__desc {
  margin: 0;
  font-size: 12px;
  color: var(--text-2);
}

.setup__row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.setup__path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding: 6px 10px;
  border: 1px solid var(--stroke);
  border-radius: 6px;
  background: var(--fill-subtle-pressed);
  font-size: 12px;
  color: var(--text-1);
}

.setup__langs {
  display: flex;
  gap: 8px;
}

.setup__lang {
  min-width: 150px;
  height: 40px;
  border: 1px solid var(--stroke);
  border-radius: 8px;
  background: var(--fill-subtle-pressed);
  color: var(--text-1);
  font-size: 13px;
  cursor: pointer;
}

.setup__lang--on {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.setup__ok {
  margin: 0;
  font-size: 12px;
  color: var(--ok, #2f9e63);
}

.setup__err {
  margin: 0;
  font-size: 12px;
  color: var(--danger, #d64545);
}

.setup__hint {
  margin: 0;
  font-size: 11px;
  color: var(--text-3);
}

.setup__phase {
  font-size: 12px;
  color: var(--text-2);
}

.setup__elapsed {
  margin-left: 6px;
  color: var(--text-3);
}

.setup__phases {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 2px 0 0;
  padding: 0;
  list-style: none;
}

.setup__phase-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-3);
}

.setup__phase-row--live {
  color: var(--text-1);
}

.setup__phase-row--done {
  color: var(--text-2);
}

.setup__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  border: 1px solid var(--stroke-strong);
}

.setup__phase-row--live .setup__dot {
  background: var(--accent);
  border-color: var(--accent);
}

.setup__phase-row--done .setup__dot {
  background: var(--ok, #2f9e63);
  border-color: transparent;
}

.setup__phase-sec {
  margin-left: auto;
  color: var(--text-3);
}

.setup__footer {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-top: 10px;
  border-top: 1px solid var(--stroke);
}

.setup__warn {
  flex: 1;
  font-size: 11px;
  color: var(--text-3);
}

.setup__actions {
  display: flex;
  gap: 8px;
}
</style>
