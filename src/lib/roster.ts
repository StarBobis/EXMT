/**
 * Character / skin roster, **derived from the analysed game cache**.
 *
 * `catalog.json` used to be the source of truth (240 hand-written rows), which
 * meant a game update needed code changes. Now the rows come from
 * `GameCache/skins.json` (every skin the analysis found) and the bundled
 * catalogue is only a **label dictionary**: Chinese display names, and nothing
 * else. `art.json` maps a character key to its illustrations, where `entry`
 * points back at the skin.
 */
import artData from "../data/art.json";
import catalogData from "../data/catalog.json";
import { shallowRef } from "vue";

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

export const catalog = shallowRef<CatalogEntry[]>([]);

const art = artData as unknown as Record<string, CharArt>;

/** The bundled catalogue is kept for its labels and portraits only. */
interface LabelRow {
  name: string;
  display?: string;
  portrait?: { bundle: string; texture: string } | null;
}
const labelRows = (catalogData as { characters: LabelRow[] }).characters;
const labels = new Map<string, string>(
  labelRows.filter((row) => !!row.display).map((row) => [row.name, row.display as string]),
);
const portraits = new Map<string, { bundle: string; texture: string }>(
  labelRows
    .filter((row) => !!row.portrait)
    .map((row) => [row.name, row.portrait as { bundle: string; texture: string }]),
);

function build(entries: CatalogEntry[]): Character[] {
  const map = new Map<string, Character>();
  for (const entry of entries) {
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

export const characters = shallowRef<Character[]>([]);

const entryById = new Map<string, CatalogEntry>();
const characterByEntry = new Map<string, Character>();
let cardsByCharacter = new WeakMap<Character, ArtCard[]>();

/**
 * Does this analysed skin belong on the home page?
 *
 * The game ships helper meshes that look like skins (`AR`, `AELG`, …: base-body
 * variants with no `slg_` set and no textures). A real skin has a `slg_` part;
 * anything the label dictionary knows about is kept regardless, so a renamed
 * variant never disappears from the roster.
 */
function isCharacterSkin(row: { name: string; parts: Record<string, string> }): boolean {
  if (labels.has(row.name)) return true;
  return Object.keys(row.parts).some((part) => part.startsWith("slg"));
}

/** Replace the roster with what the analysis found (call once, after cache load). */
export function hydrateRoster(
  skins: {
    id?: string;
    name: string;
    variant?: string;
    bundle: string;
    parts: Record<string, string>;
    part_bundles?: Record<string, string[]>;
  }[],
): void {
  const entries: CatalogEntry[] = [];
  for (const row of skins) {
    if (!isCharacterSkin(row)) continue;
    entries.push({
      id: row.id || `c_${row.name}`,
      name: row.name,
      display: labels.get(row.name),
      variant: row.variant || (row.name.toLowerCase().endsWith("dorm") ? "dorm" : "slg"),
      bundle: row.bundle,
      parts: row.parts ?? {},
      part_bundles: row.part_bundles,
      // portraits still come from the bundled dictionary; a skin the analysis
      // found but the dictionary does not know shows its first letter instead
      portrait: portraits.get(row.name) ?? null,
    });
  }
  catalog.value = entries;
  characters.value = build(entries);

  entryById.clear();
  characterByEntry.clear();
  cardsByCharacter = new WeakMap();
  for (const entry of entries) entryById.set(entry.id, entry);
  for (const character of characters.value) {
    for (const entry of character.entries) characterByEntry.set(entry.id, character);
  }
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
