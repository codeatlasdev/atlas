/**
 * Telegram on one crew server (`crew_telegram_status` / `_set` / `_link`): the server-wide bot token and the chats
 * linked to it. The token is write-only: hostd answers whether one is set, never the token itself.
 */
import { hostCall } from "#lib/host.ts";

export interface TelegramStatus {
	configured: boolean;
	chats: number;
}

/** The `crew_telegram_status` reply, read defensively (an older crew server may lack a field). */
export function parseTelegramStatus(r: unknown): TelegramStatus {
	const o = (r ?? {}) as Record<string, unknown>;
	return {
		configured: o.configured === true,
		chats: typeof o.chats === "number" && o.chats > 0 ? o.chats : 0,
	};
}

export async function loadTelegramStatus(hostKey: string): Promise<TelegramStatus> {
	return parseTelegramStatus(await hostCall("crew_telegram_status", { host: hostKey }));
}

/** Set the bot token from @BotFather; `null` removes it (linked chats stop receiving). */
export async function setTelegramToken(hostKey: string, token: string | null): Promise<void> {
	await hostCall("crew_telegram_set", { host: hostKey, token });
}

/** A 6-digit code, valid 10 minutes: the user sends `/start <code>` to the bot to link that chat. */
export async function startTelegramLink(hostKey: string): Promise<string> {
	const r = await hostCall<{ code: string }>("crew_telegram_link", { host: hostKey });
	return r.code;
}
