/** What `/api/v1/messages/received` answers. */
export interface DirectMessage {
  readonly id: number;
  /** Six bytes of the sender's key — a prefix, not an identity. */
  readonly sender_prefix: string;
  /** The sender's name, when exactly one known contact has this prefix. */
  readonly sender_name: string | null;
  /** How many known contacts share this prefix. */
  readonly sender_candidates: number;
  readonly text: string;
  readonly text_type: number;
  readonly snr: number | null;
  readonly path_len: number | null;
  readonly sent_at: number;
  readonly received_at: string;
}

/** What `/api/v1/messages/channel-received` answers. */
export interface ChannelMessage {
  readonly id: number;
  readonly channel_index: number;
  readonly text: string;
  readonly text_type: number;
  readonly snr: number | null;
  readonly path_len: number | null;
  readonly sent_at: number;
  readonly received_at: string;
}

/** What `/api/v1/messages/channels` answers. */
export interface Channel {
  readonly channel_index: number;
  readonly name: string;
  readonly seen_at: string;
}

/** What the node reports after taking a direct message. */
export interface SendResult {
  readonly flooded: boolean;
  readonly expected_ack: string | null;
  readonly estimated_timeout_ms: number;
}

/** One side of a conversation. */
export type Partner = 'contact' | 'channel';

/** Which way a message went. */
export type Direction = 'received' | 'sent';

/** What `/api/v1/messages/conversations` answers. */
export interface Conversation {
  readonly partner: Partner;
  /** Key prefix, or channel index as a string. */
  readonly id: string;
  readonly name: string | null;
  /** For a contact: how many known contacts share this prefix. */
  readonly candidates: number;
  /** The contact's full key, when the prefix resolves to exactly one. */
  readonly public_key: string | null;
  readonly last_text: string;
  readonly last_at: string;
  readonly last_direction: Direction;
  readonly messages: number;
}

/** What `/api/v1/messages/conversation` answers, oldest first. */
export interface ConversationMessage {
  readonly direction: Direction;
  readonly text: string;
  readonly at: string;
  readonly snr: number | null;
  readonly stations: number | null;
  readonly flooded: boolean | null;
}

/** How a conversation is labelled, given what is known about it. */
export function conversationTitle(conversation: Conversation): string {
  if (conversation.name !== null) return conversation.name;
  if (conversation.partner === 'channel') return `Kanal ${conversation.id}`;
  // Several contacts share this prefix, so no name can be claimed.
  if (conversation.candidates > 1) return `${conversation.id} — mehrdeutig`;
  return conversation.id;
}
