// POST /api/waitlist  { email, lang?, website? }  ->  { ok: true } | { ok: false, error }
// Cloudflare Pages Function. Storage: D1 binding `DB` (tables in site/functions/schema.sql).
// No PII is logged. The client IP is only used as a salted SHA-256 rate-limit key.

interface Env {
  DB: D1Database;
  RATE_SALT?: string;
}

const MAX_EMAIL = 254;
const EMAIL_RE = /^[^\s@<>()[\]\\,;:"]+@[^\s@<>()[\]\\,;:"]+\.[^\s@<>()[\]\\,;:"]{2,}$/;
const LIMIT = 5; // requests per window per client
const WINDOW_S = 600; // 10 minutes
const ALLOWED_ORIGINS: Record<string, true> = {
  "https://atlas.codeatlas.com.br": true,
};

const json = (body: unknown, status = 200, extra: HeadersInit = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      ...extra,
    },
  });

async function bucketKey(request: Request, salt: string): Promise<string> {
  const ip = request.headers.get("cf-connecting-ip") ?? "unknown";
  const data = new TextEncoder().encode(`${salt}:${ip}`);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return [...new Uint8Array(digest)].slice(0, 16).map((b) => b.toString(16).padStart(2, "0")).join("");
}

function originAllowed(request: Request): boolean {
  const origin = request.headers.get("origin");
  if (!origin) return false;
  if (Object.hasOwn(ALLOWED_ORIGINS, origin)) return true;
  // Pages previews and local dev
  return /^https:\/\/[a-z0-9-]+\.atlas-site-746\.pages\.dev$/.test(origin) ||
    origin === "https://atlas-site-746.pages.dev" ||
    /^http:\/\/(localhost|127\.0\.0\.1):\d+$/.test(origin);
}

export const onRequestPost: PagesFunction<Env> = async ({ request, env }) => {
  if (!originAllowed(request)) return json({ ok: false, error: "forbidden" }, 403);

  const ctype = request.headers.get("content-type") ?? "";
  if (!ctype.includes("application/json") && !ctype.includes("application/x-www-form-urlencoded")) {
    return json({ ok: false, error: "unsupported_media_type" }, 415);
  }
  const len = Number(request.headers.get("content-length") ?? "0");
  if (len > 2048) return json({ ok: false, error: "too_large" }, 413);

  let email = "";
  let lang = "";
  let honeypot = "";
  try {
    if (ctype.includes("application/json")) {
      const body = (await request.json()) as Record<string, unknown>;
      email = String(body.email ?? "");
      lang = String(body.lang ?? "");
      honeypot = String(body.website ?? "");
    } else {
      const form = await request.formData();
      email = String(form.get("email") ?? "");
      lang = String(form.get("lang") ?? "");
      honeypot = String(form.get("website") ?? "");
    }
  } catch {
    return json({ ok: false, error: "bad_request" }, 400);
  }

  // Bots fill the hidden field: pretend success, store nothing.
  if (honeypot) return json({ ok: true });

  email = email.trim().toLowerCase();
  if (!email || email.length > MAX_EMAIL || !EMAIL_RE.test(email)) {
    return json({ ok: false, error: "invalid_email" }, 422);
  }
  lang = /^[a-z]{2}(-[A-Za-z]{2})?$/.test(lang) ? lang : "";

  // Fixed-window rate limit per hashed client key, atomic in a single statement.
  const now = Math.floor(Date.now() / 1000);
  const key = await bucketKey(request, env.RATE_SALT ?? "atlas-waitlist");
  const rl = await env.DB.prepare(
    `INSERT INTO rate_limit (bucket, count, window_start) VALUES (?1, 1, ?2)
     ON CONFLICT(bucket) DO UPDATE SET
       count = CASE WHEN ?2 - window_start >= ?3 THEN 1 ELSE count + 1 END,
       window_start = CASE WHEN ?2 - window_start >= ?3 THEN ?2 ELSE window_start END
     RETURNING count`,
  )
    .bind(key, now, WINDOW_S)
    .first<{ count: number }>();
  if (rl && rl.count > LIMIT) {
    return json({ ok: false, error: "rate_limited" }, 429, { "retry-after": String(WINDOW_S) });
  }

  // Idempotent: signing up twice is a success, never reveals prior membership.
  await env.DB.prepare(
    "INSERT INTO waitlist (email, created_at, lang) VALUES (?1, ?2, ?3) ON CONFLICT(email) DO NOTHING",
  )
    .bind(email, now, lang || null)
    .run();

  return json({ ok: true });
};

// Anything but POST: explicit 405 (the form is same-origin, no CORS needed).
export const onRequest: PagesFunction<Env> = () =>
  json({ ok: false, error: "method_not_allowed" }, 405, { allow: "POST" });
