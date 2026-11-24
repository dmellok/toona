import { CaretDown, CheckCircle, LinkSimple, Printer as PrinterIcon, ShareNetwork, Stack, Trash, Warning, Wrench } from "@phosphor-icons/react";
import type { Added, InStudio, Own } from "./api";
import { useState } from "react";
import { Changes } from "./Changes";
import { dotStyle } from "./colour";

interface Props {
  data: InStudio | null;
  studioName: string;
  busy: boolean;
  onBrowse: () => void;
  onRemove: (row: Added) => void;
  onFix: (names: string[]) => void;
  /** Present when the Toona server is reachable; asks to sign in first if needed. */
  onShare: ((row: Added | Own) => void) | null;
  /** Opens a profile's own page. */
  onOpen: (profileName: string) => void;
  /** Asks which catalogue product one of the user's own filaments is. */
  onLink: (row: Own) => void;
}

/** Rows in the order they came, gathered under the printer preset each is for. */
function byPrinter<T extends { printer: string }>(rows: T[]): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const row of rows) groups.set(row.printer, [...(groups.get(row.printer) ?? []), row]);
  return [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0], undefined, { numeric: true }));
}

function PrinterHead({ name, count }: { name: string; count: number }) {
  return (
    <h3 className="printer-head">
      <PrinterIcon size={18} aria-hidden />
      <span>{name}</span>
      <span className="count">{count}</span>
    </h3>
  );
}

export function InStudioView({ data, studioName, busy, onBrowse, onRemove, onFix, onShare, onOpen, onLink }: Props) {
  const [open, setOpen] = useState<string | null>(null);
  const added = data?.added ?? [];
  const imported = data?.imported ?? [];
  const own = data?.own ?? [];
  // With one printer the heading would only repeat what every row says.
  const grouped = new Set([...added, ...own].map((row) => row.printer)).size > 1;
  const fixable = imported.filter((row) => !row.problem && !row.fixed).map((row) => row.name);

  return (
    <div className="in-studio">
      <h1>In {studioName}</h1>

      {data && added.length === 0 && (
        <div className="blank">
          <div className="title">Nothing added yet</div>
          <div className="muted">
            Filaments you add from the catalogue are listed here, and only these can be removed.
          </div>
          <button type="button" className="primary" onClick={onBrowse}>
            Browse filaments
          </button>
        </div>
      )}

      {byPrinter(added).map(([printer, rows]) => (
      <div key={printer} className="printer-group">
      {grouped && <PrinterHead name={printer} count={rows.length} />}
      <div className="rows">
        {rows.map((row) => (
          <div key={row.profile_name} className="row">
            <span className="dot" style={dotStyle(row.hex)} aria-hidden />
            <div className="text">
              <div className="title">{row.alias}</div>
              <div className="sub">
                {[grouped ? null : row.printer, row.synced ? "synced to your Bambu account" : null, row.present ? null : "files missing"]
                  .filter(Boolean)
                  .join(" · ") || "Added by Toona"}
              </div>
            </div>
            {row.parent && (
              <button
                type="button"
                className="secondary"
                aria-expanded={open === row.profile_name}
                onClick={() => setOpen(open === row.profile_name ? null : row.profile_name)}
              >
                <span>{row.changes.length === 1 ? "1 change" : `${row.changes.length} changes`}</span>
                <CaretDown size={14} aria-hidden />
              </button>
            )}
            {row.parent && row.present && (
              <button type="button" className="secondary" onClick={() => onOpen(row.profile_name)}>
                <Stack size={18} aria-hidden />
                <span>Open</span>
              </button>
            )}
            {onShare && row.parent && row.present && (
              <button type="button" className="secondary" disabled={busy} onClick={() => onShare(row)}>
                <ShareNetwork size={18} aria-hidden />
                <span>Share</span>
              </button>
            )}
            <button type="button" className="secondary" disabled={busy} onClick={() => onRemove(row)}>
              <Trash size={18} aria-hidden />
              <span>Remove</span>
            </button>
            {open === row.profile_name && row.parent && (
              <div className="row-details">
                <div className="muted">Built on {row.parent}, with these changes:</div>
                <Changes changes={row.changes} />
              </div>
            )}
          </div>
        ))}
      </div>
      </div>
      ))}

      {own.length > 0 && (
        <>
          <h2>Made in {studioName}</h2>
          <div className="section-head">
            <p>
              Custom filaments you created yourself. Toona never changes these. Say which product one is, and you can
              see what you tuned and share it.
            </p>
          </div>
          {byPrinter(own).map(([printer, rows]) => (
          <div key={printer} className="printer-group">
          {grouped && <PrinterHead name={printer} count={rows.length} />}
          <div className="rows">
            {rows.map((row) => (
              <div key={row.profile_name} className="row">
                <span className="dot" style={dotStyle(row.hex)} aria-hidden />
                <div className="text">
                  <div className="title">{row.alias}</div>
                  <div className="sub">
                    {[grouped ? null : row.printer, row.product, row.synced ? "synced to your Bambu account" : null]
                      .filter(Boolean)
                      .join(" · ") || `Made in ${studioName}`}
                  </div>
                </div>
                <button type="button" className="secondary" disabled={busy} onClick={() => onLink(row)}>
                  <LinkSimple size={18} aria-hidden />
                  <span>{row.product ? "Change product" : "Choose product"}</span>
                </button>
                {row.parent && (
                  <button type="button" className="secondary" onClick={() => onOpen(row.profile_name)}>
                    <Stack size={18} aria-hidden />
                    <span>Open</span>
                  </button>
                )}
                {onShare && row.parent && row.changes.length > 0 && (
                  <button type="button" className="secondary" disabled={busy} onClick={() => onShare(row)}>
                    <ShareNetwork size={18} aria-hidden />
                    <span>Share</span>
                  </button>
                )}
              </div>
            ))}
          </div>
          </div>
          ))}
        </>
      )}

      {imported.length > 0 && (
        <>
          <h2>Imported profiles</h2>
          <div className="section-head">
            <p>
              These don't show in an AMS slot's filament list.{" "}
              {fixable.length > 0
                ? "Where it can, Toona makes a copy that does. Your originals are kept."
                : "Toona has no copy left to make for them."}
            </p>
            {fixable.length > 1 && (
              <button type="button" className="secondary" disabled={busy} onClick={() => onFix(fixable)}>
                <Wrench size={18} aria-hidden />
                <span>Fix all {fixable.length}</span>
              </button>
            )}
          </div>
          <div className="rows">
            {imported.map((row) => (
              <div key={row.name} className="row">
                <span className="dot" aria-hidden>
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
                  <button type="button" className="secondary" disabled={busy} onClick={() => onFix([row.name])}>
                    <Wrench size={18} aria-hidden />
                    <span>Fix</span>
                  </button>
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
