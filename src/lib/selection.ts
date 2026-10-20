/**
 * The one selected skin, shared by the home roster and the model studio.
 *
 * The home page is the only place that picks a character or illustration; the
 * model page reads the same selection and reloads when it is activated. The
 * choice survives restarts through `exmt.skin`.
 */
import { ref } from "vue";
import {
  characterOfEntry,
  characters,
  findEntry,
  preferredEntry,
  type CatalogEntry,
  type Character,
} from "./roster";

const STORAGE_KEY = "exmt.skin";

function readSavedSkin(): CatalogEntry | null {
  try {
    return findEntry(localStorage.getItem(STORAGE_KEY));
  } catch {
    return null;
  }
}

export const selectedChar = ref<Character | null>(null);
export const selectedSkin = ref<CatalogEntry | null>(null);

/**
 * Pick the saved skin (or the first character) **after** the roster is known.
 *
 * The roster comes from the analysed cache now, so nothing can be selected at
 * module load time: `hydrateRoster()` runs first, then this.
 */
export function restoreSelection(): void {
  if (selectedSkin.value) return;
  const saved = readSavedSkin();
  const fallback = characters.value[0] ?? null;
  if (saved) {
    selectedChar.value = characterOfEntry(saved) ?? fallback;
    selectedSkin.value = saved;
    return;
  }
  selectedChar.value = fallback;
  selectedSkin.value = fallback ? preferredEntry(fallback) : null;
}

function persist() {
  try {
    if (selectedSkin.value) localStorage.setItem(STORAGE_KEY, selectedSkin.value.id);
  } catch {
    /* localStorage can be unavailable in restricted webviews */
  }
}

/** Jump to a character and its default skin. */
export function pickCharacter(character: Character) {
  selectedChar.value = character;
  selectedSkin.value = preferredEntry(character);
  persist();
}

/** Bring one illustration to the centre. */
export function selectSkin(entry: CatalogEntry) {
  const character = characterOfEntry(entry);
  if (character) selectedChar.value = character;
  selectedSkin.value = entry;
  persist();
}
