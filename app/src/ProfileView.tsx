import { useEffect, useMemo, useState } from "react";
import { ArrowCounterClockwise, ArrowLeft, Eye, EyeSlash, Minus, Plus } from "@phosphor-icons/react";
import { api, type Edit, type Profile, type Setting, type Shared } from "./api";
import { settingLabel } from "./Changes";
import { dotStyle } from "./colour";
import { KINDS, Tree, versionNodes, type Kind, type TreeNode } from "./Tree";

interface Props {
  profile: Profile;
  studioName: string;
  busy: boolean;
  /** Shared versions are only reachable when the Toona server is. */
  community: boolean;
  onBack: () => void;
  /** Saves the edits; with `share`, also shares the result as a new version. */
  onSave: (edits: Edit[], share: boolean) => void;
}

type Tab = "layers" | "edit" | "tree";

/** Most specific first, as they are stacked on screen. */
const STACK: Kind[] = ["you", "community", "colour", "manufacturer", "material"];
const rank = (kind: Kind) => STACK.length - STACK.indexOf(kind);

const shown = (setting: Setting, value: string) => `${value}${setting.unit}`;

/** "Bambu Lab X2D 0.4 nozzle" as "X2D · 0.4 mm". */
function shortPrinter(printer: string): string {
  const match = printer.match(/^(?:Bambu Lab )?(.*?) ([\d.]+) nozzle$/);
  return match ? `${match[1]} · ${match[2]} mm` : printer;
}

/** The columns of the layers view: the settings people look for, then anything else that was changed. */
const COLUMNS: { label: string; keys: string[] }[] = [
  { label: "Density", keys: ["filament_density"] },
  { label: "Nozzle range", keys: ["nozzle_temperature_range_low", "nozzle_temperature_range_high"] },
  { label: "Nozzle", keys: ["nozzle_temperature"] },
  { label: "Flow", keys: ["filament_flow_ratio"] },
  { label: "Max speed", keys: ["filament_max_volumetric_speed"] },
  { label: "Pressure adv.", keys: ["pressure_advance"] },
  { label: "Smooth bed", keys: ["hot_plate_temp"] },
  { label: "Textured bed", keys: ["textured_plate_temp"] },
  { label: "Fan", keys: ["fan_max_speed"] },
];

