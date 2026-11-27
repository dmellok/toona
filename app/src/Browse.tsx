import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowsClockwise, Check, MagnifyingGlass, Plus, X } from "@phosphor-icons/react";
import { api, type BarcodeMatch, type Card } from "./api";
import { useNearEnd } from "./useNearEnd";

/** Cards drawn at first, and added each time the end of the list comes into view. */
const PAGE = 60;
/** Materials offered as one-click filters; the rest are in the "More" menu. */
const QUICK_MATERIALS = 6;
/** A retail barcode is 8 to 14 digits. */
const BARCODE = /^\d{8,14}$/;

/** Brands offered as tags while typing. */
const SUGGESTIONS = 5;

const plain = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, "");

/** How many single-letter edits turn `a` into `b`, giving up past `most`. */
function distance(a: string, b: string, most: number): number {
  if (Math.abs(a.length - b.length) > most) return most + 1;
  let row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const next = [i];
    for (let j = 1; j <= b.length; j++) {
      next[j] = Math.min(row[j] + 1, next[j - 1] + 1, row[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    }
    row = next;
  }
  return row[b.length];
}

/** How well typed text names a brand: 0 not at all, higher is closer. Forgives a slip or two. */
function brandScore(typed: string, brand: string): number {
  const name = plain(brand);
  if (name.startsWith(typed)) return 4;
  if (brand.toLowerCase().split(/[^a-z0-9]+/).some((word) => word.startsWith(typed))) return 3;
  if (typed.length >= 4 && distance(typed, name.slice(0, typed.length), 1) <= 1) return 2;
  if (typed.length >= 5 && distance(typed, name, 2) <= 2) return 1;
  return 0;
}

/** A light tint per family of material, so neighbours of different kinds read apart. */
export function materialTone(material: string): string {
  const name = material.toUpperCase();
  if (name.startsWith("PLA") || name === "PHA") return "pla";
  if (["PETG", "PET", "PCTG", "CPE"].some((kind) => name.startsWith(kind))) return "petg";
  if (name.startsWith("ABS") || name.startsWith("ASA") || name === "HIPS") return "abs";
  if (["TPU", "TPE", "TPC", "PEBA"].some((kind) => name.startsWith(kind))) return "tpu";
  if (["PA", "PC", "PP", "PEEK", "PEI", "PEKK", "PPS"].some((kind) => name.startsWith(kind))) return "eng";
  return "other";
}

interface Props {
  items: Card[];
  loading: boolean;
  loadError: string | null;
  retry: () => void;
  selectedKey: string | null;
  onSelect: (key: string) => void;
  onBarcode: (key: string, variant: string) => void;
  advanced: boolean;
  version: string | null;
  warning: string | null;
  refresh: () => void;
}

export function Browse(props: Props) {
  const { items, selectedKey, onSelect, onBarcode } = props;
  const [query, setQuery] = useState("");
  const [resin, setResin] = useState(false);
  const [material, setMaterial] = useState<string | null>(null);
  /** Brands picked as tags; a filament must be from one of them. */
  const [brands, setBrands] = useState<string[]>([]);
  const [limit, setLimit] = useState(PAGE);
  const [scanned, setScanned] = useState<{ code: string; matches: BarcodeMatch[] } | null>(null);

  const ofClass = useMemo(() => items.filter((item) => item.resin === resin), [items, resin]);

  const materials = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of ofClass) counts.set(item.material, (counts.get(item.material) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([name]) => name);
  }, [ofClass]);
  const quick = materials.slice(0, QUICK_MATERIALS);
  const more = materials.slice(QUICK_MATERIALS).sort((a, b) => a.localeCompare(b));

  const brandNames = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of ofClass) counts.set(item.brand, (counts.get(item.brand) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([name]) => name);
  }, [ofClass]);

  // Brands the typed text could mean: the whole of it, or just its first word.
  const suggestions = useMemo(() => {
    const first = query.trim().split(/\s+/)[0] ?? "";
    const out: { brand: string; typed: string; score: number }[] = [];
    for (const typed of new Set([query.trim(), first])) {
      const key = plain(typed);
      // A material's name is a filter of its own, not a misspelt brand.
      if (key.length < 2 || /^\d+$/.test(key) || materials.some((name) => plain(name) === key)) continue;
      for (const brand of brandNames) {
        const score = brandScore(key, brand);
        if (score && !brands.includes(brand) && !out.some((s) => s.brand === brand)) out.push({ brand, typed, score });
      }
    }
    return out.sort((a, b) => b.score - a.score).slice(0, SUGGESTIONS);
  }, [query, brandNames, brands, materials]);

  const addBrand = (suggestion: { brand: string; typed: string }) => {
    setBrands((current) => [...current, suggestion.brand]);
    // What was typed to find the brand has done its job.
    setQuery((current) => current.trim().slice(suggestion.typed.length).trim());
  };

  const code = query.replace(/[\s-]/g, "");
  const isBarcode = BARCODE.test(code);

  // A barcode names one colour of one filament: look it up and open it.
  useEffect(() => {
    if (!isBarcode) {
      setScanned(null);
      return;
    }
    let current = true;
    api
      .barcode(code)
      .then((matches) => {
        if (!current) return;
        setScanned({ code, matches });
        if (matches.length === 1) onBarcode(matches[0].key, matches[0].variant);
      })
      .catch(() => current && setScanned({ code, matches: [] }));
    return () => {
      current = false;
    };
  }, [code, isBarcode, onBarcode]);

  const matches = useMemo(() => {
    if (isBarcode) {
      const keys = new Set(scanned?.code === code ? scanned.matches.map((m) => m.key) : []);
      return items.filter((item) => keys.has(item.key));
    }
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return ofClass.filter((item) => {
      if (material && item.material !== material) return false;
      if (brands.length && !brands.includes(item.brand)) return false;
      if (!words.length) return true;
      const hay = `${item.brand} ${item.name} ${item.material}`.toLowerCase();
      return words.every((word) => hay.includes(word));
    });
  }, [items, ofClass, query, material, brands, isBarcode, scanned, code]);

  useEffect(() => setLimit(PAGE), [query, material, resin, brands]);
  const showMore = useCallback(() => setLimit((current) => current + PAGE), []);
  const endRef = useNearEnd(showMore);

  const noun = resin ? "resin" : "filament";
  const count = matches.length === 1 ? `1 ${noun}` : `${matches.length.toLocaleString()} ${noun}s`;

  let empty: { title: string; body: string } | null = null;
  if (props.loadError) {
    empty = { title: "The catalogue didn't load", body: props.loadError };
  } else if (!props.loading && matches.length === 0) {
    if (isBarcode) {
      empty =
        scanned?.code === code
          ? { title: "That barcode isn't in the catalogue", body: "Search by brand and name instead." }
          : null;
    } else if (resin && ofClass.length === 0) {
      empty = {
        title: "No resins yet",
        body: "The catalogue lists no resins so far. They will appear here when it does.",
      };
    } else {
      empty = { title: `No ${noun}s match`, body: "Try fewer words or another material." };
    }
  }

  return (
    <section className="results">
      <div className="search">
        <label htmlFor="search" className="visually-hidden">
          Search filaments
        </label>
        <MagnifyingGlass size={22} aria-hidden />
        <input
          id="search"
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && suggestions[0]?.score >= 3 && suggestions[0].typed === query.trim()) addBrand(suggestions[0]);
            if (event.key === "Backspace" && query === "" && brands.length) setBrands(brands.slice(0, -1));
          }}
          placeholder="Search brand, name or material, or scan a barcode"
          autoComplete="off"
          spellCheck={false}
        />
      </div>

      {(brands.length > 0 || (suggestions.length > 0 && !isBarcode)) && (
        <div className="brand-row">
          {brands.map((brand) => (
            <button
              key={brand}
              type="button"
              className="brand-tag on"
              aria-label={`Remove the brand ${brand}`}
              onClick={() => setBrands(brands.filter((other) => other !== brand))}
            >
              <span>{brand}</span>
              <X size={14} aria-hidden />
            </button>
          ))}
          {!isBarcode && suggestions.length > 0 && <span className="muted small">{brands.length ? "Add a brand:" : "Brand:"}</span>}
          {!isBarcode &&
            suggestions.map((suggestion) => (
              <button key={suggestion.brand} type="button" className="brand-tag" onClick={() => addBrand(suggestion)}>
                <Plus size={14} aria-hidden />
                <span>{suggestion.brand}</span>
              </button>
            ))}
        </div>
      )}

      <div className="filters">
        <div className="segmented" role="group" aria-label="Material class">
          <button type="button" aria-pressed={!resin} onClick={() => (setResin(false), setMaterial(null), setBrands([]))}>
            Filament
          </button>
          <button type="button" aria-pressed={resin} onClick={() => (setResin(true), setMaterial(null), setBrands([]))}>
            Resin
          </button>
        </div>
        {quick.length > 0 && (
          <button type="button" className="chip" aria-pressed={material === null} onClick={() => setMaterial(null)}>
            All
          </button>
        )}
        {quick.map((name) => (
          <button
            key={name}
            type="button"
            className="chip"
            aria-pressed={material === name}
            onClick={() => setMaterial(name)}
          >
            {name}
          </button>
        ))}
        {more.length > 0 && (
          <select
            className={`chip${material && more.includes(material) ? " on" : ""}`}
            aria-label="More materials"
            value={material && more.includes(material) ? material : ""}
            onChange={(event) => setMaterial(event.target.value || null)}
          >
            <option value="">More</option>
            {more.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        )}
        <div className="spacer" />
        {!props.loading && !props.loadError && <div className="muted">{count}</div>}
      </div>

      {empty && (
        <div className="empty">
          <div className="title">{empty.title}</div>
          <div>{empty.body}</div>
          {props.loadError && (
            <button type="button" className="pill" onClick={props.retry}>
              Try again
            </button>
          )}
        </div>
      )}

      <div className="grid">
        {props.loading && Array.from({ length: 12 }, (_, index) => <div key={index} className="skeleton" />)}
        {matches.slice(0, limit).map((item) => (
          <button
            key={item.key}
            type="button"
            className="card"
            aria-pressed={item.key === selectedKey}
            onClick={() => onSelect(item.key)}
          >
            <div className="stripes" aria-hidden>
              {item.stripes.map((hex) => (
                <span key={hex} style={{ background: hex }} />
              ))}
            </div>
            <div className="card-body">
              <div className="eyebrow">{item.brand}</div>
              <div className="card-name">{item.name}</div>
              <div className="card-meta">
                <span className={`mono material ${materialTone(item.material)}`}>{item.material}</span>
                <span>{item.colours === 0 ? "No colours listed" : item.colours === 1 ? "1 colour" : `${item.colours} colours`}</span>
                <span className="spacer" />
                {item.added && (
                  <span className="added-mark">
                    <Check size={16} aria-hidden />
                    <span>Added</span>
                  </span>
                )}
              </div>
            </div>
          </button>
        ))}
      </div>
      {matches.length > limit && <div ref={endRef} style={{ height: 1 }} />}

      {(props.warning || props.advanced) && props.version && (
        <div className="footer-note">
          <span>Catalogue {props.version}, from the Open Filament Database.</span>
          {props.warning && <span>{props.warning}</span>}
          {props.advanced && (
            <button type="button" onClick={props.refresh}>
              <ArrowsClockwise size={14} aria-hidden /> Check for a newer one
            </button>
          )}
        </div>
      )}
    </section>
  );
}
