import { describe, expect, it } from 'vitest'
import { contextSnippet, generateSnippet, generateStreamSnippet, verifySnippet, mcpConfigSnippet } from './snippets'

const input = { origin: 'http://localhost:8080', apiKey: 'demo-key', collection: 'halcyon', mcpEndpoint: 'http://localhost:8081/mcp' }

describe('snippets', () => {
  it('use_origin_key_and_real_paths', () => {
    expect(contextSnippet(input)).toContain("curl http://localhost:8080/api/v1/context")
    expect(contextSnippet(input)).toContain("Authorization: Bearer demo-key")
    expect(generateSnippet(input)).toContain('"stream": false')
    expect(generateStreamSnippet(input)).toMatch(/^curl -N http:\/\/localhost:8080\/api\/v1\/generate/)
    expect(generateStreamSnippet(input)).toContain('"stream": true')
    expect(verifySnippet(input)).toContain('/api/v1/verify')
    expect(verifySnippet(input)).toContain('"passages"')
    expect(JSON.parse(mcpConfigSnippet(input)).mcpServers.atlas.url).toBe('http://localhost:8081/mcp')
    expect(JSON.parse(mcpConfigSnippet(input)).mcpServers.atlas.headers).toEqual({ Authorization: 'Bearer demo-key' })
  })
})
