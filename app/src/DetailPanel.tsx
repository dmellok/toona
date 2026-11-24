import { CaretDown, Check, DownloadSimple, Package, Trash } from "@phosphor-icons/react";
import type { Detail, Printer, Variant } from "./api";
import { Changes } from "./Changes";
import { dotStyle, swatchStyle } from "./colour";

interface Props {
  detail: Detail;
  variant: Variant | null;
  onColour: (name: string) => void;
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
  busy: boolean;
  onInstall: () => void;
  onRemove: () => void;
  onExport: () => void;
}

const range = (value: [number, number] | null) => (value ? `${value[0]}–${value[1]} °C` : "Not listed");
const presetLabel = (preset: string) => preset.split(" @")[0];

export function DetailPanel(props: Props) {
  const { detail, variant, printers, activeNozzles } = props;
  const model = printers[0]?.model ?? null;
  const targets = printers.filter((p) => activeNozzles.includes(p.nozzle));
  const builtOn = props.base ? presetLabel(props.base) : (detail.base?.label ?? "Not available");
  const colours = detail.variants.length;

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

  return (
    <aside className="detail" aria-label="Selected filament">
      <div className="swatch" style={swatchStyle(variant?.hex ?? null)}>
        <div className="swatch-top">
          <span>{detail.brand}</span>
          <span>{detail.material}</span>
        </div>
        <div>
          <div className="swatch-name">{variant?.name ?? detail.name}</div>
          <div className="swatch-hex">{variant?.hex ?? "Colour not listed"}</div>
        </div>
      </div>

      <div className="detail-body">
        <div>
          <div className="detail-name">{detail.name}</div>
          <div className="muted">
            {colours === 0 ? "No colours listed" : colours === 1 ? "1 colour in the catalogue" : `${colours} colours in the catalogue`}
            {detail.discontinued ? " · discontinued" : ""}
          </div>
        </div>

        {colours > 1 && (
          <div className="colours" role="group" aria-label="Colour">
            {detail.variants.map((v) => (
              <button
                key={v.name}
                type="button"
                className="colour"
                title={v.name}
                aria-label={v.name}
                aria-pressed={v.name === variant?.name}
                onClick={() => props.onColour(v.name)}
              >
                <span style={dotStyle(v.hex)} />
              </button>
            ))}
          </div>
        )}

        <dl className="facts">
          <dt>Nozzle</dt>
          <dd className="mono">{range(detail.nozzle)}</dd>
          <dt>Bed</dt>
          <dd className="mono">{range(detail.bed)}</dd>
          <dt>Density</dt>
          <dd className="mono">{detail.density ? `${detail.density} g/cm³` : "Not listed"}</dd>
          <dt>Built on</dt>
          <dd>{builtOn}</dd>
        </dl>

        {props.advanced && !blocked && (
          <div className="advanced">
            <div className="field">
              <label htmlFor="base">Base profile</label>
              <div className="select">
                <select id="base" value={props.base ?? ""} onChange={(event) => props.onBase(event.target.value || null)}>
                  <option value="">
                    {detail.base ? `Automatic: ${detail.base.label} (${detail.base.source})` : "Automatic"}
                  </option>
                  {detail.bases.map((preset) => (
                    <option key={preset} value={preset}>
                      {presetLabel(preset)}
                    </option>
                  ))}
                </select>
                <CaretDown size={14} aria-hidden />
              </div>
            </div>

            {printers.length > 1 && (
              <div className="field">
                <div className="label" id="nozzles-label">
                  Nozzles
                </div>
                <div className="nozzles" role="group" aria-labelledby="nozzles-label">
                  {printers.map((printer) => (
                    <button
                      key={printer.name}
                      type="button"
                      className="nozzle"
                      aria-pressed={activeNozzles.includes(printer.nozzle)}
                      onClick={() => toggleNozzle(printer.nozzle)}
                    >
                      {printer.nozzle}
                    </button>
                  ))}
                </div>
              </div>
            )}

            <div className="field">
              <div className="label">
                {detail.changes.length === 1 ? "1 change" : `${detail.changes.length} changes`} over {builtOn}
              </div>
              <Changes changes={detail.changes} colour={variant?.hex ?? null} />
            </div>

            {variant && targets[0] && (
              <div className="field">
                <div className="label">Profile name</div>
                <div className="profile-name selectable">
                  {variant.alias} @{targets[0].name}
                </div>
              </div>
            )}
          </div>
        )}

        <div className="actions">
          {blocked ? (
            <div className="blocked">{blocked}</div>
          ) : variant?.added ? (
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
              For {model} · {activeNozzles.join(", ")} mm nozzle
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
