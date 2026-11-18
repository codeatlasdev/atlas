// Preloaded by bunfig.toml into every scripts test file. Bun's default of 5 s per test is a wall-clock guess about work that
// spawns git, bun, bash or cargo: those tests take 0.2-0.4 s on an idle machine and more than 5 s only when a loaded or
// starved CI runner (Windows above all) stalls the process spawns, which says nothing about the code (#550). A test here has
// a ceiling that only a hang can reach; what a test proves is its assertions, never how fast the machine is.
import { setDefaultTimeout } from "bun:test";

setDefaultTimeout(120_000);
