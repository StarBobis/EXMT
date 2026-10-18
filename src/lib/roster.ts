/**
 * Character / skin catalogue shared by the home roster and the model studio.
 *
 * `catalog.json` is one row per skin (mesh bundle + parts); `art.json` maps a
 * character key to its illustrations, where `entry` points back at the skin.
 * The home page owns the two lists now, so the data lives here instead of in a
 * view.
 */
import artData from "../data/art.json";
import catalogData from "../data/catalog.json";

export interface CatalogEntry {
  id: string;
  name: string;
  display?: string;
  variant: string;
  bundle: string;
  parts: Record<string, string>;
  part_bundles?: Record<string, string[]>;
  portrait?: { bundle: string; texture: string } | null;
}

export interface ArtItem {
  source: string;
  file: string;
  code: string;
  rarity: string;
  entry: string | null;
  entry_label?: string | null;
  label: string;
  size: number[];
}

export interface CharArt {
  display: string;
  default: ArtItem | null;
  skins: ArtItem[];
}

export interface Character {
  key: string;
  display: string;
  entries: CatalogEntry[];
  portraitEntry: CatalogEntry;
  art: CharArt | null;
}

export interface ArtCard {
  entry: CatalogEntry;
  /** null = this skin has no illustration; the card shows a placeholder */
  art: ArtItem | null;
}

const RARITY = /^(.*?)(SSR|SR|R)(\d*)$/;

/** VepleySSR01 -> Vepley ; VepleyDorm -> Vepley */
function charKeyOf(name: string) {
  if (name.endsWith("Dorm")) return name.slice(0, -4);
  const m = RARITY.exec(name);
  return m ? m[1] : name;
}

export const catalog: CatalogEntry[] = (catalogData as { characters: CatalogEntry[] })
  .characters;

const art = artData as unknown as Record<string, CharArt>;

function buildCharacters(): Character[] {
  const map = new Map<string, Character>();
  for (const entry of catalog) {
    const key = charKeyOf(entry.name);
    let character = map.get(key);
    if (!character) {
      character = {
        key,
        display: entry.display || key,
        entries: [],
        portraitEntry: entry,
        art: art[key] ?? null,
      };
      map.set(key, character);
    }
    character.entries.push(entry);
    // prefer a slg entry, and the entry that owns the default illustration
    const better =
      (entry.variant === "slg" && character.portraitEntry.variant !== "slg") ||
      (character.art?.default?.entry === entry.id &&
        character.art?.default?.entry !== character.portraitEntry.id);
    if (better) {
      character.portraitEntry = entry;
      if (entry.display) character.display = entry.display;
    }
  }
  for (const character of map.values()) {
    character.display = character.display.replace(/\s*·\s*宿舍$/, "");
  }
  const list = [...map.values()];
  list.sort((a, b) => a.display.localeCompare(b.display, "zh-Hans-CN"));
  return list;
}

export const characters: Character[] = buildCharacters();

const entryById = new Map(catalog.map((entry) => [entry.id, entry]));
const characterByEntry = new Map<string, Character>();
for (const character of characters) {
  for (const entry of character.entries) characterByEntry.set(entry.id, character);
}

export function findEntry(id: string | null | undefined): CatalogEntry | null {
  if (!id) return null;
  return entryById.get(id) ?? null;
}

export function characterOfEntry(entry: CatalogEntry | null | undefined): Character | null {
  if (!entry) return null;
  return characterByEntry.get(entry.id) ?? null;
}

/** The skin a character opens with: default illustration, then slg, then first. */
export function preferredEntry(character: Character): CatalogEntry | null {
  return (
    findEntry(character.art?.default?.entry) ??
    character.entries.find((entry) => entry.variant === "slg") ??
    character.entries[0] ??
    null
  );
}

const cardsByCharacter = new WeakMap<Character, ArtCard[]>();

/** Every skin of the character gets a card; skins without art get a placeholder. */
export function artCardsFor(character: Character): ArtCard[] {
  const cached = cardsByCharacter.get(character);
  if (cached) return cached;
  const byEntry = new Map<string, ArtItem>();
  const fallback = character.art?.default ?? null;
  if (fallback?.entry) byEntry.set(fallback.entry, fallback);
  for (const item of character.art?.skins ?? []) {
    if (item.entry) byEntry.set(item.entry, item);
  }
  const cards = character.entries.map((entry) => ({
    entry,
    art: byEntry.get(entry.id) ?? null,
  }));
  const rank = (card: ArtCard) =>
    card.art && card.art === fallback ? 0 : card.entry.variant === "slg" ? 1 : 2;
  cards.sort((a, b) => rank(a) - rank(b));
  cardsByCharacter.set(character, cards);
  return cards;
}

export function label(entry: CatalogEntry): string {
  return entry.display || entry.name;
}

export function portraitUrl(entry: CatalogEntry): string {
  return `portraits/${entry.id}.png`;
}

/** Human label of one skin, straight from its illustration when it has one. */
export function skinLabel(entry: CatalogEntry): string {
  const character = characterOfEntry(entry);
  if (character) {
    const card = artCardsFor(character).find((item) => item.entry.id === entry.id);
    if (card?.art) return card.art.label;
  }
  return label(entry);
}
