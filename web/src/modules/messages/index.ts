import type { UiModule } from '../types';
import { MessagesPage } from './MessagesPage';

export const messagesModule: UiModule = {
  id: 'messages',
  title: 'Nachrichten',
  summary: 'Direkt und Kanäle als Fäden — lesen und schreiben im Gespräch',
  path: '/nachrichten',
  component: MessagesPage,
};
