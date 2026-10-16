# Agent logos

Official marks of the coding agents Atlas launches (ids in `crates/atlas-desktop/src/agent/registry.rs`), shown in the
New-tab menu, the tab strip and the tab switcher by `components/AgentLogo.svelte`. The table the app renders is
`../agentLogos.ts`; `../agentLogos.test.ts` keeps it byte-identical to these files and fails when a registry agent has
no logo. Unknown agents fall back to the generic robot icon.

The marks are **trademarks of their owners**, shown only to identify the agent a tab runs, never to imply endorsement.
They are inlined unaltered: no recolor, no `currentColor`, no change of proportion. The only variant switching is
between the two files the vendor itself ships.

| File                                 | Agent id | Mark                                            | Source (primary)                                                                                                                                                                                                                                                                                          | License / terms                                                                                                                                                                                                                                                                                                                                                                  | Retrieved  |
| ------------------------------------ | -------- | ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| `claude-code.svg`                    | `claude` | Claude Code pixel mascot, brand color `#D97757` | [simple-icons `claudecode.svg`](https://github.com/simple-icons/simple-icons/blob/develop/icons/claudecode.svg) (source field: `https://code.claude.com`, hex `D97757` from its data)                                                                                                                     | Path data in a CC0 collection; the mark itself is an Anthropic trademark, [Anthropic Trademark Guidelines](https://www.anthropic.com/legal/trademark-guidelines): no alteration of color, font or proportion, no implied sponsorship, no ™ next to the mark. Only the `fill="#D97757"` attribute was added (simple-icons ships it unfilled).                                     | 2026-10-15 |
| `codex-black.svg`, `codex-white.svg` | `codex`  | OpenAI blossom, the Codex icon                  | `extension/resources/blossom-black.svg` / `blossom-white.svg` in the official `openai.chatgpt` ("Codex – OpenAI's coding agent") VS Code extension 26.5908.31748, read from [Open VSX](https://open-vsx.org/extension/openai/chatgpt); it is the icon of the extension's `package.json` `viewsContainers` | The package's `LICENSE.md` is "See https://openai.com/policies/row-terms-of-use"; the mark is an OpenAI trademark, [OpenAI design guidelines](https://openai.com/brand/) (the page returned HTTP 403 to automated fetches, so its text was not verified here). Files are only whitespace-collapsed. Black for light themes, white for dark themes, exactly as OpenAI ships them. | 2026-10-15 |
| `omp.svg`                            | `omp`    | Gradient π on a dark tile                       | [`packages/collab-web/public/favicon.svg`](https://github.com/can1357/oh-my-pi/blob/main/packages/collab-web/public/favicon.svg) in `can1357/oh-my-pi`, byte-identical to the favicon served at <https://omp.sh/favicon.svg>                                                                              | MIT, [`LICENSE`](https://github.com/can1357/oh-my-pi/blob/main/LICENSE). Only whitespace collapsed and the gradient id `g` renamed `omp-g` so it cannot collide with another inline SVG.                                                                                                                                                                                         | 2026-10-15 |

`omp chat` (the native rpc-ui tab) is the same product as `omp`, so it uses the same mark.

Not used: `can1357/oh-my-pi/assets/icon.svg` (older orange/white π, superseded by the favicon above) and the simple-icons
`claude` spark (that is the Claude chat product, not Claude Code).

## Adding an agent

1. Add it to `registry.rs`.
2. Drop its official SVG here, unaltered, and add a row above (source, license, brand rules).
3. Add it to `LOGO_FILES` and `LOGOS` in `../agentLogos.ts`.

`bun test src/lib/agentLogos.test.ts` fails until all three are done.
