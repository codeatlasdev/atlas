package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"time"

	"golang.org/x/oauth2/google"
)

const (
	fcmScope        = "https://www.googleapis.com/auth/firebase.messaging"
	rateLimitPerMin = 60
)

// notifyReq is the inbound body for POST /v1/notify. Grant is what /v1/register returned for Token.
type notifyReq struct {
	Token string `json:"token"`
	Kind  string `json:"kind"`
	Grant string `json:"grant"`
}

// registerReq is the inbound body for POST /v1/register.
type registerReq struct {
	Token string `json:"token"`
}

var kindBody = map[string]string{
	"task_done": "A crewmate finished a task",
	"needs_you": "A crewmate needs you",
}

// relay serves /v1/register and /v1/notify: it issues and checks grants, rate-limits per token and
// per IP, and forwards to FCM.
type relay struct {
	fcmProject string
	fcmBase    string                 // default "https://fcm.googleapis.com"; overridden in tests
	getToken   func() (string, error) // nil skips Authorization header (tests)
	client     *http.Client

	keys       []string // grant keys: [current, previous?]; see grant.go
	trustProxy bool     // key per-IP limits on X-Forwarded-For (RELAY_TRUST_PROXY=1)

	perToken    *limiter
	perIP       *limiter
	perIPRegist *limiter
}

func newRelay(project, fcmBase string, getToken func() (string, error)) *relay {
	return &relay{
		fcmProject: project,
		fcmBase:    fcmBase,
		getToken:   getToken,
		client:     &http.Client{Timeout: 10 * time.Second},

		perToken:    newLimiter(rateLimitPerMin),
		perIP:       newLimiter(ipLimitPerMin),
		perIPRegist: newLimiter(registerLimitPerMin),
	}
}

// fcmErrorBody is the subset of the FCM HTTP v1 error response we inspect.
type fcmErrorBody struct {
	Error struct {
		Details []struct {
			Type      string `json:"@type"`
			ErrorCode string `json:"errorCode"`
		} `json:"details"`
	} `json:"error"`
}

// isUnregistered returns true when the FCM error body carries errorCode "UNREGISTERED"
// in a google.firebase.fcm.v1.FcmError detail entry.
func isUnregistered(body []byte) bool {
	var e fcmErrorBody
	if err := json.Unmarshal(body, &e); err != nil {
		return false
	}
	for _, d := range e.Error.Details {
		if d.Type == "type.googleapis.com/google.firebase.fcm.v1.FcmError" &&
			d.ErrorCode == "UNREGISTERED" {
			return true
		}
	}
	return false
}

// forward sends one FCM HTTP v1 message.
// Returns (fcmStatus, unregistered, err).
// unregistered is true only when FCM explicitly says the token is UNREGISTERED.
func (r *relay) forward(token, kind string) (int, bool, error) {
	msg := map[string]any{
		"message": map[string]any{
			"token": token,
			"notification": map[string]string{
				"title": "Atlas",
				"body":  kindBody[kind],
			},
			"data": map[string]string{"kind": kind},
		},
	}
	payload, _ := json.Marshal(msg)
	url := fmt.Sprintf("%s/v1/projects/%s/messages:send", r.fcmBase, r.fcmProject)

	req, err := http.NewRequest(http.MethodPost, url, bytes.NewReader(payload))
	if err != nil {
		return 0, false, err
	}
	req.Header.Set("Content-Type", "application/json")
	if r.getToken != nil {
		tok, err := r.getToken()
		if err != nil {
			return 0, false, fmt.Errorf("auth token: %w", err)
		}
		req.Header.Set("Authorization", "Bearer "+tok)
	}

	resp, err := r.client.Do(req)
	if err != nil {
		return 0, false, err
	}
	defer resp.Body.Close()

	// Read body only when we need to inspect it (non-2xx).
	if resp.StatusCode >= 200 && resp.StatusCode < 300 {
		io.Copy(io.Discard, resp.Body) //nolint:errcheck
		return resp.StatusCode, false, nil
	}

	respBody, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, isUnregistered(respBody), nil
}

// ServeHTTP routes POST /v1/register and POST /v1/notify.
func (r *relay) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if req.Method != http.MethodPost {
		http.NotFound(w, req)
		return
	}
	switch req.URL.Path {
	case "/v1/register":
		r.register(w, req)
	case "/v1/notify":
		r.notify(w, req)
	default:
		http.NotFound(w, req)
	}
}

// decode reads a small JSON body into v.
func decode(w http.ResponseWriter, req *http.Request, v any) bool {
	req.Body = http.MaxBytesReader(w, req.Body, 16<<10)
	if err := json.NewDecoder(req.Body).Decode(v); err != nil {
		http.Error(w, "bad request", http.StatusBadRequest)
		return false
	}
	return true
}

// register answers {"grant": ...} for a token. It proves nothing about the caller (the relay has no
// accounts): it only turns "knows the token" into "holds the grant", which the phone then shares
// with its own crew servers. Rate-limited per IP so it cannot be used to mass-mint.
func (r *relay) register(w http.ResponseWriter, req *http.Request) {
	if !r.perIPRegist.allow(clientIP(req, r.trustProxy)) {
		http.Error(w, "too many requests", http.StatusTooManyRequests)
		return
	}
	var body registerReq
	if !decode(w, req, &body) {
		return
	}
	if body.Token == "" || len(body.Token) > maxTokenLen {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"grant": r.grant(body.Token)}) //nolint:errcheck
}

// notify forwards one push after checking the caller holds the token's grant.
func (r *relay) notify(w http.ResponseWriter, req *http.Request) {
	if !r.perIP.allow(clientIP(req, r.trustProxy)) {
		http.Error(w, "too many requests", http.StatusTooManyRequests)
		return
	}
	var body notifyReq
	if !decode(w, req, &body) {
		return
	}
	if body.Token == "" || body.Kind == "" {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	if _, ok := kindBody[body.Kind]; !ok {
		http.Error(w, "bad request: unknown kind", http.StatusBadRequest)
		return
	}

	// Grant before the per-token limit: a stranger must not be able to burn a victim's quota.
	if !r.verify(body.Token, body.Grant) {
		http.Error(w, "forbidden", http.StatusForbidden)
		return
	}
	if !r.perToken.allow(body.Token) {
		http.Error(w, "too many requests", http.StatusTooManyRequests)
		return
	}

	code, unreg, err := r.forward(body.Token, body.Kind)
	if err != nil {
		log.Printf("fcm error: %v", err)
		http.Error(w, "upstream error", http.StatusBadGateway)
		return
	}

	log.Printf("notify kind=%s fcm_status=%d unregistered=%v", body.Kind, code, unreg)

	switch {
	case code >= 200 && code < 300:
		w.WriteHeader(http.StatusNoContent)
	case unreg:
		// Token explicitly marked UNREGISTERED by FCM: safe to drop.
		w.WriteHeader(http.StatusGone)
	default:
		// All other non-2xx (wrong project ID, auth error, quota, etc.) -> 502.
		http.Error(w, "fcm error", http.StatusBadGateway)
	}
}

// newGoogleTokenSource returns a function that vends Google OAuth2 access
// tokens via Application Default Credentials, refreshing automatically.
func newGoogleTokenSource(ctx context.Context) (func() (string, error), error) {
	creds, err := google.FindDefaultCredentials(ctx, fcmScope)
	if err != nil {
		return nil, fmt.Errorf("ADC: %w", err)
	}
	src := creds.TokenSource
	return func() (string, error) {
		tok, err := src.Token()
		if err != nil {
			return "", err
		}
		return tok.AccessToken, nil
	}, nil
}
