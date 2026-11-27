import {
  CaretDown,
  CheckCircle,
  CloudCheck,
  Copy,
  LinkSimple,
  MagnifyingGlass,
  Plus,
  ShareNetwork,
  Trash,
  UsersThree,
  Warning,
  Wrench,
} from "@phosphor-icons/react";
import { useMemo, useState } from "react";
import type { Added, Card, InStudio, Own, Printer } from "./api";
import { materialTone } from "./Browse";
import { Changes, withRange } from "./Changes";
import { dotStyle } from "./colour";
import { bestGuess, brandOf } from "./match";

/** What the person is looking at in the list. Kept by the app between visits and sessions. */
export interface StudioFilters {
  query: string;
  /** Nozzle sizes to show; none ticked means all of them. */
  nozzles: string[];
  /** Materials to show; none ticked means all of them. */
  materials: string[];
  source: "all" | "toona" | "own";
  /** Also show filaments for printers other than the one chosen in the header. */
  allPrinters: boolean;
  /** The imported profiles are unfolded. */
  imported: boolean;
  /** Brands whose filaments are folded away. */
  folded: string[];
}

export const NO_FILTERS: StudioFilters = {
  query: "",
  nozzles: [],
  materials: [],
  source: "all",
  allPrinters: false,
  imported: false,
  folded: [],
};

/** One product for some filaments of the user's own. */
export interface ProductLink {
  profileNames: string[];
  key: string;
}

/** What is being taken out, for the question that is asked first. */
export interface Removal {
  profileNames: string[];
  label: string;
  /** Some of it the person made in the slicer themselves. */
  own: boolean;
  /** The slicer's cloud holds a copy of some of it, so it will come back. */
  synced: boolean;
}

interface Props {
  data: InStudio | null;
  studioName: string;
  busy: boolean;
  /** The catalogue, for brands and for guessing which product a filament is. */
  items: Card[];
  printers: Printer[];
  /** The printer chosen in the header. */
  activeModel: string | null;
  filters: StudioFilters;
  onFilters: (next: StudioFilters) => void;
  /** How many profiles other people shared, by `sharedKey`. */
  sharedCounts: Record<string, number>;
  onBrowse: () => void;
  onRemove: (removal: Removal) => void;
  onFix: (names: string[]) => void;
  /** Present when the Toona server is reachable; asks to sign in first if needed. */
  onShare: ((row: Added | Own) => void) | null;
  /** Opens a profile's own page. */
  onOpen: (profileName: string) => void;
  /** Asks which catalogue product some of the user's own filaments are. */
  onLink: (profileNames: string[], label: string, current: string | null) => void;
  /** Links filaments to the products Toona guessed, without asking further. */
  onLinkNow: (links: ProductLink[]) => void;
  /** Adds a filament Toona added to more printer presets, copying the profile named. */
  onAddNozzles: (profileName: string, printerNames: string[], label: string) => void;
  /** Shows a product in the catalogue, where what others shared for it is listed. */
  onSeeShared: (catalogueKey: string) => void;
}

/** One profile in the slicer: a filament for one printer and nozzle. */
type Entry = { model: string; nozzle: string } & ({ kind: "added"; row: Added } | { kind: "own"; row: Own });

/** One filament as the slicer lists it, with the profile for each nozzle it has. */
interface Tile {
  id: string;
  kind: Entry["kind"];
  alias: string;
  brand: string;
  model: string;
  entries: Entry[];
  /** The product Toona thinks an unlinked filament is. */
  guess: Card | null;
}

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
const shortModel = (model: string) => model.replace(/^Bambu Lab /, "");
const tileId = (entry: Entry) => `${entry.kind}|${entry.model}|${entry.row.alias}`;

/** The printer model and nozzle a printer preset is for. */
export function place(printer: string, printers: Printer[]): { model: string; nozzle: string } {
  const known = printers.find((p) => p.name === printer);
  if (known) return { model: known.model, nozzle: known.nozzle };
  const parts = printer.match(/^(.*?)\s+(\d+(?:\.\d+)?) nozzle$/);
  return parts ? { model: parts[1], nozzle: parts[2] } : { model: printer, nozzle: "" };
}

/** The key `sharedCounts` holds a product's count under, for one printer and nozzle. */
export const sharedKey = (catalogueKey: string, model: string, nozzle: string) => `${catalogueKey}|${model}|${nozzle}`;

