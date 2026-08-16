import type { UiModule } from './types';

/**
 * The module registry.
 *
 * Empty on purpose: step 1 of docs/roadmap.md builds the scaffolding, step 6
 * adds the first modules (`system`, `nodes`, `messages`, `telemetry`).
 *
 * A new module appends exactly one entry here and adds nothing anywhere else.
 */
export const modules: readonly UiModule[] = [];

export type { UiModule };
