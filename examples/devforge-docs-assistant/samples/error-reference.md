# Error Reference

| Code | HTTP Status | Cause | Resolution |
|---|---|---|---|
| `invalid_api_key` | 401 | API key missing, malformed, or expired | Check the Authorization header; regenerate the key if expired |
| `insufficient_scope` | 403 | Key lacks permission for this collection | Generate a key with the correct scope |
| `rate_limit_exceeded` | 429 | Too many requests | Back off exponentially; check X-RateLimit-Reset |
| `collection_not_found` | 404 | The collection ID does not exist | Verify the collection name; create it if needed |
| `payload_too_large` | 413 | Request body exceeds 10 MB | Split large documents into smaller files |
| `internal_server_error` | 500 | Unexpected server error | Retry once; if persistent, contact support |

## Example error response

```json
{
  "error": "invalid_api_key",
  "message": "The provided API key has expired.",
  "docs": "https://docs.devforge.io/errors#invalid_api_key"
}
```

All errors include a `docs` URL with resolution steps.
