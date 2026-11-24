import { useEffect, useRef } from "react";

/** Calls `onNear` when the element the returned ref is attached to scrolls into view. */
export function useNearEnd(onNear: () => void) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) onNear();
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [onNear]);
  return ref;
}
