import type { UiModule } from './types';
import { systemModule } from './system';
import { nodesModule } from './nodes';
import { messagesModule } from './messages';
import { telemetryModule } from './telemetry';
import { settingsModule } from './settings';

/**
 * The module registry.
 *
 * A new module appends exactly one entry here and adds nothing anywhere else.
 * The order is the order of the navigation.
 */
export const modules: readonly UiModule[] = [
  systemModule,
  nodesModule,
  messagesModule,
  telemetryModule,
  settingsModule,
];

export type { UiModule };