/** A setting's value; a colour code gets a dot of that colour beside it, since the pill itself is the layer's colour. */
function Value({ text }: { text: string }) {
  if (!/^#[0-9a-f]{6}$/i.test(text)) return <>{text}</>;
  return (
    <>
      <span className="value-dot" style={{ background: text }} aria-hidden />
      {text}
    </>
  );
}

/** Every layer as a row, every setting as a column: who set what, and what ends up printing. */
function Layers({ profile }: { profile: Profile }) {
  const [hidden, setHidden] = useState<Kind[]>([]);

  const columns = useMemo(() => {
    const byKey = new Map(profile.settings.map((s) => [s.key, s]));
    const listed = new Set(COLUMNS.flatMap((c) => c.keys));
    const extra = profile.settings
      .filter((s) => s.layer !== null && !listed.has(s.key))
      .map((s) => ({ label: settingLabel(s.key), keys: [s.key] }));
    return [...COLUMNS, ...extra]
      .map((column) => ({ label: column.label, parts: column.keys.map((key) => byKey.get(key)).filter((s): s is Setting => !!s) }))
      .filter((column) => column.parts.length > 0)
      .map((column) => {
        const unit = column.parts[0].unit;
        // A layer sets the column if it sets any part of it; parts it leaves alone keep the value beneath.
        const values = new Map<Kind, string>();
        values.set("material", column.parts.map((s) => s.base ?? "–").join("–") + unit);
        for (const kind of STACK) {
          if (kind !== "material" && column.parts.some((s) => s.layer === kind)) {
            values.set(kind, column.parts.map((s) => s.value).join("–") + unit);
          }
        }
        return { label: column.label, values };
      });
  }, [profile]);

  const subs: Record<Kind, string> = {
    you: "this printer",
    community: shortPrinter(profile.printer),
    colour: profile.hex ?? "not set",
    manufacturer: profile.brand ?? "from the catalogue",
    material: profile.built_on,
  };
  const winnerOf = (values: Map<Kind, string>) => STACK.find((kind) => values.has(kind) && !hidden.includes(kind))!;

  return (
    <div className="layers-scroll">
      <div className="layers" style={{ ["--columns" as string]: columns.length }}>
        <div className="layers-row layers-top">
          <div className="layers-side caps">What prints</div>
          {columns.map((column) => {
            const winner = winnerOf(column.values);
            return (
              <div key={column.label} className="layers-col">
                <div className="layers-label">{column.label}</div>
                <div className="layers-result mono" style={{ background: KINDS[winner].colour }}>
                  <Value text={column.values.get(winner) ?? ""} />
                </div>
              </div>
            );
          })}
        </div>
        {STACK.map((kind) => {
          const off = hidden.includes(kind);
          return (
            <div key={kind} className={off ? "layers-row layer off" : "layers-row layer"}>
              <div className="layers-side">
                <span className="layers-number mono" style={{ background: KINDS[kind].colour }} aria-hidden>
                  {rank(kind)}
                </span>
                <span className="layers-name">
                  <span>{KINDS[kind].name}</span>
                  <small className="mono">{subs[kind]}</small>
                </span>
                {kind !== "material" && (
                  <button
                    type="button"
                    className="ghost-button"
                    aria-pressed={off}
                    aria-label={`${off ? "Show" : "Hide"} ${KINDS[kind].name}`}
                    onClick={() => setHidden(off ? hidden.filter((k) => k !== kind) : [...hidden, kind])}
                  >
                    {off ? <EyeSlash size={20} aria-hidden /> : <Eye size={20} aria-hidden />}
                  </button>
                )}
              </div>
              {columns.map((column) => {
                const winner = winnerOf(column.values);
                const wins = kind === winner;
                const above = rank(kind) > rank(winner);
                const colour = KINDS[winner].colour;
                const value = column.values.get(kind);
                return (
                  <div key={column.label} className="layers-cell">
                    {(wins || above) && <span className={wins ? "layers-line to-chip" : "layers-line"} style={{ background: colour }} aria-hidden />}
                    {value !== undefined && (
                      <span
                        className={off ? "layers-chip off mono" : wins ? "layers-chip wins mono" : "layers-chip mono"}
                        style={wins && !off ? { background: colour, borderColor: colour } : undefined}
                      >
                        <Value text={value} />
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

const UNSAFE = "#B45309";

/** How many decimal places a step implies, so 0.96 + 0.01 shows as 0.97 and not 0.9700000001. */
const places = (step: number) => (String(step).split(".")[1] ?? "").length;

/** Sliders with steppers for the settings Toona may change. */
function Editor({
  profile,
  busy,
  studioName,
  canShare,
  onSave,
}: {
  profile: Profile;
  busy: boolean;
  studioName: string;
  canShare: boolean;
  onSave: (edits: Edit[], share: boolean) => void;
}) {
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [target, setTarget] = useState<"you" | "community">("you");
  useEffect(() => setDraft({}), [profile]);

  const settings = profile.settings.filter((s) => s.editable);
  const ink = KINDS[target].colour;
  // Above the manufacturer's top nozzle temperature is allowed but flagged.
  const rangeHigh = Number(profile.settings.find((s) => s.key === "nozzle_temperature_range_high")?.value);
  // So is pushing it through the nozzle faster than the manufacturer rates it for.
  const safeMaxOf = (setting: Setting) => {
    if (setting.key === "filament_max_volumetric_speed") return profile.max_flow ?? null;
    return (setting.key === "nozzle_temperature" || setting.key === "nozzle_temperature_initial_layer") && Number.isFinite(rangeHigh)
      ? rangeHigh
      : null;
  };

  const set = (key: string, value: string | null) => {
    const next = { ...draft };
    if (value === null) delete next[key];
    else next[key] = value;
    setDraft(next);
  };
  const problem = (setting: Setting): string | null => {
    const text = draft[setting.key];
    if (text === undefined) return null;
    const number = Number(text);
    if (text.trim() === "" || !Number.isFinite(number)) return "Enter a number";
    if (number < setting.min || number > setting.max) return `Between ${setting.min} and ${setting.max}`;
    return null;
  };
  const changedOf = (setting: Setting) => draft[setting.key] !== undefined && draft[setting.key].trim() !== setting.value;
  const edits = settings.filter(changedOf).map((s) => ({ key: s.key, value: draft[s.key].trim() }));
  const invalid = settings.some((s) => problem(s) !== null);
  const groups = [...new Set(settings.map((s) => s.group))];
  const sub = [profile.hex, shortPrinter(profile.printer)].filter(Boolean).join(" · ");

  return (
    <div className="editor">
      <header className="editor-head">
        <span className="editor-swatch" style={dotStyle(profile.hex)} aria-hidden />
        <div className="editor-title">
          <div className="editor-alias">{profile.alias}</div>
          <div className="caps">{sub}</div>
        </div>
        {canShare && (
          <div className="save-to" role="group" aria-label="Save to">
            {(["you", "community"] as const).map((kind) => (
              <button key={kind} type="button" aria-pressed={target === kind} onClick={() => setTarget(kind)}>
                <span className="tree-key" style={{ background: KINDS[kind].colour }} aria-hidden />
                {kind === "you" ? "Just me" : "Share"}
              </button>
            ))}
          </div>
        )}
      </header>

      <section className="editor-card" aria-label="Settings">
        {groups.map((group) => (
          <div key={group}>
            <div className="editor-group caps">{group}</div>
            {settings
              .filter((s) => s.group === group)
              .map((setting) => {
                const label = settingLabel(setting.key);
                const original = Number(setting.value);
                const text = draft[setting.key] ?? setting.value;
                const number = Number(text);
                const valid = text.trim() !== "" && Number.isFinite(number);
                const changed = changedOf(setting);
                const error = problem(setting);
                const safeMax = safeMaxOf(setting);
                const unsafe = valid && safeMax !== null && number > safeMax;
                const colour = unsafe ? UNSAFE : changed ? ink : "#15171A";
                const source: Kind = changed ? target : (setting.layer ?? "material");

                // The track shows ten steps either side of where the value started, clipped to what is allowed.
                const now = valid ? number : original;
                const lo = Math.max(setting.min, Math.min(original, now) - 10 * setting.step);
                const hi = Math.min(setting.max, Math.max(original, now, safeMax ?? -Infinity) + 10 * setting.step);
                const pct = (v: number) => `${Math.min(100, Math.max(0, ((v - lo) / (hi - lo)) * 100))}%`;
                const step = (direction: 1 | -1) => {
                  const next = Math.min(setting.max, Math.max(setting.min, now + direction * setting.step));
                  set(setting.key, next.toFixed(places(setting.step)));
                };

                return (
                  <div key={setting.key} className="editor-row">
                    <label htmlFor={`edit-${setting.key}`} className="editor-label">
                      <span className="tree-key" style={{ background: KINDS[source].colour }} title={KINDS[source].name} />
                      <span>{label}</span>
                    </label>
                    <div className="track" aria-hidden>
                      <span className="track-bar" />
                      {safeMax !== null && safeMax < hi && <span className="track-unsafe" style={{ left: pct(safeMax) }} />}
                      {changed && valid && (
                        <span
                          className="track-delta"
                          style={{ left: pct(Math.min(original, number)), width: `calc(${pct(Math.max(original, number))} - ${pct(Math.min(original, number))})`, background: colour }}
                        />
                      )}
                      {changed && <span className="track-tick" style={{ left: pct(original) }} />}
                      <span className="track-thumb" style={{ left: pct(now), background: colour }} />
                    </div>
                    <div className="stepper">
                      <button type="button" className="ghost-button" aria-label={`Lower ${label}`} disabled={now <= setting.min} onClick={() => step(-1)}>
                        <Minus size={16} aria-hidden />
                      </button>
                      <span className="stepper-value mono" style={{ color: colour }}>
                        <input
                          id={`edit-${setting.key}`}
                          inputMode="decimal"
                          size={Math.max(2, text.length)}
                          value={text}
                          aria-invalid={error !== null}
                          aria-describedby={error ? `error-${setting.key}` : undefined}
                          onChange={(event) => set(setting.key, event.target.value)}
                        />
                        {setting.unit}
                      </span>
                      <button type="button" className="ghost-button" aria-label={`Raise ${label}`} disabled={now >= setting.max} onClick={() => step(1)}>
                        <Plus size={16} aria-hidden />
                      </button>
                      <button
                        type="button"
                        className="ghost-button reset"
                        aria-label={`Reset ${label}`}
                        style={draft[setting.key] === undefined ? { visibility: "hidden" } : undefined}
                        onClick={() => set(setting.key, null)}
                      >
                        <ArrowCounterClockwise size={18} aria-hidden />
                      </button>
                    </div>
                    {(error || unsafe) && (
                      <div className="editor-error" id={`error-${setting.key}`}>
                        {error ?? `Above the manufacturer's ${safeMax}${setting.unit} limit`}
                      </div>
                    )}
                  </div>
                );
              })}
          </div>
        ))}
        <div className="editor-more">Other settings are edited in {studioName}.</div>
      </section>

      <footer className="editor-foot">
        <div className="chips-row">
          {edits.length === 0 && <span className="chip-note mono">No changes</span>}
          {settings.filter(changedOf).map((s) => (
            <span key={s.key} className="chip-note mono">
              {settingLabel(s.key)} {shown(s, s.value)} → {shown(s, draft[s.key].trim())}
            </span>
          ))}
        </div>
        <button type="button" className="pill-button outline" disabled={busy || Object.keys(draft).length === 0} onClick={() => setDraft({})}>
          Discard
        </button>
        <button
          type="button"
          className="pill-button"
          style={{ background: ink }}
          disabled={busy || invalid || edits.length === 0}
          onClick={() => onSave(edits, target === "community")}
        >
          {target === "you" ? "Save" : "Share as new version"}
        </button>
      </footer>
    </div>
  );
}

/** The profile's family: the preset it starts from, what each layer adds, the shared versions, and your own changes. */
function Family({ profile, community, onTune }: { profile: Profile; community: boolean; onTune: (() => void) | null }) {
  const [versions, setVersions] = useState<Shared[] | null>(null);
  const linked = community ? profile.version_id : null;
  useEffect(() => {
    setVersions(null);
    if (linked === null) return;
    // Without the server the tree still shows the local layers.
    api.family(linked).then(setVersions).catch(() => setVersions([]));
  }, [linked]);

  if (linked !== null && versions === null) return <div className="muted">Loading…</div>;

  const adds = (kind: Kind) => profile.settings.filter((s) => s.layer === kind).map((s) => `${settingLabel(s.key)} ${shown(s, s.value)}`);
  const product = [profile.brand, profile.product].filter(Boolean).join(" ") || profile.alias;
  const nodes: TreeNode[] = [
    { id: "material", kind: "material", name: profile.built_on, adds: [], note: "The preset everything starts from" },
    { id: "manufacturer", parent: "material", kind: "manufacturer", name: product, adds: adds("manufacturer"), note: `Same as ${profile.built_on}` },
    { id: "colour", parent: "manufacturer", kind: "colour", name: profile.hex ?? "Colour", adds: adds("colour"), note: `Same as ${product}` },
    ...versionNodes(versions ?? [], "colour"),
  ];
  const installed = linked !== null && nodes.some((n) => n.id === `v${linked}`) ? `v${linked}` : "colour";
  const mine = adds("you");
  if (mine.length > 0) nodes.push({ id: "you", parent: installed, kind: "you", name: "Mine", adds: mine });
  const here = mine.length > 0 ? "you" : installed;

  return <Tree key={nodes.length} nodes={nodes} start={here} action={onTune ? { on: here, label: "Tune from here", run: onTune } : undefined} />;
}

/** One profile on its own page: its layers, an editor for the ones Toona added, and its family tree. */
export function ProfileView({ profile, studioName, busy, community, onBack, onSave }: Props) {
  const tabs: { id: Tab; name: string }[] = [
    { id: "layers", name: "Layers" },
    ...(profile.editable ? [{ id: "edit" as const, name: "Edit" }] : []),
    { id: "tree", name: "Tree" },
  ];
  const [tab, setTab] = useState<Tab>("layers");
  const active = tabs.some((t) => t.id === tab) ? tab : "layers";
  const product = profile.product ?? profile.alias;
  const sub = [profile.brand, shortPrinter(profile.printer)].filter(Boolean).join(" · ");

  return (
    <div className="profile">
      <header className="profile-head">
        <button type="button" className="icon-button" aria-label="Back to the list" onClick={onBack}>
          <ArrowLeft size={20} aria-hidden />
        </button>
        <div className="profile-title">
          <h1>{product}</h1>
          <span className="caps">{sub}</span>
        </div>
        <nav className="tabs" aria-label="Profile views">
          {tabs.map((t) => (
            <button key={t.id} type="button" className="tab" aria-pressed={active === t.id} onClick={() => setTab(t.id)}>
              {t.name}
            </button>
          ))}
        </nav>
      </header>
      {!profile.editable && (
        <div className="muted">Made in {studioName}, so Toona shows it but doesn't change it.</div>
      )}
      {active === "layers" && <Layers profile={profile} />}
      {active === "edit" && <Editor profile={profile} busy={busy} studioName={studioName} canShare={community} onSave={onSave} />}
      {active === "tree" && <Family profile={profile} community={community} onTune={profile.editable ? () => setTab("edit") : null} />}
    </div>
  );
}
