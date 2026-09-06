import { conversationTitle, type Channel, type Conversation, type Partner } from './types';

/** Which of the two message areas is shown. */
export type Area = 'direkt' | 'kanaele';

/** Maps the URL value to a conversation partner kind. */
export function partnerForArea(area: Area): Partner {
  return area === 'direkt' ? 'contact' : 'channel';
}

/** Parses the area from the address; unknown values fall back to Direkt. */
export function parseArea(value: string | null): Area {
  return value === 'kanaele' ? 'kanaele' : 'direkt';
}

/**
 * The conversations that belong to one area.
 *
 * Direct messages and channels stay separate on purpose: a channel has no
 * named sender the interface could show, so merging them into one list would
 * put a blank column on half the rows.
 */
export function conversationsForArea(
  conversations: readonly Conversation[],
  area: Area,
): readonly Conversation[] {
  const partner = partnerForArea(area);
  return conversations.filter((conversation) => conversation.partner === partner);
}

/**
 * The threads a search matches, by title, id, or last text.
 *
 * Filtered here rather than by the API: the conversation list arrives whole
 * and is a few hundred rows at most, so a round trip per keystroke would buy
 * nothing. The flat receive endpoints still accept `?q=`; this is the
 * conversation-side counterpart until the backend grows one.
 */
export function matchingThreads(
  conversations: readonly Conversation[],
  search: string,
): readonly Conversation[] {
  const needle = search.trim().toLowerCase();
  if (needle === '') return conversations;

  return conversations.filter((conversation) => {
    const title = conversationTitle(conversation).toLowerCase();
    return (
      title.includes(needle) ||
      conversation.id.toLowerCase().includes(needle) ||
      conversation.last_text.toLowerCase().includes(needle) ||
      (conversation.name !== null && conversation.name.toLowerCase().includes(needle))
    );
  });
}

/**
 * Channel threads, including channels that have never carried a message.
 *
 * A channel you have not written to yet is still a place to write — without
 * this, the only way to send into a quiet channel would be a global form at
 * the top, which is exactly what this redesign removes.
 */
export function channelThreads(
  conversations: readonly Conversation[],
  channels: readonly Channel[],
): readonly Conversation[] {
  const existing = conversationsForArea(conversations, 'kanaele');
  const known = new Set(existing.map((conversation) => conversation.id));
  const extras: Conversation[] = [];

  for (const channel of channels) {
    const id = String(channel.channel_index);
    if (known.has(id)) continue;
    extras.push({
      partner: 'channel',
      id,
      name: channel.name === '' ? null : channel.name,
      candidates: 0,
      public_key: null,
      last_text: '',
      last_at: channel.seen_at,
      last_direction: 'received',
      messages: 0,
    });
  }

  // Known threads first (API order is most recent), then quiet channels by index.
  return [...existing, ...extras.sort((a, b) => Number(a.id) - Number(b.id))];
}

/**
 * Finds the open thread in a list, or null when the address points nowhere.
 */
export function findThread(
  conversations: readonly Conversation[],
  id: string | null,
): Conversation | null {
  if (id === null || id === '') return null;
  return conversations.find((conversation) => conversation.id === id) ?? null;
}
