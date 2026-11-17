import { expect, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// scripts/macos-signing-secrets.sh turns Apple's .cer and the CSR's private key into the repository secrets that
// release.yml signs and notarizes with. A wrong secret only shows on the next release (a 40-minute round trip), so the
// script checks everything it can first, and no value may ever reach a command line or the output. Driven for real:
// openssl makes a certificate named like Apple's, a fake `gh` records its argv and stdin.
const script = join(import.meta.dir, "macos-signing-secrets.sh");
const IDENTITY = "Developer ID Application: CODEATLAS TEST (ABCDE12345)";
const KEY_ID = "KEYID12345";
const ISSUER = "11111111-2222-3333-4444-555555555555";
const P8 = "-----BEGIN PRIVATE KEY-----\nMIGTAgEAMBMG-not-a-real-key\n-----END PRIVATE KEY-----\n";

const sh = (cmd: string[], cwd: string) => {
  const r = Bun.spawnSync(cmd, { cwd });
  if (r.exitCode !== 0) throw new Error(`${cmd.join(" ")}: ${r.stderr.toString()}`);
};

// A certificate for a fresh key; `cn` is its commonName, `der` picks Apple's .cer encoding or PEM.
function setup(opts: { cn?: string; der?: boolean; otherKey?: boolean; days?: number } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "devid-"));
  sh(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "devid.key", "-out", "cert.pem", "-days", String(opts.days ?? 30), "-subj", `/CN=${opts.cn ?? IDENTITY}`], dir);
  if (opts.otherKey) sh(["openssl", "genrsa", "-out", "devid.key", "2048"], dir);
  const cer = join(dir, "devid.cer");
  sh(["openssl", "x509", "-in", "cert.pem", "-outform", opts.der === false ? "PEM" : "DER", "-out", cer], dir);
  writeFileSync(join(dir, "ASC.p8"), P8);
  writeFileSync(join(dir, "apple.env"), `# comment\nASC_KEY_ID=${KEY_ID}\nASC_ISSUER_ID=${ISSUER}\nASC_KEY_PATH=${join(dir, "ASC.p8")}\n`);
  // The fake gh: argv of every call to calls.log, stdin to out/<SECRET NAME>.
  sh(["mkdir", "out"], dir);
  writeFileSync(join(dir, "gh"), `#!/bin/sh\necho "$@" >> "${dir}/calls.log"\ncat > "${dir}/out/$3"\n`);
  chmodSync(join(dir, "gh"), 0o755);
  return { dir, cer };
}

function run(dir: string, args: string[], env: Record<string, string> = {}) {
  const r = Bun.spawnSync(["bash", script, ...args], {
    cwd: dir,
    env: { PATH: process.env.PATH ?? "", HOME: dir, DEVID_KEY: join(dir, "devid.key"), APPLE_ENV_FILE: join(dir, "apple.env"), GH: join(dir, "gh"), ...env },
  });
  return { code: r.exitCode, out: r.stdout.toString() + r.stderr.toString() };
}
const secret = (dir: string, name: string) => readFileSync(join(dir, "out", name), "utf8");
const unixTest = test.skipIf(process.platform === "win32");

unixTest("dry run: every secret name, no value, no gh call", () => {
  const { dir, cer } = setup();
  const r = run(dir, [cer, "--dry-run"]);
  expect(r.code).toBe(0);
  for (const n of ["MACOS_CERTIFICATE_P12_BASE64", "MACOS_CERTIFICATE_PASSWORD", "MACOS_SIGNING_IDENTITY", "APPLE_API_KEY_ID", "APPLE_API_ISSUER", "APPLE_API_KEY_P8"]) {
    expect(r.out).toContain(n);
  }
  for (const v of [KEY_ID, ISSUER, "not-a-real-key", "ABCDE12345"]) expect(r.out).not.toContain(v);
  expect(existsSync(join(dir, "calls.log"))).toBe(false);
});

