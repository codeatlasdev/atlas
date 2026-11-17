// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { machineName } from "./machine.ts";

// #517: the art said "your Mac" on every OS.
test("the machine in the art is named by the OS the app runs on", () => {
	expect(machineName(true, false)).toBe("your Mac");
	expect(machineName(false, true)).toBe("this PC");
	expect(machineName(false, false)).toBe("this computer");
});
