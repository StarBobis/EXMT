<script setup lang="ts">
/**
 * Live GameCache analysis card: overall bar, scanned-file counters, the file or
 * skin currently being decoded, an ETA and a stop button. Used by the first-run
 * wizard and by Settings so a six-minute wait never looks frozen.
 */
import { computed } from "vue";
import { t } from "../lib/i18n";
import type { AnalyzeEvent } from "../lib/gamecache";

const props = defineProps<{
  event: AnalyzeEvent | null;
  running: boolean;
  elapsed: number;
  cancelling?: boolean;
  /** All phases finished: show every row as done. */
  complete?: boolean;
}>();

const emit = defineEmits<{ stop: [] }>();

const PHASES = ["names", "clips", "skins", "packs", "bones"] as const;
const WEIGHTS: Record<string, number> = {
  names: 0.25,
  clips: 0.005,
  skins: 0.005,
  packs: 0.47,
  bones: 0.27,
};

const phase = computed(() => props.event?.phase ?? (props.running ? "names" : ""));
const live = computed(() => props.event?.live ?? null);

const phaseLabel = computed(() =>
  phase.value ? t(`setup.phase.${phase.value}` as never) : props.complete ? t("setup.analyzeFinished") : "",
);

const donePhases = computed(() => new Set((props.event?.phases ?? []).map((row) => row.phase)));

function phaseFraction(name: string): number {
  if (props.complete || donePhases.value.has(name)) return 1;
  if (phase.value !== name) return 0;
  const done = live.value?.done ?? 0;
  const total = live.value?.total ?? 0;
  if (!total) return 0;
  return Math.min(1, done / total);
}

const exactPercent = computed(() => {
  let value = 0;
  for (const name of PHASES) value += WEIGHTS[name] * phaseFraction(name);
  if (!props.running && !props.complete) return 0;
  return Math.max(0, Math.min(100, value * 100));
});

const percent = computed(() => Math.round(exactPercent.value));
const percentText = computed(() =>
  exactPercent.value > 0 && exactPercent.value < 10
    ? exactPercent.value.toFixed(1)
    : String(Math.round(exactPercent.value)),
);

const detail = computed(() => {
  if (!props.running && !props.complete) return "";
  const done = live.value?.done;
  const total = live.value?.total;
  if (phase.value === "names" || phase.value === "bones") {
    if (done == null || !total) return t("setup.scanStarting");
    return t("setup.scanFiles", { done, total });
  }
  if (phase.value === "packs") {
    if (done == null || !total) return t("setup.scanStarting");
    return t("setup.scanPacks", { done, total });
  }
  if (props.complete) return t("setup.analyzeFinished");
  return t("setup.phaseWorking");
});

const current = computed(() => live.value?.current ?? "");
const filesTotal = computed(() => props.event?.files_total ?? 0);
const elapsedText = computed(() => t("setup.elapsed", { time: format(props.elapsed) }));

const eta = computed(() => {
  if (!props.running || props.elapsed < 3) return "";
  // The backend knows the current phase's bytes and gives a much steadier
  // estimate than the weighted overall bar does in the first minute.
  const phaseEta = live.value?.eta_seconds;
  if (phaseEta != null && phaseEta > 0) {
    return t("setup.phaseEta", { time: format(phaseEta) });
  }
  if (exactPercent.value < 8) return "";
  const total = props.elapsed / (exactPercent.value / 100);
  return t("setup.eta", { time: format(Math.max(1, Math.round(total - props.elapsed))) });
});

function format(seconds: number): string {
  const whole = Math.max(0, Math.round(seconds));
  const minutes = Math.floor(whole / 60);
  const rest = whole % 60;
  return minutes ? `${minutes}:${String(rest).padStart(2, "0")}` : `${rest}s`;
}

function phaseDetail(name: string): string {
  if (props.complete || donePhases.value.has(name)) {
    const row = (props.event?.phases ?? []).find((item) => item.phase === name);
    return row ? format(row.seconds) : "✓";
  }
  if (phase.value !== name) return "";
  const done = live.value?.done;
  const total = live.value?.total;
  if (done != null && total) return `${done}/${total}`;
  return t("setup.phaseWorking");
}

function stop(): void {
  if (!props.cancelling) emit("stop");
}
</script>

