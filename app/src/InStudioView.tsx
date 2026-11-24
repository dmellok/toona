import { CaretDown, Trash, Warning, Wrench } from "@phosphor-icons/react";
import type { Added, InStudio } from "./api";
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
}

export function InStudioView({ data, studioName, busy, onBrowse, onRemove, onFix }: Props) {
  const [open, setOpen] = useState<string | null>(null);
  const added = data?.added ?? [];
  const imported = data?.imported ?? [];

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

      <div className="rows">
        {added.map((row) => (
          <div key={row.profile_name} className="row">
            <span className="dot" style={dotStyle(row.hex)} aria-hidden />
            <div className="text">
              <div className="title">{row.alias}</div>
              <div className="sub">
                {row.printer}
                {row.synced ? " · synced to your Bambu account" : ""}
                {row.present ? "" : " · files missing"}
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

      {imported.length > 0 && (
        <>
          <h2>Imported profiles</h2>
          <div className="section-head">
            <p>
              These don't show in an AMS slot's filament list. Toona can make a copy of each that does. Your
              originals are kept.
            </p>
            {imported.length > 1 && (
              <button type="button" className="secondary" disabled={busy} onClick={() => onFix(imported)}>
                <Wrench size={18} aria-hidden />
                <span>Fix all {imported.length}</span>
              </button>
            )}
          </div>
          <div className="rows">
            {imported.map((name) => (
              <div key={name} className="row">
                <span className="dot" aria-hidden>
                  <Warning size={18} />
                </span>
                <div className="text">
                  <div className="title">{name}</div>
                  <div className="sub">Not offered for the AMS</div>
                </div>
                <button type="button" className="secondary" disabled={busy} onClick={() => onFix([name])}>
                  <Wrench size={18} aria-hidden />
                  <span>Fix</span>
                </button>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
