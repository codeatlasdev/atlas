package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// fcmResp builds a real FCM HTTP v1 error body.
// errorCode is the value inside google.firebase.fcm.v1.FcmError; pass "" for a plain NOT_FOUND.
func fcmResp(status int, errorCode string) (int, string) {
	details := []map[string]string{}
	if errorCode != "" {
		details = append(details, map[string]string{
			"@type":     "type.googleapis.com/google.firebase.fcm.v1.FcmError",
			"errorCode": errorCode,
		})
	}
	body, _ := json.Marshal(map[string]any{
		"error": map[string]any{
			"code":    status,
			"message": "...",
			"status":  "NOT_FOUND",
			"details": details,
		},
	})
	return status, string(body)
}

// fakeFCM starts an httptest server that mimics FCM HTTP v1 messages:send.
//
//	tok-ok          → 200
//	tok-unregistered → 404 with UNREGISTERED errorCode
//	tok-notfound     → 404 with empty details (wrong project, etc.)
func fakeFCM(t *testing.T) (srv *httptest.Server, okToken, unregToken, notFoundToken string) {
	t.Helper()
	okToken = "tok-ok"
	unregToken = "tok-unregistered"
	notFoundToken = "tok-notfound"
	srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
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
			code, resp := fcmResp(http.StatusNotFound, "UNREGISTERED")
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(code)
			w.Write([]byte(resp)) //nolint:errcheck
		case notFoundToken:
			// Real FCM 404 without UNREGISTERED — e.g. wrong FCM_PROJECT_ID.
			code, resp := fcmResp(http.StatusNotFound, "")
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(code)
			w.Write([]byte(resp)) //nolint:errcheck
		default:
			http.Error(w, "unexpected token", http.StatusBadRequest)
		}
	}))
	return
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

func newTestRelay(t *testing.T, fcmSrv *httptest.Server) *httptest.Server {
	t.Helper()
	return httptest.NewServer(newRelay("test-project", fcmSrv.URL, nil))
}

func TestNotify_204(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()

	resp := post(t, relay, okToken, "task_done")
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("want 204, got %d", resp.StatusCode)
	}
}

func TestNotify_410_Unregistered(t *testing.T) {
	fcm, _, unregToken, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()

	resp := post(t, relay, unregToken, "needs_you")
	if resp.StatusCode != http.StatusGone {
		t.Fatalf("want 410, got %d", resp.StatusCode)
	}
}

func TestNotify_502_NotFoundWithoutUnregistered(t *testing.T) {
	fcm, _, _, notFoundToken := fakeFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()

	// FCM 404 without UNREGISTERED (e.g. wrong project) must NOT drop the token.
	resp := post(t, relay, notFoundToken, "task_done")
	if resp.StatusCode != http.StatusBadGateway {
		t.Fatalf("want 502, got %d", resp.StatusCode)
	}
}

func TestNotify_400_BadBody(t *testing.T) {
	fcm, _, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()

	// Missing fields.
	resp, err := http.Post(relay.URL+"/v1/notify", "application/json", strings.NewReader("{}"))
	if err != nil {
		t.Fatal(err)
	}
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400 for missing fields, got %d", resp.StatusCode)
	}

	// Unknown kind.
	resp = post(t, relay, "tok-ok", "unknown_kind")
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400 for unknown kind, got %d", resp.StatusCode)
	}
}

func TestNotify_429_RateLimit(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()

	// First 60 must not be rate-limited.
	for range rateLimitPerMin {
		resp := post(t, relay, okToken, "task_done")
		if resp.StatusCode == http.StatusTooManyRequests {
			t.Fatalf("got 429 before request %d", rateLimitPerMin+1)
		}
	}

	// The 61st must be 429.
	resp := post(t, relay, okToken, "task_done")
	if resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("want 429 on request %d, got %d", rateLimitPerMin+1, resp.StatusCode)
	}
}
