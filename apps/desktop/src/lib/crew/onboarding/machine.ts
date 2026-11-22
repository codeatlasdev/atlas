import { m } from "#lib/i18n/m.ts";
import { OS, type Os } from "#lib/keys.ts";

/** What the person's own machine is called in the onboarding art, in the words of their OS ("your Mac", "this PC"). */
export const machineName = (os: Os = OS) => m.common_os_your_computer({ os });
