import type { UiModule } from '../types';
import { TelemetryPage } from './TelemetryPage';

export const telemetryModule: UiModule = {
  id: 'telemetry',
  title: 'Telemetrie',
  summary: 'Batterie und Empfangsqualität über die Zeit',
  path: '/telemetrie',
  component: TelemetryPage,
};
