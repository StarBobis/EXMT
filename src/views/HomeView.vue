<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch, type CSSProperties } from "vue";
import { t } from "../lib/i18n";
import { navigate } from "../lib/nav";
import {
  artCardsFor,
  characters,
  label,
  portraitUrl,
  type CatalogEntry,
  type Character,
} from "../lib/roster";
import { pickCharacter, selectSkin, selectedChar, selectedSkin } from "../lib/selection";
import { prefetchSkin } from "../lib/preview";

const query = ref("");
const listRef = ref<HTMLElement | null>(null);

/** Keep the highlighted character visible when the saved skin is far down the list. */
async function revealSelected() {
  await nextTick();
  listRef.value
    ?.querySelector<HTMLElement>(".roster__item--active")
    ?.scrollIntoView({ block: "nearest" });
}

onMounted(revealSelected);
watch(selectedChar, revealSelected);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  const all = characters.value;
  if (!q) return all;
  return all.filter((character) => {
    if (character.key.toLowerCase().includes(q)) return true;
    if (character.display.toLowerCase().includes(q)) return true;
    return character.entries.some(
      (entry) =>
        entry.id.toLowerCase().includes(q) || entry.name.toLowerCase().includes(q),
    );
  });
});

const cards = computed(() =>
  selectedChar.value ? artCardsFor(selectedChar.value) : [],
);

function pick(character: Character) {
  pickCharacter(character);
  // warm the skin the studio will open, so entering it is instant
  const entry = selectedSkin.value;
  if (entry) prefetchSkin(entry);
}

function choose(entry: CatalogEntry) {
  selectSkin(entry);
  prefetchSkin(entry);
}

function enterStudio() {
  if (selectedSkin.value) navigate("models");
}

// ------------------------------------------------------------------------- //
// 立绘轮播：当前卡永远在中间，其余按环形距离叠在左右两侧
// ------------------------------------------------------------------------- //
const activeIndex = computed(() => {
  const id = selectedSkin.value?.id;
  const index = cards.value.findIndex((card) => card.entry.id === id);
  return index < 0 ? 0 : index;
});

/** 相对当前卡的环形位置：0 = 中间，±1 = 紧邻左右，依此类推。 */
function slotOf(index: number) {
  const n = cards.value.length;
  if (n < 2) return 0;
  const active = activeIndex.value;
  let distance = index - active;
  if (distance > n / 2) distance -= n;
  else if (distance < -n / 2) distance += n;
  return distance;
}

/** 位置 -> 位移/缩放/倾角；越远越小越暗，超过 3 层藏起来。 */
function slotStyle(index: number): CSSProperties {
  const distance = slotOf(index);
  const depth = Math.min(Math.abs(distance), 4);
  const direction = Math.sign(distance);
  const scale = [1, 0.86, 0.74, 0.64, 0.56][depth];
  const shift = [0, 64, 96, 118, 132][depth];
  const tilt = [0, 10, 16, 20, 22][depth];
  const dx = direction * shift;
  const x = dx < 0 ? `-50% - ${-dx}%` : `-50% + ${dx}%`;
  return {
    transform: `translate(calc(${x}), -50%) rotateY(${-direction * tilt}deg) scale(${scale})`,
    zIndex: String(30 - depth),
    opacity: Math.abs(distance) > 3 ? "0" : Math.abs(distance) === 3 ? "0.48" : "1",
    pointerEvents: Math.abs(distance) > 3 ? "none" : "auto",
  };
}
</script>

