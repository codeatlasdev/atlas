// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { machineName } from "./machine.ts";

// #517: the art said "your Mac" on every OS.
test("the machine in the art is named by the OS the app runs on", () => {
	expect(machineName("mac")).toBe("your Mac");
	expect(machineName("windows")).toBe("this PC");
	expect(machineName("linux")).toBe("this computer");
});
