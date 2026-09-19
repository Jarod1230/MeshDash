import { afterEach, describe, expect, it, vi } from 'vitest';
import { notify } from './useAlerts';

/** Stellt sich als Browser mit (oder ohne) Erlaubnis. */
function browserWith(permission: string) {
  const made: string[] = [];
  class FakeNotification {
    static permission = permission;
    constructor(_title: string, options?: { body?: string }) {
      made.push(options?.body ?? '');
    }
  }
  vi.stubGlobal('Notification', FakeNotification);

  return made;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('notify', () => {
  it('meldet sich, wenn es darf', () => {
    const made = browserWith('granted');

    notify('J-HomeNode ist still.');

    expect(made).toEqual(['J-HomeNode ist still.']);
  });

  it('fragt nicht ungefragt nach der Erlaubnis', () => {
    const made = browserWith('default');

    notify('J-HomeNode ist still.');

    expect(made).toEqual([]);
  });

  it('reißt nichts mit, wo es keine Benachrichtigungen gibt', () => {
    vi.stubGlobal('Notification', undefined);

    expect(() => notify('egal')).not.toThrow();
  });
});
