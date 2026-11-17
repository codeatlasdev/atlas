import { IS_MAC, IS_WIN } from "#lib/keys.ts";

/** What the person's own machine is called in the onboarding art, in the words of their OS (the same helper style as `fileManager`). */
export const machineName = (mac = IS_MAC, win = IS_WIN) =>
	mac ? "your Mac" : win ? "this PC" : "this computer";
