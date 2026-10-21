package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// fakeFCM is an httptest server that mimics FCM HTTP v1 messages:send.
// It returns 200 for known tokens, 404 for "unregistered", and 400 otherwise.
func fakeFCM(t *testing.T) (*httptest.Server, string, string) {
	t.Helper()
	okToken := "tok-ok"
	unregToken := "tok-unregistered"
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body struct {
			Message struct {
				Token string `json:"token"`
			} `json:"message"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			http.Error(w, "bad", http.StatusBadRequest)
			return
		}
		switch body.Message.Token {
		case okToken:
			w.WriteHeader(http.StatusOK)
		case unregToken:
			// FCM HTTP v1 returns 404 for UNREGISTERED tokens.
			http.Error(w, `{"error":{"status":"NOT_FOUND"}}`, http.StatusNotFound)
		default:
			http.Error(w, "bad token", http.StatusBadRequest)
		}
	}))
	return srv, okToken, unregToken
}

func post(t *testing.T, srv *httptest.Server, token, kind string) *http.Response {
	t.Helper()
	body, _ := json.Marshal(notifyReq{Token: token, Kind: kind})
	resp, err := http.Post(srv.URL+"/v1/notify", "application/json", bytes.NewReader(body))
	if err != nil {
		t.Fatalf("post: %v", err)
	}
	return resp
}

// newTestRelay returns a relay wired to a fake FCM server with no auth.
func newTestRelay(t *testing.T, fcmSrv *httptest.Server) (*httptest.Server, *relay) {
	t.Helper()
	r := newRelay("test-project", fcmSrv.URL, nil)
	return httptest.NewServer(r), r
}

func TestNotify_204(t *testing.T) {
	fcm, okToken, _ := fakeFCM(t)
	defer fcm.Close()
	relaySrv, _ := newTestRelay(t, fcm)
	defer relaySrv.Close()

	resp := post(t, relaySrv, okToken, "task_done")
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("want 204, got %d", resp.StatusCode)
	}
}

func TestNotify_410_Unregistered(t *testing.T) {
	fcm, _, unregToken := fakeFCM(t)
	defer fcm.Close()
	relaySrv, _ := newTestRelay(t, fcm)
	defer relaySrv.Close()

	resp := post(t, relaySrv, unregToken, "needs_you")
	if resp.StatusCode != http.StatusGone {
		t.Fatalf("want 410, got %d", resp.StatusCode)
	}
}

func TestNotify_400_BadBody(t *testing.T) {
	fcm, _, _ := fakeFCM(t)
	defer fcm.Close()
	relaySrv, _ := newTestRelay(t, fcm)
	defer relaySrv.Close()

	// Empty body.
	resp, err := http.Post(relaySrv.URL+"/v1/notify", "application/json",
		strings.NewReader("{}"))
	if err != nil {
		t.Fatal(err)
	}
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400 for missing fields, got %d", resp.StatusCode)
	}

	// Unknown kind.
	resp = post(t, relaySrv, "tok-ok", "unknown_kind")
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400 for unknown kind, got %d", resp.StatusCode)
	}
}

func TestNotify_429_RateLimit(t *testing.T) {
	fcm, okToken, _ := fakeFCM(t)
	defer fcm.Close()
	relaySrv, _ := newTestRelay(t, fcm)
	defer relaySrv.Close()

	// Send exactly 60 requests — all should succeed (or at least not 429).
	for range rateLimitPerMin {
		resp := post(t, relaySrv, okToken, "task_done")
		if resp.StatusCode == http.StatusTooManyRequests {
			t.Fatalf("got 429 before request %d", rateLimitPerMin+1)
		}
	}

	// The 61st request must be 429.
	resp := post(t, relaySrv, okToken, "task_done")
	if resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("want 429 on request 61, got %d", resp.StatusCode)
	}
}