<template>
  <section class="page">
    <div class="home">
      <aside class="home__roster">
        <div class="roster__tools">
          <input
            v-model="query"
            class="input roster__search"
            type="search"
            :placeholder="t('roster.search')"
          />
          <span class="chip chip--accent">{{ t("roster.chars", { n: filtered.length }) }}</span>
        </div>
        <div ref="listRef" class="home__list">
          <div class="roster">
            <button
              v-for="character in filtered"
              :key="character.key"
              class="roster__item"
              :class="{ 'roster__item--active': selectedChar?.key === character.key }"
              type="button"
              :title="character.display"
              @mousedown.prevent
              @click="pick(character)"
            >
              <div class="roster__avatar">
                <img
                  v-if="character.portraitEntry.portrait"
                  class="roster__img"
                  :src="portraitUrl(character.portraitEntry)"
                  :alt="character.display"
                  loading="lazy"
                />
                <span v-else class="roster__ph">{{ character.display.slice(0, 1) }}</span>
              </div>
              <div class="roster__name">{{ character.display }}</div>
            </button>
          </div>
        </div>
      </aside>

      <div v-if="selectedChar" class="home__detail">
        <div class="artboard">
          <header class="artboard__head">
            <h2 class="artboard__title">{{ selectedChar.display }}</h2>
            <span class="chip">
              {{
                t("roster.artboard", {
                  art: cards.filter((card) => card.art).length,
                  skins: selectedChar.entries.length,
                })
              }}
            </span>
            <span class="artboard__spacer" />
            <button class="btn btn--accent" type="button" @click="enterStudio">
              <span class="glyph">&#xE72A;</span>
              {{ t("roster.open") }}
            </button>
          </header>

          <div v-if="cards.length" class="deck">
            <div
              v-for="(card, index) in cards"
              :key="card.entry.id"
              class="deck__slot"
              :style="slotStyle(index)"
            >
              <button
                class="deck__card"
                :class="{
                  'deck__card--active': selectedSkin?.id === card.entry.id,
                  'deck__card--empty': !card.art,
                }"
                type="button"
                :title="
                  card.art
                    ? card.art.label
                    : t('roster.titleNoArt', { name: label(card.entry) })
                "
                @mousedown.prevent
                @click="choose(card.entry)"
              >
                <img
                  v-if="card.art"
                  class="deck__img"
                  :src="card.art.file"
                  :alt="card.art.label"
                  loading="lazy"
                />
                <div v-else class="deck__placeholder">
                  <img
                    v-if="card.entry.portrait"
                    class="deck__head"
                    :src="portraitUrl(card.entry)"
                    :alt="label(card.entry)"
                    loading="lazy"
                  />
                  <span v-else class="deck__initial">{{ label(card.entry).slice(0, 1) }}</span>
                  <span class="deck__nopic">{{ t("roster.noArt") }}</span>
                </div>

                <div class="deck__label">
                  <span class="deck__name">
                    {{ card.art ? card.art.label : label(card.entry) }}
                  </span>
                  <span class="deck__code mono">
                    {{ card.art ? card.art.entry_label || card.art.code : card.entry.variant }}
                  </span>
                </div>
              </button>
            </div>
          </div>
          <p v-else class="artboard__empty">{{ t("roster.noArtHint") }}</p>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* ---------- 角色列表 + 立绘卡牌 ---------- */
.home {
  display: grid;
  grid-template-columns: 200px minmax(0, 1fr);
  gap: 14px;
  align-items: stretch;
  min-width: 0;
  height: calc(100vh - var(--titlebar-height) - 50px);
  min-height: 430px;
}

.home__roster {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding-right: 6px;
  border-right: 1px solid var(--stroke, rgba(255, 255, 255, 0.06));
}

