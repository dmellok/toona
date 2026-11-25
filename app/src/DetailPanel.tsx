import { useEffect, useState } from "react";
import { ArrowLeft, Check, CheckCircle, DownloadSimple, Package, PencilSimple, SealCheck, Trash, Warning, X } from "@phosphor-icons/react";
import type { Detail, Printer, Shared, Spec, Variant } from "./api";
import { BasePicker } from "./BasePicker";
import { Community } from "./Community";
import { Changes, withRange } from "./Changes";
import { dotStyle, swatchStyle } from "./colour";

interface Props {
  detail: Detail;
  /** The colour chosen, if one is. A filament can be added without. */
  variant: Variant | null;
  onColour: (name: string | null) => void;
  advanced: boolean;
  studioName: string;
  studioFound: boolean;
  /** Nozzle presets of the chosen printer model. */
  printers: Printer[];
  activeNozzles: string[];
  onNozzles: (nozzles: string[]) => void;
  /** A base preset the user chose over the automatic one. */
  base: string | null;
  onBase: (preset: string | null) => void;
  /** Names of bases chosen before, most recent first. */
  recentBases: string[];
  /** A name the person gave this filament, in place of the catalogue's. */
  name: string | null;
  onName: (name: string | null) => void;
  /** Use the manufacturer's settings for one nozzle on the others it published nothing for. */
  borrowTuned: boolean;
  onBorrowTuned: (on: boolean) => void;
  busy: boolean;
  onInstall: () => void;
  onRemove: () => void;
  onExport: () => void;
  /** Profiles other people shared for this filament on the chosen printer. */
  shared: Shared[];
  signedIn: boolean;
  onAddShared: (version: Shared) => void;
  onConfirm: (version: Shared, outcome: "worked" | "problems") => void;
  onHistory: (version: Shared) => void;
  /** In a narrow window the panel covers the list while this is set. */
  open: boolean;
  onClose: () => void;
}

const range = (value: [number, number] | null) => (value ? `${value[0]}–${value[1]} °C` : "Not listed");
const presetLabel = (preset: string) => preset.split(" @")[0];

const SPEC_NAMES: Record<Spec["key"], string> = { nozzle: "Nozzle", bed: "Bed", flow: "Max flow", density: "Density" };

