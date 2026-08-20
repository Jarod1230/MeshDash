import type { UiModule } from '../types';
import { SystemPage } from './SystemPage';

export const systemModule: UiModule = {
  id: 'system',
  title: 'Übersicht',
  summary: 'Verbindung zum Node und was er über sich sagt',
  path: '/',
  component: SystemPage,
};
