/**
 * The handful of push opcodes the interface needs in order to know what a
 * live event is about.
 *
 * Values from `meshdash_proto::opcode::Push`, verified against the firmware
 * source (MeshCore commit d929643) and listed in
 * docs/research/meshcore-companion-protocol.md. Nothing here is guessed, and
 * nothing beyond these three belongs here: decoding payloads is the backend's
 * job, and the browser only needs to know which page to nudge.
 */
const PUSH_ADVERT = 0x80;
const PUSH_NEW_ADVERT = 0x8a;
const PUSH_MSG_WAITING = 0x83;

/** The opcode of a push event, read from the leading byte of its hex payload. */
export function pushOpcode(payload: string | undefined): number | null {
  if (payload === undefined || payload.length < 2) return null;
  const opcode = Number.parseInt(payload.slice(0, 2), 16);
  return Number.isNaN(opcode) ? null : opcode;
}

/** Whether a push announces that a node was heard. */
export function isAdvert(payload: string | undefined): boolean {
  const opcode = pushOpcode(payload);
  return opcode === PUSH_ADVERT || opcode === PUSH_NEW_ADVERT;
}

/** Whether a push announces that messages are waiting to be fetched. */
export function isMessageWaiting(payload: string | undefined): boolean {
  return pushOpcode(payload) === PUSH_MSG_WAITING;
}
