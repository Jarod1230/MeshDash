import { useEffect, useRef, useState } from 'react';
import { apiObjectUrl } from '../lib/api';
import type { TileAt } from './projection';

/**
 * How many tiles are kept before the oldest are let go.
 *
 * Every kept tile is an object URL holding an image in memory. Two screenfuls
 * at a typical size is around a hundred; four hundred leaves room to pan back
 * and forth over a region without refetching, and stops a long session from
 * growing without bound.
 */
const KEEP = 400;

/**
 * How long a tile that failed is left alone before it is tried again.
 *
 * A source can be briefly unreachable, or refuse under load. Never asking
 * again would leave a hole in the map for the life of the page; asking on
 * every redraw would hammer whoever just said no.
 */
const RETRY_AFTER = 30_000;

/** The key a tile is cached under, and the path it is fetched from. */
function keyOf(tile: TileAt): string {
  return `${tile.z}/${tile.x}/${tile.y}`;
}

/**
 * Fetches the tiles a view needs and hands back what has arrived.
 *
 * Tiles come through the API like everything else, because the API is guarded
 * as a whole and an `<img>` cannot carry a token. What arrives is kept as an
 * object URL; the browser's own HTTP cache means a tile that was seen before
 * costs nothing on the wire even after this cache has let it go.
 *
 * A tile that fails is remembered as failed. Retrying it on every render would
 * hammer a source that just said no — the map redraws on every mouse move
 * while it is being dragged.
 *
 * # Two copies of the same thing, on purpose
 *
 * `owned` is the truth: insertion-ordered, and the thing whose URLs have to be
 * revoked. The returned map is a snapshot in state, because a component may
 * only read state while rendering — a ref read during render is a value React
 * never promised to be current.
 */
export function useTiles(tiles: readonly TileAt[], enabled: boolean): ReadonlyMap<string, string> {
  const owned = useRef(new Map<string, string>());
  const started = useRef(new Set<string>());
  // When a tile last failed, so it can be tried again rather than written off
  // for the life of the page — a source that was briefly unreachable is not a
  // source that has no tile.
  const failed = useRef(new Map<string, number>());
  const alive = useRef(true);
  const [ready, setReady] = useState<ReadonlyMap<string, string>>(new Map());

  const wanted = tiles.map(keyOf).join(' ');

  useEffect(() => {
    if (!enabled) return;

    // Deliberately without a per-run cancellation flag.
    //
    // The view changes on every frame of a drag, so this effect restarts on
    // every frame. Cancelling the run before it would abandon requests that
    // were already in flight — and because the key stays marked as started,
    // those tiles are never asked for again. That is a stretch of map that
    // stays blank until the page is reloaded, which is what a real drag on a
    // real map produced. A tile is a tile wherever the view has moved to
    // since; there is nothing to cancel.
    //
    // Started in a microtask: every setState below happens after an await, and
    // deferring the start makes that explicit rather than silenced.
    queueMicrotask(async () => {
      for (const key of wanted.split(' ').filter((one) => one !== '')) {
        if (!alive.current) return;
        if (started.current.has(key)) continue;

        const failedAt = failed.current.get(key);
        if (failedAt !== undefined && Date.now() - failedAt < RETRY_AFTER) continue;

        // Marked before the request goes out, so a redraw mid-flight does not
        // ask for the same tile a second time.
        started.current.add(key);

        let url: string;
        try {
          url = await apiObjectUrl(`/tiles/${key}`);
        } catch {
          started.current.delete(key);
          failed.current.set(key, Date.now());
          continue;
        }

        if (!alive.current) {
          URL.revokeObjectURL(url);
          return;
        }

        failed.current.delete(key);
        owned.current.set(key, url);

        // Oldest first: a Map iterates in insertion order.
        while (owned.current.size > KEEP) {
          const oldest = owned.current.keys().next().value;
          if (oldest === undefined) break;
          const stale = owned.current.get(oldest);
          if (stale !== undefined) URL.revokeObjectURL(stale);
          owned.current.delete(oldest);
          started.current.delete(oldest);
        }

        setReady(new Map(owned.current));
      }
    });
  }, [wanted, enabled]);

  // On unmount, let go of every image at once — and forget everything else
  // along with them.
  //
  // The three refs outlive an unmount, the object URLs do not. Revoking the
  // URLs while leaving `started` full means the tiles are never fetched again
  // and every `<image>` points at a blob that no longer exists: elements in
  // the DOM, in the right place, painting nothing. React's development mode
  // mounts every component twice, so this is not a corner case — it is what
  // happens on the first page load.
  useEffect(() => {
    const urls = owned.current;
    const asked = started.current;
    const broken = failed.current;
    alive.current = true;

    return () => {
      alive.current = false;
      for (const url of urls.values()) {
        URL.revokeObjectURL(url);
      }
      urls.clear();
      asked.clear();
      broken.clear();
      setReady(new Map());
    };
  }, []);

  return ready;
}

export { keyOf as tileKey };
