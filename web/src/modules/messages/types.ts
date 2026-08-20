/** What `/api/v1/messages/received` answers. */
export interface DirectMessage {
  readonly id: number;
  /** Six bytes of the sender's key — a prefix, not an identity. */
  readonly sender_prefix: string;
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
