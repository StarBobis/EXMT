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

const saved = readSavedSkin();

function initialCharacter(entry: CatalogEntry | null): Character | null {
  if (entry) {
    const character = characterOfEntry(entry);
    if (character) return character;
  }
  return characters[0] ?? null;
}

export const selectedChar = ref<Character | null>(initialCharacter(saved));
export const selectedSkin = ref<CatalogEntry | null>(
  saved ?? (selectedChar.value ? preferredEntry(selectedChar.value) : null),
);

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
