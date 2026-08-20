import { describe, expect, it } from 'vitest';
import { isAdvert, isMessageWaiting, pushOpcode } from './pushes';

describe('push opcodes', () => {
  it('reads the opcode from the leading byte', () => {
    expect(pushOpcode('8acdcdcd')).toBe(0x8a);
  });

  it('recognises both advert forms', () => {
    // 0x80 carries only a key, 0x8A a whole contact — both mean "heard".
    expect(isAdvert('80aaaa')).toBe(true);
    expect(isAdvert('8acdcd')).toBe(true);
    expect(isAdvert('83')).toBe(false);
  });

  it('recognises the message bell', () => {
    expect(isMessageWaiting('83')).toBe(true);
    expect(isMessageWaiting('80aaaa')).toBe(false);
  });

  it('treats a missing or unreadable payload as no opcode', () => {
    expect(pushOpcode(undefined)).toBeNull();
    expect(pushOpcode('')).toBeNull();
    expect(pushOpcode('zz')).toBeNull();
  });
});
