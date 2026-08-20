import type { ComponentType } from 'react';

/**
 * A frontend module. Mirrors a backend module of the same name.
 *
 * Modules register themselves in `./index.ts`. Removing a module means
 * deleting its directory and one line from that registry — if anything else
 * breaks, the cut is wrong. See docs/module-system.md.
 */
export interface UiModule {
  /** Matches the backend module name and the `/api/v1/<id>/…` route prefix. */
  readonly id: string;
  /** Label shown in the navigation. German, see ADR-0004. */
  readonly title: string;
  /** One line saying what the page answers. Shown as the page's subtitle. */
  readonly summary: string;
  /** Path this module mounts at, relative to the app root. */
  readonly path: string;
  /** The module's page. */
  readonly component: ComponentType;
}
