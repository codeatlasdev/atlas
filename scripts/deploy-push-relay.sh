#!/usr/bin/env bash
# Deploy services/push-relay to Cloud Run and put https://push.codeatlas.com.br in front of it.
#   scripts/deploy-push-relay.sh              deploy (idempotent: every step checks before it changes)
#   scripts/deploy-push-relay.sh --dry-run    print every command, change nothing (needs no login)
# docs/mobile.md "Bringing push up" and services/push-relay/README.md "Deploy" are the runbook.
#
# Needs: `gcloud auth login` as an owner of the project, CLOUDFLARE_API_TOKEN (Zone:DNS:Edit and
# Zone:Origin Rules:Edit on codeatlas.com.br) and curl, jq, openssl. Secret values are never
# printed: RELAY_GRANT_KEY goes from openssl into Secret Manager through a pipe, the Cloudflare
# token only into an Authorization header read from a curl config on stdin.
#
# Overrides (env): GCP_PROJECT (default: gcloud's project), FCM_PROJECT_ID (default: GCP_PROJECT; the
# Firebase project the app uses), REGION, SERVICE, PUSH_DOMAIN, CF_ZONE.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

dry=0
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    -h | --help) sed -n '2,/^set -e/p' "$0" | sed '$d;s/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown arg: $a" >&2; exit 2 ;;
  esac
done

die() { echo "✗ $*" >&2; exit 1; }
step() { echo; echo "== $*"; }
# Run (or, with --dry-run, print) a command that changes something.
run() { echo "+ $*"; [[ $dry == 1 ]] || "$@"; }
# A read-only probe: real in a deploy, an unknown ("absent") answer in a dry run so the plan
# always lists every mutating command, even before the first deploy.
probe() { if [[ $dry == 1 ]]; then return 1; else "$@" >/dev/null 2>&1; fi; }

REGION="${REGION:-southamerica-east1}" # closest to the users; Cloud Run domain mapping is NOT offered here (see the Cloudflare step)
SERVICE="${SERVICE:-push-relay}"
PUSH_DOMAIN="${PUSH_DOMAIN:-push.codeatlas.com.br}"
CF_ZONE="${CF_ZONE:-codeatlas.com.br}"
SA_NAME="push-relay"
SECRET="RELAY_GRANT_KEY"
REPO="push-relay" # Artifact Registry repository

PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null || true)}"
[[ -n "$PROJECT" ]] || die "no project: set GCP_PROJECT or run gcloud config set project <id>"
FCM_PROJECT_ID="${FCM_PROJECT_ID:-$PROJECT}"
SA="$SA_NAME@$PROJECT.iam.gserviceaccount.com"
IMAGE="$REGION-docker.pkg.dev/$PROJECT/$REPO/$SERVICE:$(git rev-parse --short=12 HEAD 2>/dev/null || date +%s)"

if [[ $dry == 0 ]]; then
  for c in gcloud curl jq openssl; do command -v "$c" >/dev/null || die "$c is required"; done
  : "${CLOUDFLARE_API_TOKEN:?CLOUDFLARE_API_TOKEN is not set}"
  gcloud auth print-access-token >/dev/null 2>&1 ||
    die "gcloud is not logged in: run  gcloud auth login  (as an owner of $PROJECT) and retry"
else
  echo "(dry run: nothing below is executed; reads count as 'absent' so every change is listed)"
fi
echo "project=$PROJECT fcm-project=$FCM_PROJECT_ID region=$REGION service=$SERVICE domain=$PUSH_DOMAIN"

step "1/7 APIs"
run gcloud services enable run.googleapis.com secretmanager.googleapis.com artifactregistry.googleapis.com \
  fcm.googleapis.com cloudbuild.googleapis.com --project "$PROJECT"
# FCM HTTP v1 is called in the *Firebase* project; when it is another project, enable it there too.
if [[ "$FCM_PROJECT_ID" != "$PROJECT" ]]; then
  run gcloud services enable fcm.googleapis.com --project "$FCM_PROJECT_ID"
fi

step "2/7 Service account (FCM send only)"
if ! probe gcloud iam service-accounts describe "$SA" --project "$PROJECT"; then
  run gcloud iam service-accounts create "$SA_NAME" --project "$PROJECT" \
    --display-name "Atlas push relay (sends FCM, reads its grant key)"
