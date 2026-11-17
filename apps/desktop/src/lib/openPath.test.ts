// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { baseName, dirOf, dropAction, relativeTo, routeOpen, slashed } from "./openPath.ts";

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

// Issue #499: a loose `.md` opened the whole project UI (and registered a project). Text goes to the light editor only
// when the project check found no project; everything with a project, and every kind with no light viewer, opens as before.
test("a loose text file opens in the light editor; a file with a project, or with no light viewer, does not", () => {
	expect(routeOpen({ standalone: true }, "/Users/m/Downloads/notes.md")).toBe("light-editor");
	expect(routeOpen({ standalone: true }, "/Users/m/Downloads/todo.txt")).toBe("light-editor");
	expect(routeOpen({ standalone: true }, "/Users/m/Downloads/Makefile")).toBe("light-editor");
	expect(routeOpen({ standalone: true }, "/Users/m/Downloads/logo.svg")).toBe("light-editor");
	expect(routeOpen({ standalone: false }, "/Users/m/repo/README.md")).toBe("project");
	for (const f of ["a.png", "a.pdf", "a.mp4", "a.mp3"]) {
		expect(routeOpen({ standalone: true }, `/Users/m/Downloads/${f}`)).toBe("project");
	}
});

test("a drop opens a file anywhere and adds a folder only on the sidebar", () => {
	expect(dropAction("file", false)).toBe("open-file");
	expect(dropAction("file", true)).toBe("open-file");
	expect(dropAction("dir", true)).toBe("add-folder");
	expect(dropAction("dir", false)).toBe("ignore");
});

test("dirOf and baseName split a path in either slash style", () => {
	expect(dirOf("/Users/m/Downloads/notes.md")).toBe("/Users/m/Downloads");
	expect(dirOf("C:\\docs\\notes.md")).toBe("C:/docs");
	expect(dirOf("/notes.md")).toBe("/");
	expect(baseName("/Users/m/Downloads/notes.md")).toBe("notes.md");
	expect(baseName("C:\\docs\\notes.md")).toBe("notes.md");
});
