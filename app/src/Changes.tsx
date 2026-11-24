import type { Change } from "./api";

/** Names people use for the settings Toona changes. Others fall back to the slicer's own key. */
const LABELS: Record<string, string> = {
  default_filament_colour: "Colour",
  filament_vendor: "Brand",
  filament_density: "Density",
  filament_diameter: "Diameter",
  filament_flow_ratio: "Flow",
  filament_max_volumetric_speed: "Max speed",
  nozzle_temperature: "Nozzle",
  nozzle_temperature_initial_layer: "Nozzle, first layer",
  nozzle_temperature_range_low: "Nozzle range, low",
  nozzle_temperature_range_high: "Nozzle range, high",
  cool_plate_temp: "Cool plate",
  cool_plate_temp_initial_layer: "Cool plate, first layer",
  supertack_plate_temp: "SuperTack plate",
  supertack_plate_temp_initial_layer: "SuperTack plate, first layer",
  eng_plate_temp: "Engineering plate",
  eng_plate_temp_initial_layer: "Engineering plate, first layer",
  hot_plate_temp: "Smooth bed",
  hot_plate_temp_initial_layer: "Smooth bed, first layer",
  textured_plate_temp: "Textured bed",
  textured_plate_temp_initial_layer: "Textured bed, first layer",
  pressure_advance: "Pressure advance",
  fan_min_speed: "Fan, minimum",
  fan_max_speed: "Fan",
  filament_retraction_length: "Retraction length",
};

export const LAYERS: Record<Change["layer"], { name: string; colour: string }> = {
  manufacturer: { name: "Manufacturer", colour: "#15171A" },
  colour: { name: "Colour", colour: "#7C3AED" },
  community: { name: "Community", colour: "#2B4BEE" },
  you: { name: "You", colour: "#0B6B57" },
};

export function settingLabel(key: string): string {
  if (LABELS[key]) return LABELS[key];
  const words = key.replace(/^filament_/, "").replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

interface Props {
  changes: Change[];
  /** The colour picked in the panel, which replaces the one the backend assumed. */
  colour?: string | null;
}

/** What a profile changes over the preset it is built on, one row per setting. */
export function Changes({ changes, colour }: Props) {
  if (changes.length === 0) return <div className="muted">Nothing. It is the base preset under a new name.</div>;
  return (
    <ul className="changes">
      {changes.map((change) => {
        const layer = LAYERS[change.layer];
        const to = change.layer === "colour" && colour ? colour : change.to;
        return (
          <li key={change.key}>
            <span className="layer-dot" style={{ background: layer.colour }} title={layer.name} aria-label={layer.name} />
            <span className="change-name">{settingLabel(change.key)}</span>
            <span className="change-values mono">
              {change.from !== null && change.from !== "" && <s>{change.from}</s>}
              <span>{to}</span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