/** Whether a catalogue key names a real product, not a profile Toona converted. */
export const isProduct = (catalogueKey: string | null | undefined): catalogueKey is string =>
  !!catalogueKey && !catalogueKey.startsWith("converted/");

/** How many filaments, as the slicer lists them, a set of profiles is. */
export function filamentCount(data: InStudio | null): number {
  const rows = [...(data?.added ?? []), ...(data?.own ?? [])];
  return new Set(rows.map((row) => row.alias)).size;
}

interface TileProps extends Pick<Props, "busy" | "sharedCounts" | "onRemove" | "onShare" | "onOpen" | "onLink" | "onLinkNow" | "onAddNozzles" | "onSeeShared"> {
  tile: Tile;
  showModel: boolean;
  /** Printer presets of this model the filament has no profile for yet. */
  missing: Printer[];
  /** Null outside selection mode; otherwise whether this tile is ticked. */
  selected: boolean | null;
  onSelect: () => void;
}

function FilamentTile({ tile, showModel, missing, selected, onSelect, busy, sharedCounts, onRemove, onShare, onOpen, onLink, onLinkNow, onAddNozzles, onSeeShared }: TileProps) {
  const [picked, setPicked] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  // The numbers, changes and sharing are about one nozzle's profile; the rest is about the filament.
  const entry = tile.entries.find((e) => e.row.profile_name === picked) ?? tile.entries[0];
  const { row } = entry;
  const names = tile.entries.map((e) => e.row.profile_name);
  const hex = row.hex ?? tile.entries.find((e) => e.row.hex)?.row.hex ?? null;
  const several = tile.entries.length > 1;
  const label = several ? `${tile.alias} (${tile.entries.length} nozzles)` : tile.alias;
  const own = entry.kind === "own" ? entry.row : null;
  const present = entry.kind === "own" || entry.row.present;
  const changes = withRange(row.changes).length;
  const canOpen = row.parent !== null && present;
  const canShare = onShare && canOpen && (entry.kind === "added" || row.changes.length > 0);
  const product = row.catalogue_key;
  const shared = isProduct(product) ? (sharedCounts[sharedKey(product, entry.model, entry.nozzle)] ?? 0) : 0;
  // The slicer can hold two profiles of one name for one nozzle; nothing good comes of it.
  const copies = Math.max(...tile.entries.map((e) => tile.entries.filter((other) => other.nozzle === e.nozzle).length));
  const facts = [
    row.nozzle_temp && `${row.nozzle_temp}° nozzle`,
    row.bed_temp && `${row.bed_temp}° bed`,
    row.max_flow && `${row.max_flow} mm³/s`,
  ].filter(Boolean);
  const removal: Removal = {
    profileNames: names,
    label,
    own: tile.kind === "own",
    synced: tile.entries.some((e) => e.row.synced),
  };

  return (
    <div className="tile" data-selected={selected ?? undefined}>
      {selected !== null && (
        <input type="checkbox" className="tile-check" aria-label={`Select ${tile.alias}`} checked={selected} onChange={onSelect} />
      )}
      <span className="tile-colour" style={dotStyle(hex)} aria-hidden />
      <div className="text">
        {canOpen ? (
          <button type="button" className="title" title="Open its settings" onClick={() => onOpen(row.profile_name)}>
            {tile.alias}
          </button>
        ) : (
          <div className="title">{tile.alias}</div>
        )}
        {facts.length > 0 && <div className="tile-facts">{facts.join(" · ")}</div>}
        <div className="tags">
          {row.material && <span className={`tag material ${materialTone(row.material)}`}>{row.material}</span>}
          {showModel && <span className="tag">{shortModel(tile.model)}</span>}
          {tile.entries.map((e, index) =>
            several ? (
              <button
                key={`${e.row.profile_name}|${index}`}
                type="button"
                className="tag"
                aria-pressed={e === entry}
                title={`Show the ${e.nozzle} mm profile`}
                onClick={() => setPicked(e.row.profile_name)}
              >
                {e.nozzle}
              </button>
            ) : (
              e.nozzle && <span key={e.row.profile_name} className="tag">{e.nozzle} mm</span>
            ),
          )}
          {selected === null &&
            missing.map((printer) => (
              <button
                key={printer.name}
                type="button"
                className="tag add"
                title={`Add it for the ${printer.nozzle} mm nozzle too`}
                aria-label={`Add ${tile.alias} for the ${printer.nozzle} mm nozzle`}
                disabled={busy}
                onClick={() => onAddNozzles(row.profile_name, [printer.name], `${tile.alias} (${printer.nozzle} mm)`)}
              >
                <Plus size={12} weight="bold" aria-hidden />
                {printer.nozzle}
              </button>
            ))}
          {tile.kind === "added" && <span className="tag toona" title="Added with Toona">Toona</span>}
          {own?.product && (
            <button type="button" className="tag product" title="Change which product this is" disabled={busy} onClick={() => onLink(names, tile.alias, own.catalogue_key)}>
              <LinkSimple size={14} aria-hidden />
              <span>{own.product}</span>
            </button>
          )}
          {shared > 0 && isProduct(product) && (
            <button type="button" className="tag product" title="See what other people shared for this product" onClick={() => onSeeShared(product)}>
              <UsersThree size={14} aria-hidden />
              <span>{shared} shared</span>
            </button>
          )}
          {copies > 1 && (
            <span className="tag warn" title={`${copies} profiles have this name for the same nozzle. Keep one.`}>
              <Copy size={14} aria-hidden />
              {copies} copies
            </span>
          )}
          {tile.entries.every((e) => e.row.synced) && (
            <span className="tag-icon" title="Synced to your Bambu account">
              <CloudCheck size={16} aria-label="Synced to your Bambu account" />
            </span>
          )}
          {!present && <span className="warn-text">files missing</span>}
        </div>
      </div>

      {selected === null && (
        <div className="tile-actions">
          {row.parent && changes > 0 && (
            <button type="button" className="secondary" aria-expanded={open} onClick={() => setOpen(!open)}>
              <span>{changes === 1 ? "1 change" : `${changes} changes`}</span>
              <CaretDown size={14} aria-hidden />
            </button>
          )}
          {own && !own.product && !tile.guess && (
            <button type="button" className="secondary" disabled={busy} onClick={() => onLink(names, tile.alias, null)}>
              <LinkSimple size={18} aria-hidden />
              <span>Link to product</span>
            </button>
          )}
          {canShare && (
            <button type="button" className="secondary icon" title="Share" aria-label={`Share ${tile.alias}`} disabled={busy} onClick={() => onShare(row)}>
              <ShareNetwork size={18} aria-hidden />
            </button>
          )}
          <button
            type="button"
            className="secondary icon"
            title={tile.kind === "added" ? "Remove" : "Delete"}
            aria-label={`${tile.kind === "added" ? "Remove" : "Delete"} ${label}`}
            disabled={busy}
            onClick={() => onRemove(removal)}
          >
            <Trash size={18} aria-hidden />
          </button>
        </div>
      )}

      {tile.guess && selected === null && (
        <div className="tile-guess">
          <span>
            Looks like <strong>{tile.guess.brand} {tile.guess.name}</strong>
          </span>
          <button type="button" className="secondary" disabled={busy} onClick={() => onLinkNow([{ profileNames: names, key: tile.guess!.key }])}>
            <LinkSimple size={18} aria-hidden />
            <span>Link</span>
          </button>
          <button type="button" className="secondary" disabled={busy} onClick={() => onLink(names, tile.alias, null)}>
            Something else
          </button>
        </div>
      )}

      {open && row.parent && selected === null && (
        <div className="tile-details">
          <div className="muted">
            {several ? `The ${entry.nozzle} mm profile is built` : "Built"} on {row.parent}, with these changes:
          </div>
          <Changes changes={row.changes} />
        </div>
      )}
    </div>
  );
}

