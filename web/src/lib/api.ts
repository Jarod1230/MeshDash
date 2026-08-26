import { readToken } from './token';

/** Every API path is relative to this, matching the server's router. */
const BASE = '/api/v1';

/**
 * What went wrong with a request.
 *
 * `Unauthorized` is separate from the rest because it is the one failure the
 * user can fix: the token is missing or wrong.
 */
export type ApiError =
  | { kind: 'unauthorized' }
  | { kind: 'offline'; message: string }
  | { kind: 'server'; status: number; message: string };

/** The error body the backend sends, see docs/conventions.md. */
interface ErrorBody {
  error?: { code?: string; message?: string };
}

/** Turns whatever went wrong into a sentence for the interface. */
export function describeError(error: ApiError): string {
  switch (error.kind) {
    case 'unauthorized':
      return 'Das Token wird nicht akzeptiert.';
    case 'offline':
      return `Der Dienst antwortet nicht: ${error.message}`;
    case 'server':
      return error.message;
  }
}

/** Reads JSON from the API, or fails with a described error. */
export async function apiGet<T>(path: string): Promise<T> {
  return request<T>(path, { method: 'GET' });
}

/** Sends JSON to the API. Returns null for an empty (204) answer. */
export async function apiPost<T>(path: string, body: unknown): Promise<T | null> {
  return request<T | null>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

/** Replaces something through the API. Returns null for an empty (204) answer. */
export async function apiPut<T>(path: string, body: unknown): Promise<T | null> {
  return request<T | null>(path, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
  const token = readToken();
  const headers = new Headers(init.headers);
  if (token !== null) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  let response: Response;
  try {
    response = await fetch(`${BASE}${path}`, { ...init, headers });
  } catch (cause) {
    // A failed fetch means the service is unreachable, not that it said no.
    throw { kind: 'offline', message: String(cause) } satisfies ApiError;
  }

  if (response.status === 401) {
    throw { kind: 'unauthorized' } satisfies ApiError;
  }

  if (!response.ok) {
    throw {
      kind: 'server',
      status: response.status,
      message: await readErrorMessage(response),
    } satisfies ApiError;
  }

  if (response.status === 204) {
    return null as T;
  }

  return (await response.json()) as T;
}

/** Prefers the backend's own message over a bare status code. */
async function readErrorMessage(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as ErrorBody;
    if (typeof body.error?.message === 'string') {
      return body.error.message;
    }
  } catch {
    // Not every failure carries a JSON body — a proxy error page, for one.
  }
  return `Der Dienst antwortete mit ${response.status}.`;
}
