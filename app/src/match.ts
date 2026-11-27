// Guessing which catalogue product a filament the user named themselves is.

import type { Card } from "./api";

/** The least alike a name and a product may be for the product to be offered. */
const ENOUGH = 0.5;

/** `PLA+ 2.0` and `pla-plus 2.0` are the same words. */
function words(text: string): string[] {
  return text.toLowerCase().replace(/\+/g, " plus ").match(/[a-z0-9]+(?:\.[0-9]+)?/g) ?? [];
}

/** How many of the name's leading words are the brand's; 0 when the name is not this brand's. */
function brandWords(name: string[], brand: string[]): number {
  let same = 0;
  while (same < brand.length && name[same] === brand[same]) same += 1;
  // "eSUN PLA+" is still eSUN 3D's, and "Bambu PLA Basic" Bambu Lab's.
  return same === brand.length || (same > 0 && brand.length > 1) ? same : 0;
}

export interface Guess {
  item: Card;
  /** From 0 to 1: the share of words the name and the product have in common. */
  score: number;
}

/** The products a filament name could be, likeliest first. */
export function guesses(name: string, items: Card[]): Guess[] {
  const named = words(name);
  const found: Guess[] = [];
  for (const item of items) {
    if (item.resin) continue;
    const skip = brandWords(named, words(item.brand));
    if (!skip) continue;
    const mine = new Set(named.slice(skip));
    const theirs = new Set(words(`${item.name} ${item.material}`));
    const shared = [...mine].filter((word) => theirs.has(word)).length;
    const score = shared / (mine.size + theirs.size - shared);
    if (score >= ENOUGH) found.push({ item, score });
  }
  return found.sort((a, b) => b.score - a.score);
}

/** The one product a filament name clearly is, or null when none or several fit equally. */
export function bestGuess(name: string, items: Card[]): Card | null {
  const [first, second] = guesses(name, items);
  return first && (!second || first.score > second.score) ? first.item : null;
}

/** The brand a filament name starts with, as the catalogue writes it. */
export function brandOf(name: string, brands: string[]): string | null {
  const named = words(name);
  let best: string | null = null;
  let most = 0;
  for (const brand of brands) {
    const same = brandWords(named, words(brand));
    if (same > most) [best, most] = [brand, same];
  }
  return best;
}
