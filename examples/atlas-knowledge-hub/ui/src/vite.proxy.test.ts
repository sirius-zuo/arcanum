import { describe, expect, it } from 'vitest'
import { proxy } from '../vite.proxy'
import { ROUTES } from './routes'

const matches = (key: string, path: string) => (key.startsWith('^') ? new RegExp(key).test(path) : path.startsWith(key))
const proxied = (path: string) => Object.keys(proxy).some((k) => matches(k, path))

describe('dev proxy', () => {
  it('never_swallows_a_spa_route', () => {
    for (const r of ROUTES) expect(proxied(r.path), r.path).toBe(false)
  })

  it('proxies_representative_api_paths', () => {
    for (const p of ['/api/v1/search', '/admin/audit', '/evidence/chunk/abc', '/demo/bootstrap', '/ws/events', '/health', '/ready']) {
      expect(proxied(p), p).toBe(true)
    }
  })
})