fi
# The narrowest predefined role that carries cloudmessaging.messages.create (the only call the
# relay makes); the project must be the Firebase project, where FCM HTTP v1 authorises the send.
# add-iam-policy-binding is idempotent.
run gcloud projects add-iam-policy-binding "$FCM_PROJECT_ID" --member "serviceAccount:$SA" \
  --role roles/firebasecloudmessaging.admin --condition None --quiet --format none

step "3/7 Grant key ($SECRET, created once, never printed)"
if ! probe gcloud secrets describe "$SECRET" --project "$PROJECT"; then
  echo "+ openssl rand -base64 32 | gcloud secrets create $SECRET --data-file=- --replication-policy=automatic --project $PROJECT"
  if [[ $dry == 0 ]]; then
    # Never rotated here: a new key invalidates every grant (README "Rotating K").
    openssl rand -base64 32 | gcloud secrets create "$SECRET" --data-file=- \
      --replication-policy=automatic --project "$PROJECT" >/dev/null
  fi
else
  echo "(exists: kept as is; rotating needs RELAY_GRANT_KEY_PREVIOUS, see services/push-relay/README.md)"
fi
run gcloud secrets add-iam-policy-binding "$SECRET" --project "$PROJECT" \
  --member "serviceAccount:$SA" --role roles/secretmanager.secretAccessor --condition None --quiet --format none

step "4/7 Image (Cloud Build from services/push-relay)"
if ! probe gcloud artifacts repositories describe "$REPO" --location "$REGION" --project "$PROJECT"; then
  run gcloud artifacts repositories create "$REPO" --repository-format docker --location "$REGION" \
    --project "$PROJECT" --description "Atlas push relay images"
fi
run gcloud builds submit services/push-relay --tag "$IMAGE" --project "$PROJECT" --region "$REGION"

step "5/7 Cloud Run service"
# min 0: idle costs nothing (a cold start is a few hundred ms on a static Go binary). max 3 caps cost
# and abuse; the relay is stateless but its rate limits are per instance, so keep the cap small.
# concurrency 80 (the default) is right for a handler that only waits on one FCM call. Public:
# anyone with a token may call it, the grant is the authorization. RELAY_TRUST_PROXY=1: every
# socket peer is Google's front end, the client IP is the left-most X-Forwarded-For.
run gcloud run deploy "$SERVICE" --image "$IMAGE" --project "$PROJECT" --region "$REGION" \
  --service-account "$SA" --allow-unauthenticated \
  --min-instances 0 --max-instances 3 --concurrency 80 --cpu 1 --memory 128Mi --timeout 30 \
  --set-env-vars "FCM_PROJECT_ID=$FCM_PROJECT_ID,RELAY_TRUST_PROXY=1" \
  --set-secrets "RELAY_GRANT_KEY=$SECRET:latest"

step "6/7 Cloudflare: $PUSH_DOMAIN -> Cloud Run"
# Why not a Cloud Run domain mapping: it is Preview ("not recommended for production") and not
# offered in $REGION. A global load balancer costs ~US\$18/month for one tiny service. So: a PROXIED
# CNAME to the service's run.app host (Cloudflare terminates TLS, hides the origin, adds DDoS
# protection) plus an Origin Rule that rewrites Host to the run.app host (Cloud Run routes by Host).
if [[ $dry == 1 ]]; then
  run_host="push-relay-<hash>.a.run.app"
  echo "+ gcloud run services describe $SERVICE --region $REGION --project $PROJECT --format 'value(status.url)'"
else
  run_host="$(gcloud run services describe "$SERVICE" --region "$REGION" --project "$PROJECT" --format 'value(status.url)')"
  run_host="${run_host#https://}"
fi
echo "origin: $run_host"

# Every call reads the token from a curl config on stdin, so it is never in argv or the output.
cf() { # cf METHOD PATH [json-body]
  local method="$1" path="$2" body="${3:-}"
  local args=(-sS --fail-with-body -X "$method" "https://api.cloudflare.com/client/v4$path" -H "Content-Type: application/json")
  [[ -z "$body" ]] || args+=(--data "$body")
  printf 'header = "Authorization: Bearer %s"\n' "$CLOUDFLARE_API_TOKEN" | curl -K - "${args[@]}"
}

