import { useCallback, useEffect, useState } from 'react';

/** How large an element is, in CSS pixels. */
export interface Size {
  readonly width: number;
  readonly height: number;
}

/**
 * Measures an element and keeps measuring it.
 *
 * The ground surface is drawn to fit the window, so it has to know how large
 * that is — and a map that keeps its old dimensions after the window is
 * resized puts every node in the wrong place.
 *
 * Returns a callback ref rather than a ref object so the observer attaches
 * the moment the element exists, including when it is swapped out.
 */
export function useSize(): [(element: Element | null) => void, Size] {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  const [element, setElement] = useState<Element | null>(null);

  const attach = useCallback((next: Element | null) => setElement(next), []);

  useEffect(() => {
    if (element === null) return;

    const observer = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect;
      if (box === undefined) return;
      setSize({ width: box.width, height: box.height });
    });
    observer.observe(element);

    return () => observer.disconnect();
  }, [element]);

  return [attach, size];
}
