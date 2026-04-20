import { describe, expect, test } from "bun:test";
import { decrypt, decryptWithKey, encrypt, encryptWithKey } from "./index";

const SECRET = "test-secret-key-for-atlas-crypto";

describe("encryptWithKey / decryptWithKey", () => {
	test("roundtrip: decrypt(encrypt(text)) === text", async () => {
		const plaintext = "hello atlas";
		const encrypted = await encryptWithKey(plaintext, SECRET);
		const decrypted = await decryptWithKey(encrypted, SECRET);
		expect(decrypted).toBe(plaintext);
	});

	test("encrypts to base64 string", async () => {
		const encrypted = await encryptWithKey("test", SECRET);
		expect(typeof encrypted).toBe("string");
		expect(encrypted.length).toBeGreaterThan(0);
		// Should be valid base64
		expect(() => atob(encrypted)).not.toThrow();
	});

	test("different plaintexts produce different ciphertexts", async () => {
		const a = await encryptWithKey("aaa", SECRET);
		const b = await encryptWithKey("bbb", SECRET);
		expect(a).not.toBe(b);
	});

	test("same plaintext produces different ciphertexts (random IV)", async () => {
		const a = await encryptWithKey("same", SECRET);
		const b = await encryptWithKey("same", SECRET);
		expect(a).not.toBe(b);
	});

	test("wrong key fails to decrypt", async () => {
		const encrypted = await encryptWithKey("secret data", SECRET);
		await expect(decryptWithKey(encrypted, "wrong-key")).rejects.toThrow();
	});

	test("handles empty string", async () => {
		const encrypted = await encryptWithKey("", SECRET);
		const decrypted = await decryptWithKey(encrypted, SECRET);
		expect(decrypted).toBe("");
	});

	test("handles unicode", async () => {
		const text = "🔐 segredos do atlas — chave mestra";
		const encrypted = await encryptWithKey(text, SECRET);
		const decrypted = await decryptWithKey(encrypted, SECRET);
		expect(decrypted).toBe(text);
	});

	test("handles long text", async () => {
		const text = "x".repeat(10_000);
		const encrypted = await encryptWithKey(text, SECRET);
		const decrypted = await decryptWithKey(encrypted, SECRET);
		expect(decrypted).toBe(text);
	});
});

describe("encrypt / decrypt (env convenience)", () => {
	test("throws without ENCRYPTION_KEY", () => {
		const original = process.env.ENCRYPTION_KEY;
		delete process.env.ENCRYPTION_KEY;
		try {
			expect(() => encrypt("test")).toThrow("ENCRYPTION_KEY");
		} finally {
			if (original) process.env.ENCRYPTION_KEY = original;
		}
	});

	test("roundtrip with ENCRYPTION_KEY set", async () => {
		process.env.ENCRYPTION_KEY = SECRET;
		try {
			const encrypted = await encrypt("env test");
			const decrypted = await decrypt(encrypted);
			expect(decrypted).toBe("env test");
		} finally {
			delete process.env.ENCRYPTION_KEY;
		}
	});
});