<template>
  <section class="ap" :class="{ 'ap--idle': !running }">
    <header class="ap__head">
      <div class="ap__heading">
        <span class="ap__eyebrow">{{ running ? t("setup.analyzing") : t("setup.analyzeTitle") }}</span>
        <span class="ap__title">{{ phaseLabel }}</span>
      </div>
      <span class="ap__percent">{{ percentText }}%</span>
    </header>

    <div class="ap__bar" role="progressbar" :aria-valuenow="percent" aria-valuemin="0" aria-valuemax="100">
      <span class="ap__fill" :style="{ width: `${exactPercent}%` }" />
    </div>

    <div v-if="filesTotal" class="ap__total">
      {{ t("setup.totalFiles", { count: filesTotal.toLocaleString() }) }}
    </div>

    <div class="ap__meta">
      <span>{{ detail }}</span>
      <span v-if="running" class="ap__time">
        {{ elapsedText }}
        <template v-if="eta"> · {{ eta }}</template>
      </span>
    </div>

    <div v-if="running && current" class="ap__current" :title="current">
      <span class="ap__current-label">
        {{ phase === "packs" ? t("setup.currentSkin") : t("setup.currentFile") }}
      </span>
      <code>{{ current }}</code>
    </div>

    <ol class="ap__phases">
      <li
        v-for="name in PHASES"
        :key="name"
        class="ap__phase"
        :class="{
          'ap__phase--done': complete || donePhases.has(name),
          'ap__phase--live': phase === name && !complete,
        }"
      >
        <span class="ap__dot" aria-hidden="true" />
        <span class="ap__phase-name">{{ t(`setup.phase.${name}` as never) }}</span>
        <span class="ap__phase-detail">{{ phaseDetail(name) }}</span>
      </li>
    </ol>

    <footer v-if="running" class="ap__foot">
      <span class="ap__hint">{{ t("setup.stopHint") }}</span>
      <button class="btn ap__stop" type="button" :disabled="cancelling" @click="stop">
        {{ cancelling ? t("setup.stopping") : t("setup.stop") }}
      </button>
    </footer>
  </section>
</template>

<style scoped>
.ap {
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding: 14px 16px;
  border: 1px solid var(--stroke);
  border-radius: 10px;
  background: var(--bg-layer);
  max-width: 680px;
}

.ap--idle {
  opacity: 0.75;
}

.ap__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}

.ap__heading {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.ap__eyebrow {
  font-size: 10px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
}

.ap__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
}

.ap__percent {
  font-size: 18px;
  font-variant-numeric: tabular-nums;
  font-weight: 600;
  color: var(--accent);
}

.ap__bar {
  height: 8px;
  overflow: hidden;
  border-radius: 999px;
  background: var(--fill-subtle-pressed);
}

.ap__fill {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--accent), var(--accent-hover));
  transition: width 240ms ease;
}

.ap__total {
  margin-top: -2px;
  font-size: 10px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}

.ap__meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 11px;
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
}

.ap__time {
  color: var(--text-3);
  white-space: nowrap;
}

.ap__current {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 5px 8px;
  border-radius: 6px;
  background: var(--fill-subtle-pressed);
  font-size: 11px;
  color: var(--text-2);
}

.ap__current-label {
  flex: none;
  color: var(--text-3);
}

.ap__current code {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}

.ap__phases {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 2px 0 0;
  padding: 0;
  list-style: none;
}

.ap__phase {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-3);
}

.ap__phase--live {
  color: var(--text-1);
}

.ap__phase--done {
  color: var(--text-2);
}

.ap__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  border: 1px solid var(--stroke-strong);
}

.ap__phase--live .ap__dot {
  background: var(--accent);
  border-color: var(--accent);
}

.ap__phase--done .ap__dot {
  background: var(--ok, #2f9e63);
  border-color: transparent;
}

.ap__phase-name {
  min-width: 0;
}

.ap__phase-detail {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
  color: var(--text-3);
}

.ap__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 2px;
  padding-top: 8px;
  border-top: 1px solid var(--stroke);
}

.ap__hint {
  flex: 1;
  font-size: 10px;
  line-height: 1.5;
  color: var(--text-3);
}

.ap__stop {
  flex: none;
  color: var(--danger, #d64545);
  border-color: color-mix(in srgb, var(--danger, #d64545) 40%, transparent);
}
</style>
