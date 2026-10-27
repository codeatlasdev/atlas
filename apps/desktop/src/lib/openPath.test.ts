// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { relativeTo, slashed } from "./openPath.ts";

test("a file inside the project root is relative to it; the root itself is empty", () => {
	expect(relativeTo("/p/app", "/p/app/src/main.ts")).toBe("src/main.ts");
	expect(relativeTo("/p/app", "/p/app")).toBe("");
	expect(relativeTo("/p/app/", "/p/app/a.ts")).toBe("a.ts");
});

test("a sibling that merely shares the root's prefix is outside it", () => {
	expect(relativeTo("/p/app", "/p/app-two/a.ts")).toBeNull();
	expect(relativeTo("/p/app", "/p/other/a.ts")).toBeNull();
});

// Windows paths come as `C:\x` from a shell and `c:/x` from the registry; both must meet.
test("Windows drive paths match across slash style and drive-letter case", () => {
	expect(relativeTo("C:/dev/atlas", "c:\\dev\\atlas\\src\\a.rs")).toBe("src\\a.rs".replace(/\\/g, "/"));
	expect(slashed("C:\\dev\\atlas\\")).toBe("C:/dev/atlas");
	expect(relativeTo("C:/dev/atlas", "D:/dev/atlas/a.rs")).toBeNull();
});
