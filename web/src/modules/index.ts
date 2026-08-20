import type { UiModule } from './types';
import { systemModule } from './system';

/**
 * The module registry.
 *
 * A new module appends exactly one entry here and adds nothing anywhere else.
 * The order is the order of the navigation.
 */
export const modules: readonly UiModule[] = [systemModule];

export type { UiModule };
