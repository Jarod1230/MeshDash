import type { UiModule } from '../types';
import { NodesPage } from './NodesPage';

export const nodesModule: UiModule = {
  id: 'nodes',
  title: 'Knoten',
  summary: 'Wer ist im Mesh, wie weit weg und wann zuletzt gehört',
  path: '/knoten',
  component: NodesPage,
};
