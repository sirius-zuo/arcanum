# SDK Quickstart

Get your first API call working in under 5 minutes.

## Install

```bash
npm install @devforge/sdk
# or
pip install devforge-sdk
```

## Configure

```typescript
import { DevforgeClient } from '@devforge/sdk'

const client = new DevforgeClient({
  apiKey: process.env.DEVFORGE_API_KEY,
  baseUrl: 'https://api.devforge.io',
})
```

## Make your first call

```typescript
const response = await client.collections.list()
console.log(response.collections)
```

## Hello World

```typescript
const result = await client.search.query({
  query: 'getting started',
  collectionId: 'my-docs',
  topK: 5,
})

for (const chunk of result.chunks) {
  console.log(chunk.text)
}
```

## Next steps

- [Authentication guide](./api-authentication.md)
- [Error reference](./error-reference.md)
- [Rate limiting](./rate-limiting.md)
