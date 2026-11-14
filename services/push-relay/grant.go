package main

// Caller authentication for /v1/notify (issue #205).
//
// The relay has no accounts and keeps no per-user state, so a "grant" is a pure function of a
// server secret K and the FCM token: grant = base64url(HMAC-SHA256(K, token)). A phone fetches its
// grant from POST /v1/register and hands {token, grant} to each of its crew servers; a crew server
// presents the grant on every notify and the relay recomputes it. Whoever only learned an FCM token
// (a log line, a leaked tokens.json minus the grant) cannot notify it.
//
// K comes from RELAY_GRANT_KEY. RELAY_GRANT_KEY_PREVIOUS is also accepted for verification, never
// for minting, so K can be rotated without invalidating every phone at once.

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

const (
	minKeyLen = 16
	// maxTokenLen bounds /v1/register input. FCM registration tokens are ~160-200 bytes.
	maxTokenLen = 4096

	// Per-IP fixed-window limits, in addition to the per-token notify limit. They sit before the
	// grant check so guessing grants from one address is bounded too.
	ipLimitPerMin       = 300
	registerLimitPerMin = 20
)

// loadKeys returns [current, previous?]. It refuses a missing or short current key: a relay that
// starts without one would either accept everyone or reject everyone.
func loadKeys(current, previous string) ([]string, error) {
	if len(current) < minKeyLen {
		return nil, errors.New("RELAY_GRANT_KEY is required (at least 16 bytes, e.g. `openssl rand -base64 32`)")
	}
	keys := []string{current}
	if previous != "" {
		if len(previous) < minKeyLen {
			return nil, errors.New("RELAY_GRANT_KEY_PREVIOUS must be at least 16 bytes when set")
		}
		keys = append(keys, previous)
	}
	return keys, nil
}

func mint(key, token string) string {
	m := hmac.New(sha256.New, []byte(key))
	m.Write([]byte(token))
	return base64.RawURLEncoding.EncodeToString(m.Sum(nil))
}

// grant is what /v1/register hands out: always minted with the current key.
func (r *relay) grant(token string) string { return mint(r.keys[0], token) }

// verify accepts a grant minted by any configured key. Every key is checked (no early exit on the
// first match) and compared in constant time.
func (r *relay) verify(token, grant string) bool {
	ok := false
	for _, k := range r.keys {
		if hmac.Equal([]byte(mint(k, token)), []byte(grant)) {
			ok = true
		}
	}
	return ok
}

// limiter is a fixed-window counter per key (a token or an IP).
type limiter struct {
	max     int
	mu      sync.Mutex
	buckets map[string]*bucket
}

func newLimiter(max int) *limiter { return &limiter{max: max, buckets: make(map[string]*bucket)} }

// bucket is one key's window.
type bucket struct {
	count     int
	windowEnd time.Time
}

// allow returns false once key has used its allowance for the current one-minute window.
func (l *limiter) allow(key string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	now := time.Now()
	if len(l.buckets) > 10000 { // bound memory under a flood of distinct keys
		for k, b := range l.buckets {
			if now.After(b.windowEnd) {
				delete(l.buckets, k)
			}
		}
	}
	b, ok := l.buckets[key]
	if !ok || now.After(b.windowEnd) {
		l.buckets[key] = &bucket{count: 1, windowEnd: now.Add(time.Minute)}
		return true
	}
	if b.count >= l.max {
		return false
	}
	b.count++
	return true
}

// clientIP is the address limits are keyed on: the socket peer, or, only when the operator says a
// trusted proxy sets it (RELAY_TRUST_PROXY=1, e.g. Cloud Run / a load balancer), the left-most
// X-Forwarded-For entry. Trusting the header by default would let anyone dodge the limit.
func clientIP(req *http.Request, trustProxy bool) string {
	if trustProxy {
		if xff := req.Header.Get("X-Forwarded-For"); xff != "" {
			first, _, _ := strings.Cut(xff, ",")
			if ip := strings.TrimSpace(first); ip != "" {
				return ip
			}
		}
	}
	host, _, err := net.SplitHostPort(req.RemoteAddr)
	if err != nil {
		return req.RemoteAddr
	}
	return host
}