unixTest("a real run sets the six secrets with values on stdin only; the .p12 opens with the generated password", () => {
  const { dir, cer } = setup();
  const r = run(dir, [cer, "--repo", "acme/widgets"]);
  expect(r.code).toBe(0);
  expect(readdirSync(join(dir, "out")).sort()).toEqual(["APPLE_API_ISSUER", "APPLE_API_KEY_ID", "APPLE_API_KEY_P8", "MACOS_CERTIFICATE_P12_BASE64", "MACOS_CERTIFICATE_PASSWORD", "MACOS_SIGNING_IDENTITY"]);
  expect(secret(dir, "MACOS_SIGNING_IDENTITY")).toBe(IDENTITY);
  expect(secret(dir, "APPLE_API_KEY_ID")).toBe(KEY_ID);
  expect(secret(dir, "APPLE_API_ISSUER")).toBe(ISSUER);
  expect(secret(dir, "APPLE_API_KEY_P8")).toBe(P8);

  const password = secret(dir, "MACOS_CERTIFICATE_PASSWORD");
  const b64 = secret(dir, "MACOS_CERTIFICATE_P12_BASE64");
  expect(b64).not.toContain("\n");
  writeFileSync(join(dir, "got.p12"), Buffer.from(b64, "base64"));
  // The same openssl read release.yml does, with the password from the secret: certificate and key are inside.
  const p = Bun.spawnSync(["openssl", "pkcs12", "-in", join(dir, "got.p12"), "-passin", "env:PW", "-nodes"], { env: { PATH: process.env.PATH ?? "", PW: password } });
  const pem = p.stdout.toString();
  expect(p.exitCode).toBe(0);
  expect(pem).toContain("BEGIN CERTIFICATE");
  expect(pem).toContain("PRIVATE KEY");

  // Nothing secret on a command line or in the output.
  const calls = readFileSync(join(dir, "calls.log"), "utf8");
  expect(calls.split("\n").filter(Boolean)).toHaveLength(6);
  expect(calls).toContain("--repo acme/widgets");
  for (const v of [password, b64.slice(0, 40), KEY_ID, ISSUER, "not-a-real-key"]) {
    expect(calls).not.toContain(v);
    expect(r.out).not.toContain(v);
  }
});

unixTest("apple.env paths may be written ~/, $HOME/ or ${HOME}/ (the real file uses $HOME)", () => {
  const { dir, cer } = setup();
  for (const ref of ["~", "$HOME", "${HOME}", '"$HOME"']) {
    writeFileSync(join(dir, "apple.env"), `ASC_KEY_ID=${KEY_ID}\nASC_ISSUER_ID=${ISSUER}\nASC_KEY_PATH=${ref}/ASC.p8\n`);
    expect(run(dir, [cer, "--dry-run"]).code).toBe(0);
  }
});

unixTest("a PEM .cer works like Apple's DER one", () => {
  const { dir, cer } = setup({ der: false });
  expect(run(dir, [cer]).code).toBe(0);
  expect(secret(dir, "MACOS_SIGNING_IDENTITY")).toBe(IDENTITY);
});

unixTest("missing inputs fail naming the file, before any secret is set", () => {
  const { dir, cer } = setup();
  const missing = (r: { code: number; out: string }, what: string) => {
    expect(r.code).toBe(1);
    expect(r.out).toContain(what);
    expect(existsSync(join(dir, "calls.log"))).toBe(false);
  };
  missing(run(dir, [join(dir, "nope.cer")]), "nope.cer");
  missing(run(dir, [cer], { DEVID_KEY: join(dir, "nokey.key") }), "nokey.key");
  missing(run(dir, [cer], { APPLE_ENV_FILE: join(dir, "no.env") }), "no.env");
  missing(run(dir, []), "usage");
  writeFileSync(join(dir, "apple.env"), `ASC_KEY_ID=${KEY_ID}\nASC_ISSUER_ID=${ISSUER}\nASC_KEY_PATH=${join(dir, "gone.p8")}\n`);
  missing(run(dir, [cer, "--dry-run"]), "gone.p8");
  writeFileSync(join(dir, "apple.env"), `ASC_KEY_ID=${KEY_ID}\n`);
  missing(run(dir, [cer, "--dry-run"]), "ASC_ISSUER_ID");
});

unixTest("a certificate that is not from this key, not Developer ID, or not a certificate is refused (no secret set)", () => {
  for (const [opts, why] of [
    [{ otherKey: true }, "another CSR"],
    [{ cn: "Apple Development: Someone (ABCDE12345)" }, "not a Developer ID Application"],
  ] as const) {
    const { dir, cer } = setup(opts);
    const r = run(dir, [cer]);
    expect(r.code).toBe(1);
    expect(r.out).toContain(why);
    expect(existsSync(join(dir, "calls.log"))).toBe(false);
  }
  const { dir } = setup();
  writeFileSync(join(dir, "junk.cer"), "not a certificate");
  const r = run(dir, [join(dir, "junk.cer")]);
  expect(r.code).toBe(1);
  expect(r.out).toContain("not an X.509 certificate");
});
