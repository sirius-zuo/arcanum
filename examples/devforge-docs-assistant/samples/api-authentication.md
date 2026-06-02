# API Authentication

Devforge supports two authentication methods: API keys and OAuth2.

## API Keys

Include your API key in the `Authorization` header of every request:

```
Authorization: Bearer <your-api-key>
```

API keys are scoped to specific collections. Generate one in the dashboard under
Settings → API Keys. Keys expire after 90 days by default.

**Common errors:**
- `401 invalid_api_key` — key is missing, malformed, or expired
- `403 insufficient_scope` — key lacks permission for the requested collection

## OAuth2

Use the OAuth2 authorization code flow for user-facing integrations:

1. Redirect the user to `https://api.devforge.io/oauth/authorize?client_id=...&scope=...`
2. Handle the callback at your redirect URI — exchange the code for a token
3. Use the access token as a Bearer token in all API requests

Token refresh: access tokens expire in 1 hour. Use the refresh token to obtain a new one
without prompting the user again. Refresh tokens expire after 30 days.
