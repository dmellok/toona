import { useEffect, useRef, useState } from "react";
import { ArrowCircleUp, CheckCircle, CloudArrowUp, GithubLogo, Warning } from "@phosphor-icons/react";
import { api, type Card, type Prefs, type Shared, type Update, type User } from "./api";
import { Tree, versionNodes } from "./Tree";
import { swatchStyle } from "./colour";

export type Sheet =
  | { kind: "running"; retry: () => void }
  | { kind: "sync"; confirm: (dontAsk: boolean) => void }
  | { kind: "added"; alias: string; colourName: string; hex: string | null; backup: string | null }
  | { kind: "fixed"; names: string[] }
  | { kind: "confirm-remove"; label: string; confirm: () => void }
  | { kind: "removed"; label: string; stillInCloud: boolean }
  | { kind: "exported"; files: string[] }
  | { kind: "sign-in"; code: string; cancel: () => void }
  | { kind: "account"; user: User; signOut: () => void }
  | { kind: "share"; label: string; changes: number; isEdit: boolean; confirm: (note: string) => void }
  | { kind: "shared"; label: string }
  | { kind: "link"; label: string; items: Card[]; current: string | null; pick: (key: string | null) => void }
  | { kind: "tree"; versions: Shared[]; current: number }
  | { kind: "saved"; label: string; changes: number; backup: string | null }
  | { kind: "prefs"; first: boolean; prefs: Prefs; save: (updates: boolean, usage: boolean) => void; check: (() => void) | null }
  | { kind: "update"; update: Update; install: () => void }
  | { kind: "updating"; version: string }
  | { kind: "current"; version: string }
  | { kind: "error"; message: string };

interface Props {
  sheet: Sheet | null;
  studioName: string;
  close: () => void;
}

