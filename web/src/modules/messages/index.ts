import type { UiModule } from '../types';
import { MessagesPage } from './MessagesPage';

export const messagesModule: UiModule = {
  id: 'messages',
  title: 'Nachrichten',
  summary: 'Was hereinkam, und was hinausgeht',
  path: '/nachrichten',
  component: MessagesPage,
};
