import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowsClockwise, Check, MagnifyingGlass } from "@phosphor-icons/react";
import { api, type BarcodeMatch, type Card } from "./api";
import { useNearEnd } from "./useNearEnd";

/** Cards drawn at first, and added each time the end of the list comes into view. */
const PAGE = 60;
/** Materials offered as one-click filters; the rest are in the "More" menu. */
const QUICK_MATERIALS = 6;
/** A retail barcode is 8 to 14 digits. */
const BARCODE = /^\d{8,14}$/;

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
      if (!words.length) return true;
      const hay = `${item.brand} ${item.name} ${item.material}`.toLowerCase();
      return words.every((word) => hay.includes(word));
    });
  }, [items, ofClass, query, material, isBarcode, scanned, code]);

  useEffect(() => setLimit(PAGE), [query, material, resin]);
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
          placeholder="Search brand, name or material, or scan a barcode"
          autoComplete="off"
          spellCheck={false}
        />
      </div>

      <div className="filters">
        <div className="segmented" role="group" aria-label="Material class">
          <button type="button" aria-pressed={!resin} onClick={() => (setResin(false), setMaterial(null))}>
            Filament
          </button>
          <button type="button" aria-pressed={resin} onClick={() => (setResin(true), setMaterial(null))}>
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
                <span className="mono">{item.material}</span>
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