dns_body="$(jq -nc --arg n "$PUSH_DOMAIN" --arg c "$run_host" '{type:"CNAME",name:$n,content:$c,proxied:true,ttl:1,comment:"Atlas push relay (Cloud Run)"}')"
rule_expr="http.host eq \"$PUSH_DOMAIN\""
rule="$(jq -nc --arg e "$rule_expr" --arg h "$run_host" \
  '{ref:"atlas_push_relay_host",description:"Atlas push relay: Cloud Run routes by Host",expression:$e,action:"route",action_parameters:{host_header:$h}}')"

if [[ $dry == 1 ]]; then
  echo "+ GET  /zones?name=$CF_ZONE"
  echo "+ GET  /zones/<zone>/dns_records?name=$PUSH_DOMAIN"
  echo "+ POST|PUT /zones/<zone>/dns_records[/<id>]  $dns_body"
  echo "+ GET  /zones/<zone>/rulesets/phases/http_request_origin/entrypoint"
  echo "+ PUT  /zones/<zone>/rulesets/phases/http_request_origin/entrypoint  (this rule merged with the existing ones) $rule"
else
  zone="$(cf GET "/zones?name=$CF_ZONE" | jq -r '.result[0].id // empty')"
  [[ -n "$zone" ]] || die "Cloudflare zone $CF_ZONE not found for this token"
  rec="$(cf GET "/zones/$zone/dns_records?name=$PUSH_DOMAIN" | jq -r '.result[0].id // empty')"
  if [[ -n "$rec" ]]; then
    echo "+ PUT /zones/$zone/dns_records/$rec (update $PUSH_DOMAIN)"
    cf PUT "/zones/$zone/dns_records/$rec" "$dns_body" >/dev/null
  else
    echo "+ POST /zones/$zone/dns_records (create $PUSH_DOMAIN)"
    cf POST "/zones/$zone/dns_records" "$dns_body" >/dev/null
  fi
  # The entrypoint ruleset holds ALL origin rules of the zone: PUT replaces them, so keep the others.
  ep="/zones/$zone/rulesets/phases/http_request_origin/entrypoint"
  existing="$(cf GET "$ep" 2>/dev/null | jq -c '.result.rules // []' 2>/dev/null || echo '[]')"
  [[ -n "$existing" ]] || existing='[]'
  merged="$(jq -nc --argjson old "$existing" --argjson rule "$rule" \
    '{rules: ([$old[] | select(.ref != $rule.ref) | del(.id,.version,.last_updated)] + [$rule])}')"
  echo "+ PUT $ep (origin rule atlas_push_relay_host)"
  cf PUT "$ep" "$merged" >/dev/null
fi

step "7/7 Smoke check https://$PUSH_DOMAIN"
out="$(mktemp)"
trap 'rm -f "$out"' EXIT
smoke() { # smoke PATH BODY: prints the HTTP status, keeps the body in $out
  curl -sS -m 20 -o "$out" -w '%{http_code}' -X POST "https://$PUSH_DOMAIN$1" \
    -H 'Content-Type: application/json' --data "$2" || echo 000
}
if [[ $dry == 1 ]]; then
  echo "+ curl -X POST https://$PUSH_DOMAIN/v1/register -d '{\"token\":\"smoke-<id>\"}'   expect 200 with a grant"
  echo "+ curl -X POST https://$PUSH_DOMAIN/v1/notify -d '{\"token\":\"smoke-<id>\",\"kind\":\"task_done\",\"grant\":\"bad\"}'   expect 403"
  echo "(dry run done)"
  exit 0
fi
tok="smoke-$(openssl rand -hex 8)"
ok=0
# DNS and Cloudflare's certificate can take a minute on the first deploy.
for _ in $(seq 1 18); do
  code="$(smoke /v1/register "{\"token\":\"$tok\"}")"
  if [[ "$code" == 200 ]]; then ok=1; break; fi
  echo "  register -> $code, retrying in 10 s"; sleep 10
done
[[ $ok == 1 ]] || { die "register never answered 200 (last: $code); gcloud run services logs read $SERVICE --region $REGION"; }
jq -e '.grant | type == "string" and length > 0' <"$out" >/dev/null || { die "register answered 200 without a grant"; }
echo "✓ POST /v1/register -> 200 + grant"
code="$(smoke /v1/notify "{\"token\":\"$tok\",\"kind\":\"task_done\",\"grant\":\"bad\"}")"
[[ "$code" == 403 ]] || die "POST /v1/notify with a bad grant answered $code, expected 403"
echo "✓ POST /v1/notify with a bad grant -> 403"
echo "✓ push relay is up at https://$PUSH_DOMAIN"
