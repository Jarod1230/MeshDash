import '@testing-library/jest-dom/vitest';
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// Each test gets a fresh document and a fresh token store; otherwise one test
// signing in would leave the next one authenticated.
afterEach(() => {
  cleanup();
  window.localStorage.clear();
});
