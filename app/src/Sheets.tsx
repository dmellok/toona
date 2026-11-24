import { useEffect, useState } from "react";
import { CheckCircle, CloudArrowUp, Warning } from "@phosphor-icons/react";
import { api } from "./api";
import { swatchStyle } from "./colour";

export type Sheet =
  | { kind: "running"; retry: () => void }
  | { kind: "sync"; confirm: (dontAsk: boolean) => void }
  | { kind: "added"; alias: string; colourName: string; hex: string | null; backup: string | null }
  | { kind: "fixed"; names: string[] }
  | { kind: "confirm-remove"; label: string; confirm: () => void }
  | { kind: "removed"; label: string; stillInCloud: boolean }
  | { kind: "exported"; files: string[] }
  | { kind: "error"; message: string };

interface Props {
  sheet: Sheet | null;
  studioName: string;
  close: () => void;
}

export function Sheets({ sheet, studioName, close }: Props) {
  const [dontAsk, setDontAsk] = useState(false);

  useEffect(() => {
    if (!sheet) return;
    setDontAsk(false);
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sheet, close]);

  if (!sheet) return null;

  const where = (
    <ol className="steps">
      <li>
        <span className="mono">01</span>
        <span>The filament dropdown, under Custom.</span>
      </li>
      <li>
        <span className="mono">02</span>
        <span>An AMS slot's filament list, on the Device tab.</span>
      </li>
    </ol>
  );

  let body: React.ReactNode;
  let className = "sheet";

  switch (sheet.kind) {
    case "running":
      body = (
        <>
          <div className="warn-icon">
            <Warning size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">Quit {studioName} first</h2>
          <p>Filaments can't be changed while {studioName} is open. Quit it, then try again.</p>
          <div className="sheet-buttons">
            <button type="button" onClick={close}>
              Cancel
            </button>
            <button type="button" className="go" autoFocus onClick={sheet.retry}>
              Try again
            </button>
          </div>
        </>
      );
      break;

    case "sync":
      body = (
        <>
          <div>
            <CloudArrowUp size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">This syncs to your Bambu account</h2>
          <p>
            {studioName} uploads new filaments to Bambu Cloud the next time it opens, the same as ones you create
            there. To remove one for good later, delete it in {studioName}.
          </p>
          <label className="check">
            <input type="checkbox" checked={dontAsk} onChange={(event) => setDontAsk(event.target.checked)} />
            <span>Don't ask again</span>
          </label>
          <div className="sheet-buttons">
            <button type="button" onClick={close}>
              Cancel
            </button>
            <button type="button" className="go" autoFocus onClick={() => sheet.confirm(dontAsk)}>
              Continue
            </button>
          </div>
        </>
      );
      break;

    case "added":
      className = "sheet done";
      body = (
        <>
          <div className="done-swatch" style={swatchStyle(sheet.hex)}>
            <span>{sheet.colourName}</span>
            <CheckCircle size={28} aria-hidden />
          </div>
          <div className="done-body">
            <h2 id="sheet-title">Added to {studioName}</h2>
            <p>
              Open {studioName} and look for <strong>{sheet.alias}</strong> in two places.
            </p>
            {where}
            {sheet.backup && <div className="fine selectable">Backup: {sheet.backup}</div>}
            <div className="sheet-buttons">
              <button type="button" className="go" autoFocus onClick={close}>
                Done
              </button>
            </div>
          </div>
        </>
      );
      break;

    case "fixed":
      body = (
        <>
          <div>
            <CheckCircle size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">
            {sheet.names.length === 1 ? "AMS-ready copy made" : `${sheet.names.length} AMS-ready copies made`}
          </h2>
          <p>Open {studioName} and look for them in two places. Once they work, delete the originals there.</p>
          {where}
          <div className="fine selectable">{sheet.names.join("\n")}</div>
          <div className="sheet-buttons">
            <button type="button" className="go" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "confirm-remove":
      body = (
        <>
          <h2 id="sheet-title">Remove from {studioName}?</h2>
          <p>
            <strong>{sheet.label}</strong> will be taken out of {studioName}'s folder. A backup is made first.
          </p>
          <div className="sheet-buttons">
            <button type="button" autoFocus onClick={close}>
              Cancel
            </button>
            <button type="button" className="go" onClick={sheet.confirm}>
              Remove
            </button>
          </div>
        </>
      );
      break;

    case "removed":
      body = (
        <>
          <h2 id="sheet-title">Removed</h2>
          <p>
            <strong>{sheet.label}</strong> is no longer in {studioName}'s folder.
          </p>
          {sheet.stillInCloud && (
            <p>
              Your Bambu account still holds a copy, so {studioName} will bring it back at its next sync. To remove
              it for good, delete it inside {studioName}.
            </p>
          )}
          <div className="sheet-buttons">
            <button type="button" className="go" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "exported":
      body = (
        <>
          <h2 id="sheet-title">Bundle saved</h2>
          <p>
            In {studioName}, choose File, Import, Import Configs and pick the file. Imported profiles don't show in
            the AMS list; adding directly does.
          </p>
          <div className="fine selectable">{sheet.files.join("\n")}</div>
          <div className="sheet-buttons">
            <button type="button" onClick={() => void api.reveal(sheet.files[0])}>
              Show in folder
            </button>
            <button type="button" className="go" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "error":
      body = (
        <>
          <div className="warn-icon">
            <Warning size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">That didn't work</h2>
          <p className="selectable">{sheet.message}</p>
          <div className="sheet-buttons">
            <button type="button" className="go" autoFocus onClick={close}>
              Close
            </button>
          </div>
        </>
      );
      break;
  }

  return (
    <div className="scrim" onClick={(event) => event.target === event.currentTarget && close()}>
      <div className={className} role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        {body}
      </div>
    </div>
  );
}
