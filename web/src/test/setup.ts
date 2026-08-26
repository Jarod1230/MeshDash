import '@testing-library/jest-dom/vitest';
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// jsdom has no layout, so it has no ResizeObserver either. The ground surface
// measures itself with one; without a stand-in every test that mounts the
// shell would die on a missing constructor rather than on anything real.
//
// It reports a window-sized box, because jsdom's own measurement is zero in
// every direction and a surface with no size draws nothing — which would make
// every test of the drawing pass for the wrong reason.
globalThis.ResizeObserver = class {
  constructor(private readonly notify: ResizeObserverCallback) {}

  observe(target: Element) {
    this.notify(
      [{ target, contentRect: { width: 1024, height: 768 } } as ResizeObserverEntry],
      this as unknown as ResizeObserver,
    );
  }

  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

// Each test gets a fresh document and a fresh token store; otherwise one test
// signing in would leave the next one authenticated.
afterEach(() => {
  cleanup();
  window.localStorage.clear();
});
