const INK = "#15171A";

/** Whether dark text reads better than white on this colour. */
export function isLight(hex: string): boolean {
  const n = parseInt(hex.slice(1), 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 > 0.6;
}

/** Styles for a block filled with a filament colour, with readable text and a visible edge. */
export function swatchStyle(hex: string | null): React.CSSProperties {
  if (!hex) return { background: "#E4E4DF", color: INK, borderColor: "#C9C9C3" };
  const light = isLight(hex);
  return { background: hex, color: light ? INK : "#FFFFFF", borderColor: light ? "#C9C9C3" : hex };
}

/** Styles for a small colour dot; pale colours get an edge so they show on white. */
export function dotStyle(hex: string | null): React.CSSProperties {
  if (!hex) return { background: "#E4E4DF", borderStyle: "dashed", borderColor: "#8B8F94" };
  return { background: hex, borderColor: isLight(hex) ? "#B5B5AF" : hex };
}
