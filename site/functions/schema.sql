-- D1 schema for the waitlist (database: atlas-waitlist, binding: DB).
-- Apply:  wrangler d1 execute atlas-waitlist --remote --file site/functions/schema.sql
CREATE TABLE IF NOT EXISTS waitlist (
  email      TEXT PRIMARY KEY,
  created_at INTEGER NOT NULL,
  lang       TEXT
);

-- Fixed-window rate limit keyed by a salted hash of the client IP (no raw IPs stored).
CREATE TABLE IF NOT EXISTS rate_limit (
  bucket       TEXT PRIMARY KEY,
  count        INTEGER NOT NULL,
  window_start INTEGER NOT NULL
);
