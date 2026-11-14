# push-relay

The relay Atlas Crew servers notify through (`push.codeatlas.com.br`): it holds the Firebase credentials, so a crew server needs none. It forwards **only the event kind** (`task_done` or `needs_you`) to the FCM token, never content, and has no accounts and no database.

## API

| Endpoint | Body | Answer |
|---|---|---|
| `POST /v1/register` | `{token}` | `200 {"grant": "<base64url>"}` (`400` bad body, `429` per-IP limit) |
| `POST /v1/notify` | `{token, kind, grant}` | `204` sent, `403` grant does not match, `410` FCM says the token is gone, `429` limit, `502` FCM/upstream error |

**Grants.** `grant = base64url(HMAC-SHA256(K, token))`, a pure function of the server secret `K` and the token, so the relay stores nothing per user. The phone asks `/v1/register` for the grant when it gets its FCM token and gives `{token, grant}` to each of its crew servers (`push_register`); a crew server presents the grant on every notify and the relay recomputes it (constant-time compare). Without the grant, knowing a token is not enough to notify it. `/v1/register` proves nothing about the caller (there are no accounts); it only turns "knows the token" into "holds the grant", which is what the phone then shares with its own servers.

**Limits** (fixed one-minute windows): 60 notifies per token, 300 notifies per IP, 20 registrations per IP. The grant is checked before the per-token limit, so a stranger cannot burn someone's quota.

## Configuration

| Variable | |
|---|---|
| `FCM_PROJECT_ID` | required: the Firebase project |
| `RELAY_GRANT_KEY` | required: `K`, at least 16 bytes (`openssl rand -base64 32`). The relay refuses to start without it |
| `RELAY_GRANT_KEY_PREVIOUS` | optional: the key being retired. Grants made with it are still accepted; new grants never use it |
| `RELAY_TRUST_PROXY=1` | key the per-IP limits on the left-most `X-Forwarded-For` (set it behind Cloud Run or a load balancer, where every socket peer is the proxy). Off by default: the header is spoofable |
| `PORT` | listen port (default `8080`) |
| Google credentials | Application Default Credentials with the `firebase.messaging` scope |

**Rotating `K`.** Set `RELAY_GRANT_KEY_PREVIOUS` to the old key and `RELAY_GRANT_KEY` to a new one, deploy, and keep both while phones re-register (a phone re-registers on every launch and on FCM token refresh, so a few weeks of normal use is enough). Then drop `RELAY_GRANT_KEY_PREVIOUS`. A crew server that gets `403` drops that token; the phone registers again on its next launch.

## Develop

```bash
scripts/push-relay-check.sh    # gofmt, go vet, go test
```

Point crew servers at a self-hosted relay with `ATLAS_PUSH_RELAY=<url>` in their unit, and build the phone app with `--dart-define=ATLAS_PUSH_RELAY=<url>`.