export function InStudioView({
  data,
  studioName,
  busy,
  items,
  printers,
  activeModel,
  filters,
  onFilters,
  sharedCounts,
  onBrowse,
  onRemove,
  onFix,
  onShare,
  onOpen,
  onLink,
  onLinkNow,
  onAddNozzles,
  onSeeShared,
}: Props) {
  /** Null outside selection mode; otherwise the ids of the tiles ticked. */
  const [selection, setSelection] = useState<string[] | null>(null);
  const imported = data?.imported ?? [];
  const set = (change: Partial<StudioFilters>) => onFilters({ ...filters, ...change });
  const toggled = (list: string[], value: string) => (list.includes(value) ? list.filter((other) => other !== value) : [...list, value]);

  const entries = useMemo<Entry[]>(
    () => [
      ...(data?.added ?? []).map((row): Entry => ({ kind: "added", row, ...place(row.printer, printers) })),
      ...(data?.own ?? []).map((row): Entry => ({ kind: "own", row, ...place(row.printer, printers) })),
    ],
    [data, printers],
  );
  const brands = useMemo(() => [...new Set(items.map((item) => item.brand))], [items]);

  /** The nozzles each filament has a profile for, whatever the filters hide. */
  const covered = useMemo(() => {
    const found = new Map<string, Set<string>>();
    for (const entry of entries) found.set(tileId(entry), (found.get(tileId(entry)) ?? new Set()).add(entry.nozzle));
    return found;
  }, [entries]);

  // The header's printer decides what is listed, unless the person asks for every printer.
  const elsewhere = activeModel ? entries.filter((e) => e.model !== activeModel).length : 0;
  const everyPrinter = filters.allPrinters || !activeModel;
  const scoped = everyPrinter ? entries : entries.filter((e) => e.model === activeModel);
  const nozzles = [...new Set(scoped.map((e) => e.nozzle).filter(Boolean))].sort(byName);
  const materials = [...new Set(scoped.map((e) => e.row.material).filter((m): m is string => !!m))].sort(byName);
  const wantedNozzles = filters.nozzles.filter((n) => nozzles.includes(n));
  const wantedMaterials = filters.materials.filter((m) => materials.includes(m));
  const typed = filters.query.toLowerCase().split(/\s+/).filter(Boolean);
  const says = (text: string) => typed.every((word) => text.toLowerCase().includes(word));

  const tiles = useMemo(() => {
    const found = new Map<string, Tile>();
    for (const entry of scoped) {
      const { row } = entry;
      if (filters.source !== "all" && (filters.source === "toona") !== (entry.kind === "added")) continue;
      if (wantedNozzles.length && !wantedNozzles.includes(entry.nozzle)) continue;
      if (wantedMaterials.length && !wantedMaterials.includes(row.material ?? "")) continue;
      const brand = row.brand ?? brandOf(row.alias, brands) ?? row.alias.split(/\s+/)[0] ?? "Other";
      const product = entry.kind === "own" ? (entry.row.product ?? "") : "";
      if (!says(`${row.alias} ${brand} ${product} ${row.material ?? ""} ${row.printer}`)) continue;
      const id = tileId(entry);
      const tile = found.get(id);
      if (tile) tile.entries.push(entry);
      else found.set(id, { id, kind: entry.kind, alias: row.alias, brand, model: entry.model, entries: [entry], guess: null });
    }
    for (const tile of found.values()) {
      tile.entries.sort((a, b) => byName(a.nozzle, b.nozzle));
      if (tile.kind === "own" && tile.entries.every((e) => e.kind === "own" && !e.row.catalogue_key)) {
        tile.guess = bestGuess(tile.alias, items);
      }
    }
    return [...found.values()].sort((a, b) => byName(a.alias, b.alias) || byName(a.model, b.model));
    // `says` and the wanted lists are made from the filters and the scope each time.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scoped, filters, brands, items]);

  const groups = useMemo(() => {
    const found = new Map<string, Tile[]>();
    for (const tile of tiles) {
      // "eSUN" and "ESUN" are one maker.
      const name = [...found.keys()].find((other) => other.toLowerCase() === tile.brand.toLowerCase()) ?? tile.brand;
      found.set(name, [...(found.get(name) ?? []), tile]);
    }
    return [...found.entries()].sort((a, b) => byName(a[0], b[0]));
  }, [tiles]);

  /** Printer presets of a filament's model that it has no profile for, if Toona can add them. */
  const missingFor = (tile: Tile): Printer[] => {
    if (tile.kind !== "added" || !tile.entries.every((e) => isProduct(e.row.catalogue_key))) return [];
    const has = covered.get(tile.id) ?? new Set();
    return printers.filter((p) => p.model === tile.model && !has.has(p.nozzle)).sort((a, b) => byName(a.nozzle, b.nozzle));
  };

  const guessed = tiles.filter((tile) => tile.guess);
  const shownImported = imported.filter((row) => says(row.name));
  const fixable = shownImported.filter((row) => !row.problem && !row.fixed).map((row) => row.name);
  const importedOpen = filters.imported || (typed.length > 0 && shownImported.length > 0);
  const filtering = typed.length > 0 || wantedNozzles.length > 0 || wantedMaterials.length > 0 || filters.source !== "all";
  const kinds = new Set(scoped.map((e) => e.kind));
  // Only what is on screen can be ticked; a filter change drops the rest.
  const ticked = tiles.filter((tile) => selection?.includes(tile.id));
  const removeTicked = () => {
    const picked = ticked.flatMap((tile) => tile.entries);
    onRemove({
      profileNames: picked.map((e) => e.row.profile_name),
      label: ticked.length === 1 ? ticked[0].alias : `${ticked.length} filaments`,
      own: picked.some((e) => e.kind === "own"),
      synced: picked.some((e) => e.row.synced),
    });
    setSelection(null);
  };

  return (
    <div className="in-studio">
      <h1>In {studioName}</h1>

      {data && entries.length === 0 && (
        <div className="blank">
          <div className="title">Nothing here yet</div>
          <div className="muted">
            Filaments you add from the catalogue are listed here, next to the ones you made in {studioName} yourself.
          </div>
          <button type="button" className="primary" onClick={onBrowse}>
            Browse filaments
          </button>
        </div>
      )}

      {entries.length > 0 && (
        <div className="studio-tools">
          <label className="search">
            <MagnifyingGlass size={22} aria-hidden />
            <input
              type="search"
              placeholder="Find a filament"
              aria-label={`Find a filament in ${studioName}`}
              value={filters.query}
              onChange={(event) => set({ query: event.target.value })}
            />
          </label>
          <div className="filters">
            {kinds.size > 1 && (
              <div className="segmented" role="group" aria-label="Where it came from">
                {([["all", "All"], ["toona", "Added with Toona"], ["own", `Made in ${studioName}`]] as const).map(([value, text]) => (
                  <button key={value} type="button" aria-pressed={filters.source === value} onClick={() => set({ source: value })}>
                    {text}
                  </button>
                ))}
              </div>
            )}
            {nozzles.length > 1 &&
              nozzles.map((nozzle) => (
                <button
                  key={nozzle}
                  type="button"
                  className="chip"
                  aria-pressed={wantedNozzles.includes(nozzle)}
                  title={`Only the ${nozzle} mm nozzle`}
                  onClick={() => set({ nozzles: toggled(wantedNozzles, nozzle) })}
                >
                  {nozzle}
                </button>
              ))}
            {nozzles.length > 1 && materials.length > 1 && <span className="filter-gap" aria-hidden />}
            {materials.length > 1 &&
              materials.map((material) => (
                <button
                  key={material}
                  type="button"
                  className="chip"
                  aria-pressed={wantedMaterials.includes(material)}
                  title={`Only ${material}`}
                  onClick={() => set({ materials: toggled(wantedMaterials, material) })}
                >
                  {material}
                </button>
              ))}
            <span className="filter-gap" aria-hidden />
            {elsewhere > 0 && (
              <button type="button" className="chip" aria-pressed={filters.allPrinters} onClick={() => set({ allPrinters: !filters.allPrinters })}>
                All printers
              </button>
            )}
            <button type="button" className="chip" aria-pressed={selection !== null} onClick={() => setSelection(selection ? null : [])}>
              Select
            </button>
            {elsewhere > 0 && !filters.allPrinters && activeModel && (
              <span className="muted filter-note">
                Showing the {shortModel(activeModel)}. {elsewhere} more for other printers.
              </span>
            )}
          </div>
        </div>
      )}

      {guessed.length > 1 && selection === null && (
        <div className="guess-all">
          <LinkSimple size={18} aria-hidden />
          <span>
            Toona recognises {guessed.length} of the filaments you made. Linking one to its product shows what you
            tuned, and lets you share it.
          </span>
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={() => onLinkNow(guessed.map((tile) => ({ profileNames: tile.entries.map((e) => e.row.profile_name), key: tile.guess!.key })))}
          >
            Link all {guessed.length}
          </button>
        </div>
      )}

      {groups.map(([brand, rows]) => {
        // A search looks inside folded brands too.
        const folded = filters.folded.includes(brand) && typed.length === 0;
        return (
          <section key={brand} className="brand-group">
            <h2 className="group-head">
              <button type="button" aria-expanded={!folded} onClick={() => set({ folded: toggled(filters.folded, brand) })}>
                <CaretDown size={14} aria-hidden />
                <span>{brand}</span>
                <span className="count">{rows.length}</span>
              </button>
            </h2>
            {!folded && (
              <div className="tiles">
                {rows.map((tile) => (
                  <FilamentTile
                    key={tile.id}
                    tile={tile}
                    showModel={everyPrinter && elsewhere > 0}
                    missing={missingFor(tile)}
                    selected={selection ? selection.includes(tile.id) : null}
                    onSelect={() => setSelection(toggled(selection ?? [], tile.id))}
                    busy={busy}
                    sharedCounts={sharedCounts}
                    onRemove={onRemove}
                    onShare={onShare}
                    onOpen={onOpen}
                    onLink={onLink}
                    onLinkNow={onLinkNow}
                    onAddNozzles={onAddNozzles}
                    onSeeShared={onSeeShared}
                  />
                ))}
              </div>
            )}
          </section>
        );
      })}

      {entries.length > 0 && tiles.length === 0 && (
        <div className="blank">
          <div className="title">{filtering ? "Nothing matches" : `Nothing for the ${shortModel(activeModel ?? "")} yet`}</div>
          {filtering ? (
            <button type="button" className="primary" onClick={() => set({ query: "", nozzles: [], materials: [], source: "all" })}>
              Clear the search and filters
            </button>
          ) : (
            <button type="button" className="primary" onClick={() => set({ allPrinters: true })}>
              Show all printers
            </button>
          )}
        </div>
      )}

      {shownImported.length > 0 && selection === null && (
        <section className="imported">
          <div className="imported-head">
            <button type="button" className="imported-toggle" aria-expanded={importedOpen} onClick={() => set({ imported: !importedOpen })}>
              <CaretDown size={14} aria-hidden />
              <span>Imported profiles</span>
              <span className="count">{shownImported.length}</span>
            </button>
            <span className="muted">
              These don't show in an AMS slot's filament list.{" "}
              {fixable.length > 0
                ? `Toona can make a copy that does for ${fixable.length === 1 ? "one" : fixable.length} of them. Your originals are kept.`
                : "Toona has no copy left to make for them."}
            </span>
            {fixable.length > 1 && (
              <button type="button" className="secondary" disabled={busy} onClick={() => onFix(fixable)}>
                <Wrench size={18} aria-hidden />
                <span>Fix all {fixable.length}</span>
              </button>
            )}
          </div>
          {importedOpen && (
            <div className="tiles">
              {shownImported.map((row) => (
                <div key={row.name} className="tile plain">
                  <span className="mark" aria-hidden>
                    {row.fixed ? <CheckCircle size={18} /> : <Warning size={18} />}
                  </span>
                  <div className="text">
                    <div className="title">{row.name}</div>
                    <div className="sub">
                      {row.fixed
                        ? "A copy the AMS can use is already there"
                        : row.problem
                          ? `Can't be fixed here. ${row.problem}`
                          : "Not offered for the AMS"}
                    </div>
                  </div>
                  {!row.fixed && !row.problem && (
                    <div className="tile-actions">
                      <button type="button" className="secondary" disabled={busy} onClick={() => onFix([row.name])}>
                        <Wrench size={18} aria-hidden />
                        <span>Fix</span>
                      </button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      {selection !== null && (
        <div className="select-bar" role="toolbar" aria-label="Selected filaments">
          <strong>{ticked.length} selected</strong>
          <button type="button" className="secondary" onClick={() => setSelection(tiles.map((tile) => tile.id))}>
            Select all {tiles.length} shown
          </button>
          <span className="spacer" />
          <button type="button" className="secondary" disabled={busy || ticked.length === 0} onClick={removeTicked}>
            <Trash size={18} aria-hidden />
            <span>Remove{ticked.length > 0 ? ` ${ticked.length}` : ""}</span>
          </button>
          <button type="button" className="secondary" onClick={() => setSelection(null)}>
            Done
          </button>
        </div>
      )}
    </div>
  );
}