/** The manufacturer's figures beside the profile's, with a mark for each that agrees. */
function Compare({ spec, builtOn }: { spec: Spec[]; builtOn: string }) {
  const flow = spec.find((row) => row.key === "flow");
  let note: string | null = null;
  if (flow?.at != null) {
    if (flow.limit == null) note = `The manufacturer lists no limit, so this profile keeps ${builtOn}'s.`;
    else if (flow.base != null && flow.base > flow.limit) note = `Lowered from ${builtOn}'s to the manufacturer's limit.`;
    else if (flow.at < flow.limit) note = `${builtOn} is tuned below the manufacturer's limit, and this profile keeps that.`;
  }
  const bars: { name: string; value: number | null; kind: string }[] = flow?.at != null
    ? [
        { name: "This profile", value: flow.at, kind: flow.ok === false ? "off" : "this" },
        { name: "Manufacturer's limit", value: flow.limit, kind: "other" },
        { name: `${builtOn} on your printer`, value: flow.base, kind: "other" },
      ]
    : [];
  const top = Math.max(1, ...bars.map((bar) => bar.value ?? 0));

  return (
    <>
      <table className="spec">
        <thead>
          <tr>
            <th scope="col">Setting</th>
            <th scope="col">Manufacturer</th>
            <th scope="col">This profile</th>
            <th scope="col">
              <span className="visually-hidden">Within the manufacturer's figures</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {spec.map((row) => (
            <tr key={row.key}>
              <th scope="row">{SPEC_NAMES[row.key]}</th>
              <td className="mono">{row.spec ?? <span className="muted">Not listed</span>}</td>
              <td className="mono">{row.value ?? "–"}</td>
              <td>
                {row.ok === true && (
                  <span title="Within the manufacturer's figures">
                    <CheckCircle size={18} weight="fill" className="spec-ok" aria-label="Within the manufacturer's figures" />
                  </span>
                )}
                {row.ok === false && (
                  <span title="Outside the manufacturer's figures">
                    <Warning size={18} weight="fill" className="spec-off" aria-label="Outside the manufacturer's figures" />
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {bars.length > 0 && (
        <div className="flow">
          <div className="label">Max flow, in mm³/s</div>
          <dl className="flow-rows">
            {bars.map((bar) => (
              <div key={bar.name} className="flow-row">
                <dt>{bar.name}</dt>
                <dd>
                  <span className="flow-track" aria-hidden>
                    {bar.value != null && <span className={`flow-fill ${bar.kind}`} style={{ width: `${(100 * bar.value) / top}%` }} />}
                  </span>
                  <span className="mono">{bar.value ?? "Not listed"}</span>
                </dd>
              </div>
            ))}
          </dl>
          {note && <div className="muted flow-note">{note}</div>}
        </div>
      )}
    </>
  );
}

/** The profile's name, with a pencil to give the filament a name of your own. */
function ProfileName({ alias, fallback, printer, custom, onName }: { alias: string; fallback: string; printer: string; custom: boolean; onName: (name: string | null) => void }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(alias);
  useEffect(() => {
    setEditing(false);
    setDraft(alias);
  }, [alias]);

  if (!editing) {
    return (
      <div className="profile-name">
        <span className="selectable">
          {alias} @{printer}
        </span>
        <button type="button" className="icon-button small" aria-label="Rename this profile" title="Rename" onClick={() => setEditing(true)}>
          <PencilSimple size={16} aria-hidden />
        </button>
      </div>
    );
  }
  const save = () => {
    const name = draft.trim();
    onName(name && name !== fallback ? name : null);
    setEditing(false);
  };
  return (
    <form
      className="rename"
      onSubmit={(event) => {
        event.preventDefault();
        save();
      }}
    >
      <input aria-label="Filament name" value={draft} maxLength={80} autoFocus onChange={(event) => setDraft(event.target.value)} />
      <div className="muted small mono">@{printer}</div>
      <div className="rename-buttons">
        {custom && (
          <button type="button" className="secondary" onClick={() => onName(null)}>
            Use "{fallback}"
          </button>
        )}
        <button type="button" className="secondary" onClick={() => setEditing(false)}>
          Cancel
        </button>
        <button type="submit" className="secondary go">
          Save
        </button>
      </div>
    </form>
  );
}

export function DetailPanel(props: Props) {
  const { detail, variant, printers, activeNozzles } = props;
  const [picking, setPicking] = useState(false);
  useEffect(() => setPicking(false), [detail.key]);

  const model = printers[0]?.model ?? null;
  const builtOn = props.base ? presetLabel(props.base) : (detail.base?.label ?? "Not available");
  const colours = detail.variants.length;
  const usable = (printer: Printer) => detail.base_printers.length === 0 || detail.base_printers.includes(printer.name);
  const targets = printers.filter((p) => activeNozzles.includes(p.nozzle) && usable(p));
  const missing = printers.filter((p) => !usable(p)).map((p) => p.nozzle);
  const added = variant ? variant.added : detail.added;
  const flow = detail.spec.find((row) => row.key === "flow");
  const changeCount = withRange(detail.changes).length;
  const short = (printer: Printer) => printer.nozzle;
  /** Nozzles of this printer the manufacturer published settings for, and the rest. */
  const published = printers.filter((p) => detail.tuned?.published.includes(p.name));
  const unpublished = printers.filter((p) => usable(p) && !published.includes(p));
  const list = (items: string[]) => (items.length > 1 ? `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}` : (items[0] ?? ""));

  const toggleNozzle = (nozzle: string) => {
    const next = activeNozzles.includes(nozzle)
      ? activeNozzles.filter((n) => n !== nozzle)
      : [...activeNozzles, nozzle];
    // At least one nozzle stays chosen, in the printer's own order.
    if (next.length) props.onNozzles(printers.map((p) => p.nozzle).filter((n) => next.includes(n)));
  };

  let blocked = detail.blocked;
  if (!props.studioFound) blocked = `${props.studioName} wasn't found on this computer.`;
  else if (!model) blocked = `No printer is set up in ${props.studioName}.`;
  const canChoose = props.studioFound && model !== null && detail.bases.length > 0;

  return (
    <aside className={props.open ? "detail open" : "detail"} aria-label="Selected filament">
      <button type="button" className="secondary detail-back" onClick={props.onClose}>
        <ArrowLeft size={18} aria-hidden />
        <span>All filaments</span>
      </button>
      <div className="swatch" style={swatchStyle(variant?.hex ?? null)}>
        <div className="swatch-top">
          <span>{detail.brand}</span>
          <span>{detail.material}</span>
        </div>
        <div>
          <div className="swatch-name">{variant?.name ?? detail.name}</div>
          <div className="swatch-hex">{variant ? (variant.hex ?? "Colour not listed") : "No colour chosen"}</div>
        </div>
      </div>

      <div className="detail-body">
        <div>
          {/* Without a colour the card above already carries the name. */}
          {variant && <div className="detail-name">{detail.name}</div>}
          <div className="muted">
            {colours === 0 ? "No colours listed" : colours === 1 ? "1 colour in the catalogue" : `${colours} colours in the catalogue`}
            {detail.discontinued ? " · discontinued" : ""}
          </div>
        </div>

        {colours > 0 && (
          <div className="field">
            <div className="colours" role="group" aria-label="Colour">
              {detail.variants.map((v) => (
                <button
                  key={v.name}
                  type="button"
                  className="colour"
                  title={v.name}
                  aria-label={v.name}
                  aria-pressed={v.name === variant?.name}
                  onClick={() => props.onColour(v.name === variant?.name ? null : v.name)}
                >
                  <span style={dotStyle(v.hex)} />
                </button>
              ))}
            </div>
            {variant ? (
              <button type="button" className="clear-colour" onClick={() => props.onColour(null)}>
                <X size={14} aria-hidden />
                <span>No colour</span>
              </button>
            ) : (
              <div className="muted small">Optional: skip it for one profile that covers every colour.</div>
            )}
          </div>
        )}

        <dl className="facts">
          <dt>Nozzle</dt>
          <dd className="mono">{range(detail.nozzle)}</dd>
          <dt>Bed</dt>
          <dd className="mono">{range(detail.bed)}</dd>
          <dt>Max flow</dt>
          <dd className="mono">{flow?.limit != null ? `${flow.limit} mm³/s` : "Not listed"}</dd>
          <dt>Density</dt>
          <dd className="mono">{detail.density ? `${detail.density} g/cm³` : "Not listed"}</dd>
          <dt>Built on</dt>
          <dd>
            {canChoose ? (
              <button
                type="button"
                className="fact-edit"
                aria-expanded={picking}
                aria-label={`Built on ${builtOn}. Change the base profile`}
                onClick={() => setPicking(!picking)}
              >
                <span>{builtOn}</span>
                <PencilSimple size={16} aria-hidden />
              </button>
            ) : (
              builtOn
            )}
          </dd>
        </dl>

        {detail.tuned && !blocked && published.length > 0 && (
          <div className="tuned">
            <SealCheck size={20} weight="fill" aria-hidden />
            <div>
              <div>{detail.tuned.brand}'s own settings</div>
              <ul className="muted small">
                <li>
                  {list(published.map(short))} mm: published at {new URL(detail.tuned.source).hostname}, read {detail.tuned.retrieved}.
                </li>
                {unpublished.length > 0 && (
                  <li>
                    {list(unpublished.map(short))} mm:{" "}
                    {props.borrowTuned
                      ? `none published, so the ${short(published.find((p) => p.nozzle === "0.4") ?? published[0])} mm ones are used, except the flow limit.`
                      : `none published, so ${unpublished.length > 1 ? "these use" : "this uses"} ${builtOn}'s.`}
                  </li>
                )}
              </ul>
              {unpublished.length > 0 && (
                <button type="button" className="mini-switch switch" role="switch" aria-checked={props.borrowTuned} onClick={() => props.onBorrowTuned(!props.borrowTuned)}>
                  <span className="switch-track" aria-hidden>
                    <span className="switch-thumb" />
                  </span>
                  <span>Use them for the other nozzles too</span>
                </button>
              )}
            </div>
          </div>
        )}

        {picking && canChoose && (
          <BasePicker
            auto={detail.base && !props.base ? detail.base : null}
            bases={detail.bases}
            value={props.base}
            recent={props.recentBases}
            onPick={(preset) => {
              props.onBase(preset);
              setPicking(false);
            }}
            onClose={() => setPicking(false)}
          />
        )}

        {!blocked && printers.length > 1 && (
          <div className="field">
            <div className="label" id="nozzles-label">
              Nozzles to add it for
            </div>
            <div className="nozzles" role="group" aria-labelledby="nozzles-label">
              {printers.map((printer) => (
                <button
                  key={printer.name}
                  type="button"
                  className="nozzle"
                  disabled={!usable(printer)}
                  aria-pressed={usable(printer) && activeNozzles.includes(printer.nozzle)}
                  onClick={() => toggleNozzle(printer.nozzle)}
                >
                  {printer.nozzle}
                  {published.includes(printer) && <SealCheck size={14} weight="fill" aria-label="Manufacturer's settings" />}
                </button>
              ))}
            </div>
            {missing.length > 0 && (
              <div className="muted small">
                {props.studioName} has no {builtOn} preset for the {missing.join(", ")} mm nozzle.
              </div>
            )}
          </div>
        )}

        {props.advanced && !blocked && (
          <div className="advanced">
            <div className="field">
              <Compare spec={detail.spec} builtOn={builtOn} />
            </div>

            <div className="field">
              <div className="label">
                {changeCount === 1 ? "1 change" : `${changeCount} changes`} over {builtOn}
              </div>
              <Changes changes={detail.changes} colour={variant?.hex ?? null} />
            </div>

            {targets[0] && (
              <div className="field">
                <div className="label">Profile name</div>
                <ProfileName
                  alias={variant?.alias ?? detail.alias}
                  fallback={detail.default_alias}
                  printer={targets[0].name}
                  custom={props.name !== null}
                  onName={props.onName}
                />
              </div>
            )}
          </div>
        )}

        <Community
          versions={props.shared}
          signedIn={props.signedIn}
          blocked={blocked ?? (added ? `Remove it from ${props.studioName} first.` : null)}
          busy={props.busy}
          studioName={props.studioName}
          onAdd={props.onAddShared}
          onConfirm={props.onConfirm}
          onHistory={props.onHistory}
        />

        <div className="actions">
          {blocked ? (
            <div className="blocked">{blocked}</div>
          ) : added ? (
            <div className="in-studio-row">
              <div className="in-studio-mark">
                <Check size={20} aria-hidden />
                <span>In {props.studioName}</span>
              </div>
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove from ${props.studioName}`}
                disabled={props.busy}
                onClick={props.onRemove}
              >
                <Trash size={20} aria-hidden />
              </button>
            </div>
          ) : (
            <button type="button" className="primary" disabled={props.busy} onClick={props.onInstall}>
              <DownloadSimple size={20} aria-hidden />
              <span>Add to {props.studioName}</span>
            </button>
          )}
          {!blocked && model && (
            <div className="target">
              For {model} · {targets.map((p) => p.nozzle).join(", ")} mm nozzle{variant ? "" : " · no colour"}
            </div>
          )}
          {props.advanced && !blocked && (
            <button type="button" className="link" disabled={props.busy} onClick={props.onExport}>
              <Package size={18} aria-hidden />
              <span>Export a bundle to import instead</span>
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
