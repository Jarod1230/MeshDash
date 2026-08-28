import type { UiModule } from '../types';
import { SettingsPage } from './SettingsPage';

export const settingsModule: UiModule = {
  id: 'settings',
  title: 'Einstellungen',
  summary: 'Was sich ändern lässt, ohne den Dienst anzufassen',
  path: '/einstellungen',
  component: SettingsPage,
};