export function Sheets({ sheet, studioName, close }: Props) {
  const [dontAsk, setDontAsk] = useState(false);
  const [note, setNote] = useState("");
  const [query, setQuery] = useState("");
  const [updates, setUpdates] = useState(true);
  const [usage, setUsage] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);

  // Fields start empty for each sheet that opens, and only then: the app re-renders
  // every few seconds while a sheet is up, which must not wipe what was typed.
  useEffect(() => {
    setDontAsk(false);
    setNote("");
    setQuery("");
    // First run: update checks start on, usage numbers start off. Later: as they were left.
    setUpdates(sheet?.kind === "prefs" && !sheet.first ? sheet.prefs.updates : true);
    setUsage(sheet?.kind === "prefs" && !sheet.first ? sheet.prefs.usage : false);
    setProgress(null);
  }, [sheet]);

  const updating = sheet?.kind === "updating";
  useEffect(() => {
    if (!updating) return;
    return api.onUpdateProgress((done, total) => setProgress(total ? Math.min(100, Math.round((100 * done) / total)) : null));
  }, [updating]);

  const closeRef = useRef(close);
  closeRef.current = close;
  const isOpen = sheet !== null;
  useEffect(() => {
    if (!isOpen) return;
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && !updating && closeRef.current();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, updating]);

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

    case "sign-in":
      body = (
        <>
          <h2 id="sheet-title">Sign in with GitHub</h2>
          <p>Enter this code on GitHub. Toona reads only your public profile: your name, picture and when you joined.</p>
          <div className="sign-in-code mono selectable">{sheet.code}</div>
          <p className="muted">Waiting for you to approve it on GitHub…</p>
          <div className="sheet-buttons">
            <button type="button" onClick={sheet.cancel}>
              Cancel
            </button>
            <button type="button" className="go" autoFocus onClick={() => void api.openSignInPage()}>
              <GithubLogo size={18} aria-hidden /> Open GitHub
            </button>
          </div>
        </>
      );
      break;

    case "account":
      body = (
        <>
          <h2 id="sheet-title">Signed in as @{sheet.user.handle}</h2>
          <p>Profiles you share and your "it worked" answers show this name.</p>
          <div className="sheet-buttons">
            <button type="button" onClick={sheet.signOut}>
              Sign out
            </button>
            <button type="button" className="go" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "share":
      body = (
        <>
          <h2 id="sheet-title">Share {sheet.label}?</h2>
          <p>
            {sheet.isEdit ? "This is shared as an edit of the profile it was made from. " : ""}
            {sheet.changes === 1 ? "Its 1 changed setting" : `Its ${sheet.changes} changed settings`} will be public
            under your GitHub name, along with your printer model and nozzle size. The colour and any G-code are not
            shared.
          </p>
          <label className="note-field">
            <span>What did you change, and why? (optional)</span>
            <textarea value={note} maxLength={500} rows={3} onChange={(event) => setNote(event.target.value)} />
          </label>
          <div className="sheet-buttons">
            <button type="button" onClick={close}>
              Cancel
            </button>
            <button type="button" className="go" onClick={() => sheet.confirm(note)}>
              Share
            </button>
          </div>
        </>
      );
      break;

    case "shared":
      body = (
        <>
          <div className="ok-icon">
            <CheckCircle size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">Shared</h2>
          <p>{sheet.label} is now listed for others with the same printer and nozzle.</p>
          <div className="sheet-buttons">
            <button type="button" className="go" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "link": {
      const words = query.toLowerCase().split(/\s+/).filter(Boolean);
      const matches = sheet.items
        .filter((item) => {
          const text = `${item.brand} ${item.name} ${item.material}`.toLowerCase();
          return words.every((word) => text.includes(word));
        })
        .slice(0, 40);
      body = (
        <>
          <h2 id="sheet-title">Which product is {sheet.label}?</h2>
          <p>Toona compares it with the preset it would use for that product, to show what you changed.</p>
          <input
            className="link-search"
            type="search"
            placeholder="Search brand, name or material"
            aria-label="Search products"
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <ul className="link-list">
            {matches.map((item) => (
              <li key={item.key}>
                <button type="button" aria-pressed={item.key === sheet.current} onClick={() => sheet.pick(item.key)}>
                  <span>
                    {item.brand} {item.name}
                  </span>
                  <span className="mono">{item.material}</span>
                </button>
              </li>
            ))}
            {matches.length === 0 && <li className="muted">No product matches.</li>}
          </ul>
          <div className="sheet-buttons">
            {sheet.current && (
              <button type="button" onClick={() => sheet.pick(null)}>
                Clear
              </button>
            )}
            <button type="button" onClick={close}>
              Cancel
            </button>
          </div>
        </>
      );
      break;
    }

    case "tree":
      className = "sheet wide";
      body = (
        <>
          <h2 id="sheet-title">History</h2>
          <p>Each edit sits to the right of the version it was made from. The dark line leads to the one you picked.</p>
          <Tree nodes={versionNodes(sheet.versions)} start={`v${sheet.current}`} />
          <div className="sheet-buttons">
            <button type="button" className="go" onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "saved":
      body = (
        <>
          <div className="ok-icon">
            <CheckCircle size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">Saved</h2>
          <p>
            {sheet.changes === 1 ? "1 setting" : `${sheet.changes} settings`} of <strong>{sheet.label}</strong> changed.
            Open {studioName} to use it.
          </p>
          {sheet.backup && <div className="fine selectable">Backup: {sheet.backup}</div>}
          <div className="sheet-buttons">
            <button type="button" className="go" autoFocus onClick={close}>
              Done
            </button>
          </div>
        </>
      );
      break;

    case "prefs":
      className = "sheet roomy";
      body = (
        <>
          <h2 id="sheet-title">{sheet.first ? "Two choices before you start" : "Settings"}</h2>
          {sheet.first && <p>Toona works the same whichever you pick, and you can change both later under Settings.</p>}
          <button type="button" className="setting switch" role="switch" aria-checked={updates} onClick={() => setUpdates(!updates)}>
            <span>
              <strong>Check for updates</strong>
              <span>
                When Toona opens, it asks api.toona.wiki whether there is a newer version. Nothing installs until you
                say so.
              </span>
            </span>
            <span className="switch-track" aria-hidden>
              <span className="switch-thumb" />
            </span>
          </button>
          <button type="button" className="setting switch" role="switch" aria-checked={usage} onClick={() => setUsage(!usage)}>
            <span>
              <strong>Share usage numbers</strong>
              <span>
                Once a day, tell Toona's makers which version, system, slicer and printer models are in use, and how
                many filaments were added. No account, no id, and never which filaments or anything in your files.
              </span>
            </span>
            <span className="switch-track" aria-hidden>
              <span className="switch-thumb" />
            </span>
          </button>
          <details className="sent">
            <summary>See exactly what would be sent</summary>
            <pre className="selectable">{JSON.stringify(sheet.prefs.report, null, 2)}</pre>
            <p>
              "first", "new_week" and "new_month" say whether this is the first time this copy checked in at all, this
              week and this month. That is how installs are counted without giving each one an id.
            </p>
          </details>
          {!sheet.first && (
            <div className="about">
              <span className="mono">Toona {sheet.prefs.version}</span>
              {sheet.check && (
                <button type="button" className="pill-button outline" onClick={sheet.check}>
                  Check now
                </button>
              )}
            </div>
          )}
          <div className="sheet-buttons">
            {!sheet.first && (
              <button type="button" onClick={close}>
                Cancel
              </button>
            )}
            <button type="button" className="go" autoFocus onClick={() => sheet.save(updates, usage)}>
              {sheet.first ? "Continue" : "Save"}
            </button>
          </div>
        </>
      );
      break;

    case "update":
      body = (
        <>
          <div>
            <ArrowCircleUp size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">Toona {sheet.update.version} is ready</h2>
          {sheet.update.notes ? <p className="notes selectable">{sheet.update.notes}</p> : <p>A newer version of Toona is available.</p>}
          <p>Installing takes a moment and reopens Toona. Your filaments and settings stay as they are.</p>
          <div className="sheet-buttons">
            <button type="button" onClick={close}>
              Later
            </button>
            <button type="button" className="go" autoFocus onClick={sheet.install}>
              Install and reopen
            </button>
          </div>
        </>
      );
      break;

    case "updating":
      body = (
        <>
          <h2 id="sheet-title">Installing Toona {sheet.version}</h2>
          <p>{progress === null ? "Downloading…" : `Downloading… ${progress}%`} Toona reopens by itself when it's done.</p>
          <div
            className={progress === null ? "progress waiting" : "progress"}
            role="progressbar"
            aria-label="Download"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progress ?? undefined}
          >
            <span style={{ width: `${progress ?? 100}%` }} />
          </div>
        </>
      );
      break;

    case "current":
      body = (
        <>
          <div className="ok-icon">
            <CheckCircle size={32} aria-hidden />
          </div>
          <h2 id="sheet-title">You have the newest version</h2>
          <p>
            Toona <span className="mono">{sheet.version}</span> is the latest there is.
          </p>
          <div className="sheet-buttons">
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
    <div className="scrim" onClick={(event) => event.target === event.currentTarget && !updating && close()}>
      <div className={className} role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        {body}
      </div>
    </div>
  );
}
