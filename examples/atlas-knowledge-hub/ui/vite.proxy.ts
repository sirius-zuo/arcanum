import type { ProxyOptions } from 'vite'

const target = 'http://localhost:8080'

/**
 * Keys starting with `^` are regexes in Vite. Anchoring on a trailing slash keeps the
 * SPA routes `/admin` and `/evidence?chunk=...` out of the proxy on a hard reload.
 */
export const proxy: Record<string, string | ProxyOptions> = {
  '^/api/': target,
  '^/admin/': target,
  '^/evidence/': target,
  '^/demo/': target,
  '^/ws/': { target, ws: true },
  '^/(health|ready)$': target,
}