.home__list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.home__detail {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.roster__tools {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.roster__search {
  flex: 1;
  min-width: 0;
}

.roster__tools .chip {
  flex: none;
  white-space: nowrap;
}

/* 头像只留两列：一屏能扫到更多角色 */
.roster {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: 6px;
}

.roster__item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 5px 2px 6px;
  border: 1px solid transparent;
  border-radius: 9px;
  background: transparent;
  cursor: pointer;
  transition: 120ms ease;
}

.roster__item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.roster__item--active {
  background: rgba(76, 194, 255, 0.12);
  border-color: rgba(76, 194, 255, 0.55);
}

.roster__avatar {
  width: 44px;
  height: 44px;
  border-radius: 12px;
  overflow: hidden;
  background: linear-gradient(160deg, #24334a, #101724);
  display: flex;
  align-items: center;
  justify-content: center;
}

.roster__img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  object-position: 50% 12%;
}

.roster__ph {
  font-size: 20px;
  font-weight: 600;
  opacity: 0.75;
}

.roster__name {
  font-size: 10.5px;
  line-height: 14px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---------- 立绘卡片 ---------- */
.artboard {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.artboard__head {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 32px;
}

.artboard__title {
  margin: 0;
  font-family: var(--font-display);
  font-size: 18px;
  font-weight: 600;
  line-height: 26px;
}

.artboard__spacer {
  flex: 1;
}

.artboard__empty {
  margin: 6px 0 0;
  font-size: 12px;
  opacity: 0.6;
}

/* 立绘卡牌：一张居中、其余按环形距离往两侧叠；卡面比例锁死成立绘本身的 380:752 */
.deck {
  --deck-h: clamp(380px, 56vh, 600px);
  position: relative;
  height: var(--deck-h);
  margin: auto 0;
  perspective: 1400px;
  perspective-origin: 50% 46%;
}

.deck__slot {
  position: absolute;
  left: 50%;
  top: 50%;
  width: calc(var(--deck-h) * 380 / 752);
  height: var(--deck-h);
  transform-origin: 50% 50%;
  transition:
    transform 340ms cubic-bezier(0.2, 0.7, 0.2, 1),
    opacity 340ms ease;
  will-change: transform, opacity;
  backface-visibility: hidden;
}

.deck__card {
  position: relative;
  display: block;
  width: 100%;
  height: 100%;
  padding: 0;
  border: 1px solid var(--stroke, rgba(255, 255, 255, 0.08));
  border-radius: 12px;
  overflow: hidden;
  background: radial-gradient(120% 70% at 50% 20%, #2a3a55 0%, #131b28 70%, #0d131d 100%);
  cursor: pointer;
  transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
}

.deck__card:hover {
  transform: translateY(-3px);
  border-color: rgba(255, 255, 255, 0.24);
}

.deck__card--active {
  border-color: var(--accent, #4cc2ff);
  box-shadow: 0 0 0 1px var(--accent, #4cc2ff) inset, 0 14px 34px rgba(0, 0, 0, 0.5);
}

.deck__img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  object-position: 50% 100%;
}

/* 没有立绘的皮肤：同一张卡片位，显示头像 + 「无立绘」标记 */
.deck__card--empty {
  background: repeating-linear-gradient(
      135deg,
      rgba(255, 255, 255, 0.022) 0 10px,
      transparent 10px 20px
    ),
    radial-gradient(120% 70% at 50% 25%, #202b3d 0%, #121a26 70%, #0d131d 100%);
}

.deck__placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding-bottom: 30px;
}

.deck__head {
  width: 54%;
  aspect-ratio: 1;
  border-radius: 50%;
  object-fit: cover;
  opacity: 0.55;
  filter: grayscale(0.35);
}

.deck__initial {
  font-size: 30px;
  font-weight: 600;
  opacity: 0.4;
}

.deck__nopic {
  font-size: 10px;
  letter-spacing: 0.08em;
  color: rgba(206, 224, 255, 0.45);
  border: 1px solid rgba(206, 224, 255, 0.18);
  border-radius: 999px;
  padding: 1px 8px;
}

.deck__label {
  position: absolute;
  inset: auto 0 0 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 14px 8px 8px;
  background: linear-gradient(to top, rgba(6, 10, 18, 0.94), rgba(6, 10, 18, 0.6) 55%, transparent);
}

.deck__name {
  font-size: 12px;
  font-weight: 600;
  color: #eef4ff;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.deck__code {
  font-size: 9px;
  color: rgba(214, 230, 255, 0.6);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
