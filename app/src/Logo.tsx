/** The small Toona mark, drawn on a 16-unit grid so it stays crisp at icon sizes. */
export function Logo({ size }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" style={{ flex: "0 0 auto" }}>
      <rect width="16" height="16" rx="3.6" fill="#15171A" />
      <rect x="4" y="4" width="6" height="2" rx="1" fill="#F0475B" />
      <rect x="2" y="7" width="8.75" height="2" rx="1" fill="#F2B70A" />
      <rect x="4" y="10" width="6" height="2" rx="1" fill="#39B9DB" />
      <polygon points="11.25,7.7 12.75,5 14.5,5 13.3,7.7" fill="#F1F1EE" />
      <polygon points="11.25,8.3 12.75,11 14.5,11 13.3,8.3" fill="#F1F1EE" />
    </svg>
  );
}
