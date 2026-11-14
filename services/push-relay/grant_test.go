package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

// Issue #205: /v1/notify used to forward for anyone who knew an FCM token. A caller now needs the
// grant the relay itself issued for that token at /v1/register.

const testKey = "test-key-current"

func newAuthedRelay(t *testing.T, fcm *httptest.Server, keys ...string) *httptest.Server {
	t.Helper()
	if len(keys) == 0 {
		keys = []string{testKey}
	}
	r := newRelay("test-project", fcm.URL, nil)
	r.keys = keys
	return httptest.NewServer(r)
}

func postJSON(t *testing.T, srv *httptest.Server, path string, v any) *http.Response {
	t.Helper()
	body, _ := json.Marshal(v)
	resp, err := http.Post(srv.URL+path, "application/json", bytes.NewReader(body))
	if err != nil {
		t.Fatalf("post %s: %v", path, err)
	}
	return resp
}

func notifyWith(t *testing.T, srv *httptest.Server, token, grant string) *http.Response {
	t.Helper()
	return postJSON(t, srv, "/v1/notify", map[string]string{"token": token, "kind": "task_done", "grant": grant})
}

func registerGrant(t *testing.T, srv *httptest.Server, token string) string {
	t.Helper()
	resp := postJSON(t, srv, "/v1/register", map[string]string{"token": token})
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("register: want 200, got %d", resp.StatusCode)
	}
	var out struct {
		Grant string `json:"grant"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil || out.Grant == "" {
		t.Fatalf("register: no grant in body (err=%v)", err)
	}
	return out.Grant
}

func TestNotify_403_WithoutGrant(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	resp := notifyWith(t, relay, okToken, "")
	if resp.StatusCode != http.StatusForbidden {
		t.Fatalf("want 403 without a grant, got %d", resp.StatusCode)
	}
}

func TestNotify_403_WrongGrant(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	// A grant issued for ANOTHER token must not work for this one.
	other := registerGrant(t, relay, "some-other-token")
	for _, g := range []string{"garbage", other, "!!not-base64!!"} {
		resp := notifyWith(t, relay, okToken, g)
		if resp.StatusCode != http.StatusForbidden {
			t.Fatalf("grant %q: want 403, got %d", g, resp.StatusCode)
		}
	}
}

// A rejected caller must never reach FCM (the whole point), and must not eat the token's quota.
func TestNotify_403_NeverForwarded(t *testing.T) {
	hits := 0
	fcm := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { hits++ }))
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	notifyWith(t, relay, "tok-ok", "wrong")
	if hits != 0 {
		t.Fatalf("FCM was called %d time(s) for an unauthenticated notify", hits)
	}
}

func TestRegister_GrantVerifiesAndNotifyForwards(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	grant := registerGrant(t, relay, okToken)
	resp := notifyWith(t, relay, okToken, grant)
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("want 204 with the issued grant, got %d", resp.StatusCode)
	}
}

// The grant is a pure function of (K, token): registering again (token refresh, a second crew
// server) yields the same value, so the relay keeps no per-user state.
func TestRegister_GrantIsDeterministic(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	if a, b := registerGrant(t, relay, okToken), registerGrant(t, relay, okToken); a != b {
		t.Fatalf("grant changed between registrations: %q vs %q", a, b)
	}
}

func TestRegister_400_BadBody(t *testing.T) {
	fcm, _, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	resp := postJSON(t, relay, "/v1/register", map[string]string{"token": ""})
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400 for an empty token, got %d", resp.StatusCode)
	}
	if resp := postJSON(t, relay, "/v1/register", map[string]string{"token": string(make([]byte, 5000))}); resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400 for an oversized token, got %d", resp.StatusCode)
	}
}

// Key rotation: a grant minted under the previous key is still accepted while it is configured;
// new grants are always minted with the current key.
func TestNotify_PreviousKeyAcceptedDuringRotation(t *testing.T) {
	fcm, okToken, _, _ := fakeFCM(t)
	defer fcm.Close()

	before := newAuthedRelay(t, fcm, "old-key")
	oldGrant := registerGrant(t, before, okToken)
	before.Close()

	rotating := newAuthedRelay(t, fcm, "new-key", "old-key")
	defer rotating.Close()
	if resp := notifyWith(t, rotating, okToken, oldGrant); resp.StatusCode != http.StatusNoContent {
		t.Fatalf("previous-key grant: want 204 during rotation, got %d", resp.StatusCode)
	}
	newGrant := registerGrant(t, rotating, okToken)
	if newGrant == oldGrant {
		t.Fatal("register under rotation must mint with the current key")
	}

	// Rotation finished: the previous key is dropped and its grants stop working.
	done := newAuthedRelay(t, fcm, "new-key")
	defer done.Close()
	if resp := notifyWith(t, done, okToken, oldGrant); resp.StatusCode != http.StatusForbidden {
		t.Fatalf("old grant after rotation: want 403, got %d", resp.StatusCode)
	}
	if resp := notifyWith(t, done, okToken, newGrant); resp.StatusCode != http.StatusNoContent {
		t.Fatalf("new grant after rotation: want 204, got %d", resp.StatusCode)
	}
}

func TestLoadKeys(t *testing.T) {
	if _, err := loadKeys("", ""); err == nil {
		t.Fatal("must refuse to start without RELAY_GRANT_KEY")
	}
	if _, err := loadKeys("short", ""); err == nil {
		t.Fatal("must refuse a key shorter than 16 bytes")
	}
	keys, err := loadKeys("0123456789abcdef-current", "0123456789abcdef-previous")
	if err != nil || len(keys) != 2 || keys[0] != "0123456789abcdef-current" {
		t.Fatalf("keys=%v err=%v", keys, err)
	}
	if keys, err := loadKeys("0123456789abcdef-current", ""); err != nil || len(keys) != 1 {
		t.Fatalf("previous is optional: keys=%v err=%v", keys, err)
	}
}

// Per-IP limits sit in front of the grant check, so guessing grants from one address is bounded.
func TestNotify_429_PerIPLimit(t *testing.T) {
	fcm, _, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	// Distinct tokens each time, so the per-token bucket never trips; only the IP can.
	for i := range ipLimitPerMin {
		resp := notifyWith(t, relay, "tok-"+string(rune('a'+i%26))+string(rune('a'+i/26)), "wrong")
		if resp.StatusCode == http.StatusTooManyRequests {
			t.Fatalf("got 429 before request %d", ipLimitPerMin+1)
		}
	}
	resp := notifyWith(t, relay, "tok-fresh", "wrong")
	if resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("want 429 on request %d from one IP, got %d", ipLimitPerMin+1, resp.StatusCode)
	}
}

func TestRegister_429_PerIPLimit(t *testing.T) {
	fcm, _, _, _ := fakeFCM(t)
	defer fcm.Close()
	relay := newAuthedRelay(t, fcm)
	defer relay.Close()

	for i := range registerLimitPerMin {
		resp := postJSON(t, relay, "/v1/register", map[string]string{"token": "t" + string(rune('a'+i%26)) + string(rune('a'+i/26))})
		if resp.StatusCode != http.StatusOK {
			t.Fatalf("register %d: want 200, got %d", i+1, resp.StatusCode)
		}
	}
	resp := postJSON(t, relay, "/v1/register", map[string]string{"token": "one-more"})
	if resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("want 429 after %d registrations from one IP, got %d", registerLimitPerMin, resp.StatusCode)
	}
}

// Behind a proxy the client address is the left-most X-Forwarded-For entry only when the operator
// opts in; by default a spoofed header must not dodge the per-IP limit.
func TestClientIP_IgnoresForwardedForByDefault(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/v1/notify", nil)
	req.RemoteAddr = "203.0.113.7:51234"
	req.Header.Set("X-Forwarded-For", "198.51.100.1")
	if got := clientIP(req, false); got != "203.0.113.7" {
		t.Fatalf("got %q, want the socket address", got)
	}
	if got := clientIP(req, true); got != "198.51.100.1" {
		t.Fatalf("trusted proxy: got %q, want the forwarded address", got)
	}
}
