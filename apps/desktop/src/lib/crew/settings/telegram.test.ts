// @ts-nocheck bun test runtime types are not part of the app tsconfig
// Contract: Atlas Desktop's Telegram settings read the real `crew_telegram_status` reply.
// The fixture is written by the Rust test `telegram_is_set_up_linked_and_cleared_without_the_token_ever_coming_back`
// (UPDATE_FIXTURES=1 cargo test -p atlas-host telegram_is_set_up_linked).
import { describe, expect, test } from "bun:test";
import fixture from "../../../__fixtures__/crew_telegram_status.json" with { type: "json" };
import { parseTelegramStatus } from "./telegram.ts";

describe("parseTelegramStatus", () => {
	test("reads a configured server from the real reply", () => {
		expect(parseTelegramStatus(fixture)).toEqual({ configured: true, chats: 0 });
	});

	test("the reply never carries the token", () => {
		expect(Object.keys(fixture).sort()).toEqual(["chats", "configured"]);
	});

	test("a reply missing fields reads as not set up, never as configured", () => {
		expect(parseTelegramStatus({})).toEqual({ configured: false, chats: 0 });
		expect(parseTelegramStatus(null)).toEqual({ configured: false, chats: 0 });
		expect(parseTelegramStatus({ configured: "yes", chats: -1 })).toEqual({ configured: false, chats: 0 });
	});
});
