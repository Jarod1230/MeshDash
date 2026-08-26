import type { UiModule } from '../types';
import { SystemPage } from './SystemPage';

export const systemModule: UiModule = {
  id: 'system',
  title: 'Verbindung',
  summary: 'Verbindung zum Node und was er über sich sagt',
  path: '/verbindung',
  component: SystemPage,
};
