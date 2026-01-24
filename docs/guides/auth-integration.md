# External Auth Provider Integration (TAS-150)

Integrating Tasker's API security with external identity providers via JWKS endpoints.

> See also: [Auth Documentation Hub](../auth/README.md) for architecture overview, [Configuration](../auth/configuration.md) for full TOML reference.

---

## JWKS Integration

Tasker supports JWKS (JSON Web Key Set) for dynamic public key discovery. This enables key rotation without redeploying Tasker.

### Configuration

```toml
[auth]
enabled = true
jwt_verification_method = "jwks"
jwks_url = "https://your-provider.com/.well-known/jwks.json"
jwks_refresh_interval_seconds = 3600
jwt_issuer = "https://your-provider.com/"
jwt_audience = "tasker-api"
permissions_claim = "permissions"  # or custom claim name
```

### How It Works

1. On first request, Tasker fetches the JWKS from the configured URL
2. Keys are cached for the configured refresh interval
3. When a token has an unknown `kid` (Key ID), a refresh is triggered
4. RSA keys are parsed from the JWK `n` and `e` components

---

## Auth0

### Auth0 Configuration

1. Create an API in Auth0 Dashboard:
   - Name: `Tasker API`
   - Identifier: `tasker-api` (this becomes the audience)
   - Signing Algorithm: RS256

2. Create permissions in the API settings matching Tasker's vocabulary:
   - `tasks:create`, `tasks:read`, `tasks:list`, etc.

3. Assign permissions to users/applications via Auth0 roles

### Tasker Configuration for Auth0

```toml
[auth]
enabled = true
jwt_verification_method = "jwks"
jwks_url = "https://YOUR_DOMAIN.auth0.com/.well-known/jwks.json"
jwks_refresh_interval_seconds = 3600
jwt_issuer = "https://YOUR_DOMAIN.auth0.com/"
jwt_audience = "tasker-api"
permissions_claim = "permissions"
```

### Token Request

```bash
curl --request POST \
  --url https://YOUR_DOMAIN.auth0.com/oauth/token \
  --header 'content-type: application/json' \
  --data '{
    "client_id": "YOUR_CLIENT_ID",
    "client_secret": "YOUR_CLIENT_SECRET",
    "audience": "tasker-api",
    "grant_type": "client_credentials"
  }'
```

---

## Keycloak

### Keycloak Configuration

1. Create a realm and client for Tasker
2. Define client roles matching Tasker permissions
3. Configure the client to include roles in the `permissions` token claim via a protocol mapper

### Tasker Configuration for Keycloak

```toml
[auth]
enabled = true
jwt_verification_method = "jwks"
jwks_url = "https://keycloak.example.com/realms/YOUR_REALM/protocol/openid-connect/certs"
jwks_refresh_interval_seconds = 3600
jwt_issuer = "https://keycloak.example.com/realms/YOUR_REALM"
jwt_audience = "tasker-api"
permissions_claim = "permissions"  # Configure via protocol mapper
```

---

## Okta

### Okta Configuration

1. Create an API authorization server
2. Add custom claims for permissions
3. Define scopes matching Tasker permissions

### Tasker Configuration for Okta

```toml
[auth]
enabled = true
jwt_verification_method = "jwks"
jwks_url = "https://YOUR_DOMAIN.okta.com/oauth2/YOUR_AUTH_SERVER_ID/v1/keys"
jwks_refresh_interval_seconds = 3600
jwt_issuer = "https://YOUR_DOMAIN.okta.com/oauth2/YOUR_AUTH_SERVER_ID"
jwt_audience = "tasker-api"
permissions_claim = "scp"  # Okta uses "scp" for scopes by default
```

---

## Custom JWKS Endpoint

Any provider that serves a standard JWKS endpoint works. The endpoint must return:

```json
{
  "keys": [
    {
      "kty": "RSA",
      "kid": "key-id-1",
      "use": "sig",
      "alg": "RS256",
      "n": "<base64url-encoded modulus>",
      "e": "<base64url-encoded exponent>"
    }
  ]
}
```

---

## Static Public Key (Development)

For development or simple deployments without a JWKS endpoint:

```toml
[auth]
enabled = true
jwt_verification_method = "public_key"
jwt_public_key_path = "/etc/tasker/keys/jwt-public-key.pem"
jwt_issuer = "tasker-core"
jwt_audience = "tasker-api"
```

Generate keys with:
```bash
tasker-cli auth generate-keys --output-dir /etc/tasker/keys
```

---

## Permission Claim Mapping

If your identity provider uses a different claim name for permissions:

```toml
permissions_claim = "custom_permissions"  # Default: "permissions"
```

The claim must be a JSON array of strings:
```json
{
  "custom_permissions": ["tasks:create", "tasks:read"]
}
```

---

## Strict Validation

When `strict_validation = true` (default), tokens containing unknown permission strings are rejected. Set to `false` if your provider includes additional scopes/permissions not in Tasker's vocabulary:

```toml
strict_validation = false
log_unknown_permissions = true  # Still log unknown permissions for monitoring
```
