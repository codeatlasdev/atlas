// What each release channel says about itself on the download page, as messages. The English is `crates/atlas-core/data/channels.json`
// (the CLI and Desktop print it too); channel-copy.test.ts fails when a message drifts from it. Labels (Stable, Beta, Alpha) are names
// and are read from the JSON. Static references, not `m[...]`, so the compiler keeps tree-shaking and `i18n:check` sees every key used.
import { m } from "./paraglide/messages.js";
import type { Channel } from "../../functions/_lib/channels";

export interface ChannelCopy {
	promise: () => string;
	risk: () => string;
	audience: () => string;
	switchTo: () => string;
}

export const CHANNEL_COPY: Record<Channel, ChannelCopy> = {
	stable: { promise: m.page_download_channel_stable_promise, risk: m.page_download_channel_stable_risk, audience: m.page_download_channel_stable_audience, switchTo: m.page_download_channel_stable_switch_to },
	beta: { promise: m.page_download_channel_beta_promise, risk: m.page_download_channel_beta_risk, audience: m.page_download_channel_beta_audience, switchTo: m.page_download_channel_beta_switch_to },
	alpha: { promise: m.page_download_channel_alpha_promise, risk: m.page_download_channel_alpha_risk, audience: m.page_download_channel_alpha_audience, switchTo: m.page_download_channel_alpha_switch_to },
};

/** The note shown while `stage` is "alpha" in channels.json. */
export const stageAlphaNote = m.page_download_channel_stage_alpha_note;
