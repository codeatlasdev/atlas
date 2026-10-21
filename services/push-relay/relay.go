package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"sync"
	"time"

	"golang.org/x/oauth2/google"
)

const (
	fcmScope        = "https://www.googleapis.com/auth/firebase.messaging"
	rateLimitPerMin = 60
)

// notifyReq is the inbound body for POST /v1/notify.
type notifyReq struct {
	Token string `json:"token"`
	Kind  string `json:"kind"`
}

var kindBody = map[string]string{
	"task_done": "A bot finished a task",
	"needs_you": "A bot needs you",
}

// bucket is a fixed-window counter (60 requests per 1-minute window).
type bucket struct {
	count     int
	windowEnd time.Time
}

// relay handles /v1/notify, rate-limits per token, and forwards to FCM.
type relay struct {
	fcmProject string
	fcmBase    string                 // default "https://fcm.googleapis.com"; overridden in tests
	getToken   func() (string, error) // nil skips Authorization header (tests)
	client     *http.Client

	mu      sync.Mutex
	buckets map[string]*bucket
}

func newRelay(project, fcmBase string, getToken func() (string, error)) *relay {
	return &relay{
		fcmProject: project,
		fcmBase:    fcmBase,
		getToken:   getToken,
		client:     &http.Client{Timeout: 10 * time.Second},
		buckets:    make(map[string]*bucket),
	}
}

// allow returns false and leaves the bucket unchanged when the token has
// exhausted its 60-request-per-minute allowance.
func (r *relay) allow(token string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	now := time.Now()
	b, ok := r.buckets[token]
	if !ok || now.After(b.windowEnd) {
		r.buckets[token] = &bucket{count: 1, windowEnd: now.Add(time.Minute)}
		return true
	}
	if b.count >= rateLimitPerMin {
		return false
	}
	b.count++
	return true
}

// forward sends one FCM HTTP v1 message and returns the FCM status code.
func (r *relay) forward(token, kind string) (int, error) {
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
		return 0, err
	}
	req.Header.Set("Content-Type", "application/json")
	if r.getToken != nil {
		tok, err := r.getToken()
		if err != nil {
			return 0, fmt.Errorf("auth token: %w", err)
		}
		req.Header.Set("Authorization", "Bearer "+tok)
	}

	resp, err := r.client.Do(req)
	if err != nil {
		return 0, err
	}
	defer resp.Body.Close()
	io.Copy(io.Discard, resp.Body) //nolint:errcheck
	return resp.StatusCode, nil
}

// ServeHTTP handles POST /v1/notify.
func (r *relay) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	if req.Method != http.MethodPost || req.URL.Path != "/v1/notify" {
		http.NotFound(w, req)
		return
	}

	var body notifyReq
	if err := json.NewDecoder(req.Body).Decode(&body); err != nil || body.Token == "" || body.Kind == "" {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	if _, ok := kindBody[body.Kind]; !ok {
		http.Error(w, "bad request: unknown kind", http.StatusBadRequest)
		return
	}

	if !r.allow(body.Token) {
		http.Error(w, "too many requests", http.StatusTooManyRequests)
		return
	}

	code, err := r.forward(body.Token, body.Kind)
	if err != nil {
		log.Printf("fcm error: %v", err)
		http.Error(w, "upstream error", http.StatusBadGateway)
		return
	}

	log.Printf("notify kind=%s fcm_status=%d", body.Kind, code)

	// Map FCM HTTP v1 outcomes to our contract:
	//   200/204 → 204 (sent)
	//   404     → 410 (token unregistered)
	//   anything else → 502
	switch {
	case code == http.StatusOK || code == http.StatusNoContent:
		w.WriteHeader(http.StatusNoContent)
	case code == http.StatusNotFound:
		// FCM HTTP v1 returns 404 for UNREGISTERED tokens.
		w.WriteHeader(http.StatusGone)
	default:
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
