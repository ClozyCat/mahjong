import { afterEach, describe, expect, it, vi } from 'vitest';

import { createTable, getHealth } from './api';

describe('api helpers', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('posts to /api/tables and returns the parsed table snapshot', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          table_code: 'AB12CD',
          phase: 'waiting',
          created_at: '2026-03-26T06:00:00Z',
          seats: [],
        }),
        {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        },
      ),
    );
    vi.stubGlobal('fetch', fetchMock);

    const result = await createTable('http://localhost:8080');

    expect(fetchMock).toHaveBeenCalledWith('http://localhost:8080/api/tables', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ test_mode: false }),
    });
    expect(result.table_code).toBe('AB12CD');
  });

  it('sends a requested table code when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          table_code: 'ROOM42',
          phase: 'waiting',
          created_at: '2026-03-26T06:00:00Z',
          seats: [],
        }),
        {
          status: 201,
          headers: { 'Content-Type': 'application/json' },
        },
      ),
    );
    vi.stubGlobal('fetch', fetchMock);

    await createTable('http://localhost:8080', 'ROOM42', true);

    expect(fetchMock).toHaveBeenCalledWith('http://localhost:8080/api/tables', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ table_code: 'ROOM42', test_mode: true }),
    });
  });

  it('reads the health endpoint', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ status: 'ok' }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );

    await expect(getHealth('http://localhost:8080')).resolves.toEqual({ status: 'ok' });
  });

  it('throws a readable error on failed responses', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ detail: 'boom' }), {
          status: 500,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );

    await expect(createTable('http://localhost:8080')).rejects.toThrow(/500/i);
  });

  it('surfaces status and detail for conflict responses', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ detail: 'table_code_exists' }), {
          status: 409,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );

    await expect(createTable('http://localhost:8080', 'ROOM42', false)).rejects.toMatchObject({
      status: 409,
      detail: { detail: 'table_code_exists' },
    });
  });
});
