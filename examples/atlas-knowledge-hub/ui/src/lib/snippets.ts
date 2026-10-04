export interface SnippetInput {
  origin: string
  apiKey: string
  collection: string
  mcpEndpoint: string
}

function curl(input: SnippetInput, path: string, body: unknown, flags = ''): string {
  const json = JSON.stringify(body, null, 2).replace(/'/g, "'\\''")
  return [
    `curl ${flags ? `${flags} ` : ''}${input.origin}${path} \\`,
    `  -H 'Authorization: Bearer ${input.apiKey}' \\`,
    `  -H 'Content-Type: application/json' \\`,
    `  -d '${json}'`,
  ].join('\n')
}

export function contextSnippet(i: SnippetInput): string {
  return curl(i, '/api/v1/context', { collection_id: i.collection, query: 'How many vacation days do I get?', token_budget: 2000, render: 'xml' })
}

export function generateSnippet(i: SnippetInput): string {
  return curl(i, '/api/v1/generate', { collection_id: i.collection, query: 'How many vacation days do I get?', stream: false, verify: true })
}

/** Server-sent events: `-N` turns curl output buffering off so tokens appear as they arrive. */
export function generateStreamSnippet(i: SnippetInput): string {
  return curl(i, '/api/v1/generate', { collection_id: i.collection, query: 'How many vacation days do I get?', stream: true }, '-N')
}

/** `passages` come from a Context response: each `ref_id` with its `chunk_ids`. */
export function verifySnippet(i: SnippetInput): string {
  return curl(i, '/api/v1/verify', {
    collection_id: i.collection,
    answer: 'Employees get 25 vacation days [P1].',
    passages: [{ ref_id: 'P1', chunk_ids: ['<chunk id from a Context response>'] }],
  })
}

export function mcpConfigSnippet(i: SnippetInput): string {
  return JSON.stringify({ mcpServers: { atlas: { type: 'http', url: i.mcpEndpoint, headers: { Authorization: `Bearer ${i.apiKey}` } } } }, null, 2)
}

export function mcpListSnippet(i: SnippetInput): string {
  return [
    `curl ${i.mcpEndpoint} \\`,
    `  -H 'Content-Type: application/json' \\`,
    `  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'`,
  ].join('\n')
}
