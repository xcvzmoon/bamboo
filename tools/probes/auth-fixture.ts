import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';
import * as v from 'valibot';

export const storageMechanismSchema = v.picklist([
  'cookie',
  'persistentCookie',
  'localStorage',
  'indexedDB',
]);
export type StorageMechanism = v.InferOutput<typeof storageMechanismSchema>;
export type AuthFixture = {
  origin: string;
  close: () => Promise<void>;
};

export async function startAuthFixture(): Promise<AuthFixture> {
  const token = randomUUID();
  const server = createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1');
    const parsed = v.safeParse(storageMechanismSchema, url.searchParams.get('mechanism'));
    if (!parsed.success) {
      response.writeHead(400).end('Invalid mechanism');
      return;
    }
    const mechanism = parsed.output;
    response.setHeader('Cache-Control', 'no-store');
    if (url.pathname === '/issue' && request.method === 'POST') {
      if (mechanism === 'cookie' || mechanism === 'persistentCookie') {
        response.setHeader(
          'Set-Cookie',
          `bamboo_fixture=${token}; Path=/; HttpOnly; SameSite=Lax${mechanism === 'persistentCookie' ? '; Max-Age=3600' : ''}`,
        );
      }
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify({ token }));
      return;
    }
    if (url.pathname === '/check') {
      const authenticated =
        mechanism === 'cookie' || mechanism === 'persistentCookie'
          ? request.headers.cookie
              ?.split(';')
              .some((cookie) => cookie.trim() === `bamboo_fixture=${token}`)
          : request.headers.authorization === `Bearer ${token}`;
      response
        .writeHead(authenticated ? 200 : 401)
        .end(authenticated ? 'Authenticated' : 'Signed out');
      return;
    }
    if (url.pathname !== '/login' && url.pathname !== '/protected') {
      response.writeHead(404).end('Not found');
      return;
    }
    const login = url.pathname === '/login';
    response.setHeader('Content-Type', 'text/html; charset=utf-8');
    response.end(`<!doctype html><title>Bamboo authentication fixture</title>
      <h1>${login ? 'Sign in to Bamboo fixture' : 'Checking session'}</h1>
      ${login ? '<button id="login">Sign in</button>' : ''}
      <script>
      const mechanism = ${JSON.stringify(mechanism)};
      function database() {
        return new Promise((resolve, reject) => {
          const request = indexedDB.open('bamboo-fixture', 1);
          request.onupgradeneeded = () => request.result.createObjectStore('sessions');
          request.onsuccess = () => resolve(request.result);
          request.onerror = () => reject(new Error('FIXTURE_INDEXEDDB'));
        });
      }
      async function store(value) {
        if (mechanism === 'localStorage') { localStorage.setItem('bamboo_fixture', value); }
        if (mechanism === 'indexedDB') {
          const db = await database();
          await new Promise((resolve, reject) => {
            const transaction = db.transaction('sessions', 'readwrite');
            transaction.objectStore('sessions').put(value, 'token');
            transaction.oncomplete = resolve;
            transaction.onerror = () => reject(new Error('FIXTURE_INDEXEDDB'));
          });
          db.close();
        }
      }
      async function read() {
        if (mechanism === 'localStorage') { return localStorage.getItem('bamboo_fixture'); }
        if (mechanism === 'indexedDB') {
          const db = await database();
          try {
            return await new Promise((resolve, reject) => {
              const request = db.transaction('sessions').objectStore('sessions').get('token');
              request.onsuccess = () => resolve(request.result);
              request.onerror = () => reject(new Error('FIXTURE_INDEXEDDB'));
            });
          } finally { db.close(); }
        }
        return '';
      }
      async function check() {
        try {
          const value = await read();
          const response = await fetch('/check?mechanism=' + mechanism, {headers: {Authorization: 'Bearer ' + value}});
          document.querySelector('h1').textContent = response.ok ? 'Authenticated' : 'Signed out';
        } catch { document.querySelector('h1').textContent = 'Storage unavailable'; }
      }
      ${
        login
          ? `document.querySelector('#login').onclick = async () => {
        const response = await fetch('/issue?mechanism=' + mechanism, {method: 'POST'});
        const {token} = await response.json();
        await store(token);
        location.href = '/protected?mechanism=' + mechanism;
      };`
          : 'check();'
      }
      </script>`);
  });
  try {
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    const { port } = v.parse(v.object({ port: v.number() }), server.address());
    return {
      origin: `http://127.0.0.1:${port}`,
      async close(): Promise<void> {
        server.closeAllConnections();
        await new Promise<void>((resolve, reject) => {
          server.close((error) => {
            if (error) {
              reject(error);
            } else {
              resolve();
            }
          });
        });
      },
    };
  } catch (error) {
    server.close();
    throw error;
  }
}
