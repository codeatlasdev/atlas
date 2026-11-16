// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { tabTitle, titleState } from "./title.ts";

test("omp title markers map to states; plain titles do not", () => {
	expect(titleState("π : fixing tests")).toBe("working");
	expect(titleState("repo | π ! approve bash")).toBe("blocked");
	expect(titleState("π > done")).toBe("idle");
	expect(titleState("⠋ thinking")).toBe("working");
	expect(titleState("~/P/c/ac")).toBeNull();
	expect(titleState("api:3000")).toBeNull();
});

// Regression: 1.5.0-alpha.26, a terminal tab read "@oma:~/Projects/orbit-shop": the shell's `user@host:dir` title with
// an empty user name.
test("a title with an empty user part never starts with '@'", () => {
	expect(tabTitle("@oma:~/Projects/orbit-shop")).toBe("oma:~/Projects/orbit-shop");
	expect(tabTitle("@host:~/p")).toBe("host:~/p");
	expect(tabTitle("@host")).toBe("host");
});

test("titles that are already right, or are not user@host, are left as the program wrote them", () => {
	expect(tabTitle("user@host:~/p")).toBe("user@host:~/p");
	expect(tabTitle("host:~/p")).toBe("host:~/p");
	expect(tabTitle("@angular/cli")).toBe("@angular/cli");
	expect(tabTitle("@")).toBe("@");
	expect(tabTitle("π : fixing tests")).toBe("π : fixing tests");
	expect(tabTitle("")).toBe("");
	expect(tabTitle("  @oma:~  ")).toBe("oma:~");
});

test("the cleaned title still carries the agent state", () => {
	expect(titleState(tabTitle("@oma π : task"))).toBe("working");
});
