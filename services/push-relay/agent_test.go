package main

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"unicode/utf8"
)

// A plain agent's push (an omp / Claude Code / Codex pane on the Mac) carries the pane key and a
// short title so the phone can open that agent's chat. Crew pushes carry neither.

type fcmSeen struct {
	Message struct {
		Token        string            `json:"token"`
		Notification map[string]string `json:"notification"`
		Data         map[string]string `json:"data"`
	} `json:"message"`
}

// capturingFCM answers 200 to everything and records the last message.
func capturingFCM(t *testing.T) (*httptest.Server, *fcmSeen) {
	t.Helper()
	seen := &fcmSeen{}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(b, seen)
		w.WriteHeader(http.StatusOK)
	}))
	return srv, seen
}

func notifyAgent(t *testing.T, srv *httptest.Server, token string, body map[string]string) *http.Response {
	t.Helper()
	body["token"] = token
	body["grant"] = mint(testKey, token)
	return postJSON(t, srv, "/v1/notify", body)
}

func TestNotify_AgentKindsForwardPaneAndTitle(t *testing.T) {
	for kind, want := range map[string]string{
		"agent_done":      "omp · atlas: An agent finished",
		"agent_needs_you": "omp · atlas: An agent needs you",
	} {
		fcm, seen := capturingFCM(t)
		relay := newTestRelay(t, fcm)
		resp := notifyAgent(t, relay, "tok-a", map[string]string{"kind": kind, "pane": "term-7f3a", "title": "omp · atlas"})
		if resp.StatusCode != http.StatusNoContent {
			t.Fatalf("%s: want 204, got %d", kind, resp.StatusCode)
		}
		d := seen.Message.Data
		if d["kind"] != kind || d["pane"] != "term-7f3a" || d["title"] != "omp · atlas" || len(d) != 3 {
			t.Fatalf("%s: FCM data = %v", kind, d)
		}
		if got := seen.Message.Notification["body"]; got != want {
			t.Fatalf("%s: body = %q, want %q", kind, got, want)
		}
		relay.Close()
		fcm.Close()
	}
}

// What the Mac's hostd serializes (apps/mobile/test/fixtures/push_agent_payload.json, written by the
// Rust test `push_payload_fixture_is_the_real_serializer_output`) must pass through unchanged.
func TestNotify_RustFixtureReachesFCMData(t *testing.T) {
	raw, err := os.ReadFile("../../apps/mobile/test/fixtures/push_agent_payload.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture map[string]string
	if err := json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	fcm, seen := capturingFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()
	sent := map[string]string{}
	for k, v := range fixture {
		sent[k] = v
	}
	resp := notifyAgent(t, relay, "tok-a", sent) // adds the token and grant to `sent`, not to `fixture`
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("want 204, got %d", resp.StatusCode)
	}
	if len(seen.Message.Data) != len(fixture) {
		t.Fatalf("FCM data = %v, want exactly the fixture %v", seen.Message.Data, fixture)
	}
	for k, v := range fixture {
		if seen.Message.Data[k] != v {
			t.Fatalf("data[%s] = %q, want %q", k, seen.Message.Data[k], v)
		}
	}
}

// A crew push never carries a pane or a title, even if a caller sends them: the app would open a
// chat for a pane that does not exist on that computer.
func TestNotify_CrewKindsDropPaneAndTitle(t *testing.T) {
	fcm, seen := capturingFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()
	resp := notifyAgent(t, relay, "tok-a", map[string]string{"kind": "task_done", "pane": "p", "title": "t"})
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("want 204, got %d", resp.StatusCode)
	}
	if d := seen.Message.Data; len(d) != 1 || d["kind"] != "task_done" {
		t.Fatalf("crew push data = %v", d)
	}
	if got := seen.Message.Notification["body"]; got != "A crewmate finished a task" {
		t.Fatalf("crew body = %q", got)
	}
}

// An agent push without a pane cannot open anything: refused, not sent half-empty.
func TestNotify_AgentKindNeedsAPane(t *testing.T) {
	fcm, _ := capturingFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()
	resp := notifyAgent(t, relay, "tok-a", map[string]string{"kind": "agent_done", "title": "omp"})
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("want 400, got %d", resp.StatusCode)
	}
}

// The title is shown on a lock screen: bounded, no control characters, never a log line.
func TestNotify_TitleIsBoundedAndCleaned(t *testing.T) {
	fcm, seen := capturingFCM(t)
	defer fcm.Close()
	relay := newTestRelay(t, fcm)
	defer relay.Close()
	long := strings.Repeat("é", 200) + "\n\x07"
	resp := notifyAgent(t, relay, "tok-a", map[string]string{"kind": "agent_done", "pane": "p", "title": "a\nb" + long})
	if resp.StatusCode != http.StatusNoContent {
		t.Fatalf("want 204, got %d", resp.StatusCode)
	}
	title := seen.Message.Data["title"]
	if n := utf8.RuneCountInString(title); n > 60 || !utf8.ValidString(title) {
		t.Fatalf("title has %d runes / valid=%v", n, utf8.ValidString(title))
	}
	if strings.ContainsAny(title, "\n\x07") {
		t.Fatalf("control characters survived: %q", title)
	}
}
