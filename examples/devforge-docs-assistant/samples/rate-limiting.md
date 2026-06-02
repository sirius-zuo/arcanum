# Rate Limiting

Devforge enforces rate limits per API key. Every response includes rate limit headers:

| Header | Description |
|---|---|
| `X-RateLimit-Limit` | Maximum requests allowed per window |
| `X-RateLimit-Remaining` | Requests remaining in the current window |
| `X-RateLimit-Reset` | Unix timestamp when the window resets |

## Default limits

- Free tier: 100 requests/minute, 10,000 requests/day
- Pro tier: 1,000 requests/minute, 500,000 requests/day
- Enterprise: custom limits

## Handling rate limit errors

When you receive a `429 rate_limit_exceeded` response, implement exponential backoff:

1. Wait `2^attempt × 100ms` before retrying (cap at 30 seconds)
2. Add ±10% jitter to prevent thundering herd
3. After 5 retries, surface the error to the user

Never retry immediately after a 429 — it will only worsen the situation.
