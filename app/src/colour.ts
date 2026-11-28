const INK = "#15171A";

const channels = (hex: string) => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};

const brightness = (hex: string) => {
  const [r, g, b] = channels(hex);
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
};

/** Whether dark text reads better than white on this colour. */
export function isLight(hex: string): boolean {
  return brightness(hex) > 0.6;
}

/** The colours of a filament: one, or the several of a multi-colour one. */
export interface Colours {
  hex: string | null;
  /** All of them when there is more than one; empty otherwise. */
  hexes?: string[];
  /** They run into each other along the filament, rather than lying side by side in it. */
  gradual?: boolean;
}

/**
 * What to paint for a filament's colours: the one colour, a blend from each to the next
 * for a filament that changes along its length, or even bands for one that has them side by side.
 */
export function fill(hexes: string[], gradual: boolean, angle = 90): string {
  if (hexes.length < 2) return hexes[0];
  if (gradual) return `linear-gradient(${angle}deg, ${hexes.join(", ")})`;
  const band = 100 / hexes.length;
  const stops = hexes.map((hex, index) => `${hex} ${index * band}% ${(index + 1) * band}%`);
  return `linear-gradient(${angle}deg, ${stops.join(", ")})`;
}

const all = (colours: Colours): string[] => (colours.hexes?.length ? colours.hexes : colours.hex ? [colours.hex] : []);

/** Styles for a block filled with a filament's colours, with readable text and a visible edge. */
export function swatchStyle(colours: Colours | string | null): React.CSSProperties {
  const hexes = typeof colours === "string" ? [colours] : colours ? all(colours) : [];
  if (hexes.length === 0) return { background: "#E4E4DF", color: INK, borderColor: "#C9C9C3" };
  const gradual = typeof colours === "object" && colours !== null && colours.gradual === true;
  // Text sits at the top left, over the first colour of a blend, or over all of a banded one.
  const light = hexes.length > 1 && !gradual ? hexes.every(isLight) : isLight(hexes[0]);
  const style: React.CSSProperties = { background: fill(hexes, gradual, 135), color: light ? INK : "#FFFFFF", borderColor: light ? "#C9C9C3" : hexes[0] };
  // Over several colours of mixed brightness neither ink reads everywhere, so the text gets an outline.
  if (hexes.length > 1 && hexes.some(isLight) !== hexes.every(isLight)) {
    style.color = "#FFFFFF";
    style.textShadow = "0 0 2px rgba(0, 0, 0, 0.7), 0 1px 3px rgba(0, 0, 0, 0.5)";
    style.borderColor = "#C9C9C3";
  }
  return style;
}

/** Styles for a small colour dot; pale colours get an edge so they show on white. */
export function dotStyle(colours: Colours | string | null): React.CSSProperties {
  const hexes = typeof colours === "string" ? [colours] : colours ? all(colours) : [];
  if (hexes.length === 0) return { background: "#E4E4DF", borderStyle: "dashed", borderColor: "#8B8F94" };
  const gradual = typeof colours === "object" && colours !== null && colours.gradual === true;
  return { background: fill(hexes, gradual, 135), borderColor: hexes.some(isLight) ? "#B5B5AF" : hexes[0] };
}
