# Writing for Atlas

Atlas is used by developers who work with AI coding agents all day. They read our text between two other tasks, often in a terminal, often while an agent is waiting. Their agents read the same text. Write for both.

This file holds the rules shared by every language. Then read the file for your language:

- English source text: [`en.md`](en.md)
- Brazilian Portuguese: [`pt-BR.md`](pt-BR.md)
- Terms that keep or change their form: `i18n/glossary.yaml`
- Words and patterns the lint rejects: [`flagged-terms.yaml`](flagged-terms.yaml)

## Voice

Atlas sounds like a senior engineer on your team: calm, exact, on your side, short with words. It states what happened, then what to do. It does not sell, cheer, apologize at length or explain things you already know.

Tone shifts with the moment; the voice stays the same ([Mailchimp](https://styleguide.mailchimp.com/voice-and-tone/), [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/writing)).

| Moment | Tone | Example |
|---|---|---|
| Something broke | Direct, no drama | `Couldn't reach crew server box. SSH timed out after 10 s.` |
| Success | Brief, past tense | `Worktree created` |
| Waiting | Says what it waits for | `Waiting for hostd to restart (up to 30 s)` |
| Destructive action | Names the object and the cost | `Delete 3 worktrees? Uncommitted changes in them are lost.` |
| First run | Shows the next step | `No projects yet. Add any project folder: Atlas detects how to run it.` |

## Ten rules

1. **Say the specific thing.** Name the file, the service, the branch, the number. `Couldn't open src/main.rs: file not found` beats `An error occurred`. Generic text is the main sign of low-effort and machine-made writing: models drift toward the most probable, most general phrasing ([Wikipedia: Signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing)).
2. **Lead with what matters.** Result first, cause second, detail last. Put the action in the first words of a button and the outcome in the first words of a message.
3. **Tell people what to do next.** Every error, empty state and blocked state ends with an action or a command ([NN/g](https://www.nngroup.com/articles/error-message-guidelines/), [Vercel](https://vercel.com/design/guidelines): "error messages guide the exit").
4. **Use the same word for the same thing.** One noun per concept, everywhere: UI, CLI, docs, both languages. The glossary decides ([Shopify](https://shopify.dev/docs/apps/design/content), [Primer](https://primer.style/product/getting-started/foundations/content)).
5. **Active voice, present tense, second person.** `Atlas restarts the service`, not `The service will be restarted` ([Google](https://developers.google.com/style/voice)).
6. **Sentence case everywhere.** Titles, buttons, menu items, tabs, toasts. Capitalize only the first word and proper names (Atlas, Atlas Crew, GitHub, Claude Code).
7. **No em dash, no en dash.** In any language, in any file a person reads. Use a colon, a comma, parentheses or a new sentence. See "Punctuation" below.
8. **No hype and no filler.** No adjectives that praise our own product, no "AI vocabulary", no padding triplets, no `not just X, but Y`. See "What machine-made text looks like".
9. **No blame, no jokes in failure.** Don't call input `invalid` or `illegal`, don't write `Oops`. Humor is fine on a calm screen and never in errors, waits or data loss ([Primer](https://primer.style/product/getting-started/foundations/content), [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/writing)).
10. **Read it out loud.** If you would not say it to a colleague at the next desk, rewrite it.

## Punctuation

The em dash rule is a house rule, not grammar. Two reasons:

- Readers now treat it as a marker of machine text. Wikipedia lists em dash overuse among its signs of AI writing, noting that models put them "in places where humans are more likely to use commas, parentheses, colons". A 2026 study by The Economist (55,940 sentences, reported in [Folha, 2026-08-01](https://www1.folha.uol.com.br/mercado/2026/08/como-identificar-textos-gerados-por-ia.shtml)) found that among current models only Claude still uses more dashes than human writers. Most Atlas text is drafted by agents, so the dash would show up here first.
- It hides structure. A dash can mean "because", "for example", "and also" or "which is". The replacement forces you to pick the real relation.

Replace by meaning, not by swapping one mark for another:

| Dash used for | Write instead | Example |
|---|---|---|
| Explanation or list after a clause | Colon | `Atlas found 3 services: api, web, worker.` |
| Aside | Parentheses or commas | `The tunnel (if enabled) closes on quit.` |
| Contrast or second thought | New sentence | `The build passed. Two tests were skipped.` |
| Cause | `because`, `so` | `Restart hostd so the new key takes effect.` |
| Range | `to`, or a hyphen in tables | `2 to 5 minutes`, `2-5 min` |

Don't fix the dash habit by writing only short sentences with periods and commas. Wikipedia notes that models trained to avoid dashes now produce flat, uniform punctuation, and that this has become a sign of its own. Vary sentence length. Use parentheses and semicolons when they are the right mark.

Other rules for both languages:

- No emoji in UI text or docs. Screen readers announce their names, they render differently per OS, and emoji bullets are a known machine-text tell. Text symbols that Atlas already uses for status in the CLI (`✓`, `✗`, `▸`, `◆`, `·`) are fine.
- No exclamation marks in errors, warnings or toasts ([Google](https://developers.google.com/style/tone)).
- Use `…` only for a truncated value or an action that opens a dialog for more input (`Rename…`).
- Code, commands, paths, flags, env vars and config keys go in code formatting in docs and in backticks in CLI output: `` Run `atlas crew install box` ``.

## What machine-made text looks like

Research since 2023 shows that LLM text has measurable habits. Avoid them because they make text vague and generic. The fact that readers also recognize them is a second reason.

**Vocabulary.** Words that rose sharply after chatbots arrived: `delve`, `intricate`, `underscore`, `pivotal`, `showcase`, `tapestry`, `testament`, `meticulous` and others ([Kobak et al., Science Advances 2025](https://arxiv.org/abs/2406.07016): at least 13.5% of 2024 biomedical abstracts were processed with LLMs, detected by excess style words; [Juzek and Ward, ACL Findings 2025](https://arxiv.org/abs/2412.11385): 21 focal words, linked to human feedback training; [Liang et al., 2024](https://arxiv.org/abs/2403.07183): 6.5% to 16.9% of AI conference peer reviews substantially modified by LLMs). The list moves with each model generation: `delve` dropped off in 2025, and Wikipedia lists `leverage`, `foster`, `multifaceted`, `quietly`, `steady`, `dependable` and `prioritize` for models from mid-2026. Marketing verbs (`unlock`, `elevate`, `empower`, `seamless`) belong to the same family. GOV.UK has banned many of them for years because they "do not say what you actually mean" ([GOV.UK, words to avoid](https://guidance.publishing.service.gov.uk/writing-to-gov-uk-standards/style-guides/a-to-z-style-guide/#words-to-avoid)).

**Grammar.** Instruction-tuned models use present participle clauses at 2 to 5 times the human rate (`..., ensuring that ...`, `..., highlighting ...`) and nominalizations at 1.5 to 2 times the human rate ([Reinhart et al., PNAS 2025](https://pmc.ncbi.nlm.nih.gov/articles/PMC11874169)). They avoid plain `is` and `has` in favor of `serves as`, `features`, `offers` (Wikipedia). Write verbs, not nouns made from verbs: `Atlas installs`, not `Atlas performs the installation`.

**Structure.** `Not X, but Y`, `not only ... but also`, groups of three, a closing line about why it all matters, and hedges about what "may vary". The Economist study above found ChatGPT and Claude use these constructions more per thousand sentences than other models and than humans. Also: headings in Title Case, bold sprinkled through prose, emoji at the start of list items, and every list having exactly three items (Wikipedia).

**Chat residue.** `Great question`, `I hope this helps`, `Certainly!`, `Let's dive in`, `Here's the thing`. These leak into UI copy and commit messages when an agent writes them.

How to avoid all of it: write the specific fact first, then delete every word that does not change what the reader does next. Detection tools and human guessing are both unreliable (Wikipedia cites studies where non-expert humans score close to chance), so the goal is never "sound human". The goal is: be specific, be useful, be short.

## Text that agents read too

Atlas output is read by Claude Code, Codex and omp as often as by people: CLI errors, hostd responses, MCP tool results, `--json`, logs and docs. Anthropic's guidance for agent tools asks for errors that "communicate specific and actionable improvements, rather than opaque error codes or tracebacks", and for names over opaque IDs ([Anthropic, Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)).

- **One problem, one sentence, one next step.** The next step is a command the reader can run as is: `` Run `atlas crew browsers install box` `` beats `install the browsers first`.
- **Stable codes for anything a program or agent may match.** A dotted id that never changes and is never translated: `crew.unreachable`, `hostd.unauthorized`. The message around it is translated; the code is not. On the wire, send the code plus parameters and translate at the display boundary (see `local://i18n-repo-map.md` §2.4). This is the pattern of Stripe's error object: a fixed `code`, a human `message`, a `doc_url` ([Stripe API errors](https://docs.stripe.com/api/errors)).
- **Machine output never changes language.** `--json`, `step:` lines, exit codes, flag names, config keys and MCP tool descriptions stay English and stable. Only text for people is translated.
- **Real names, not handles.** `crewmate reviewer on box`, not `entity 7f3a…`. Include the path, the port, the service name.
- **No color-only meaning.** `error:` and `warning:` are words, not just red and yellow, and ANSI codes are off when output is not a terminal ([clig.dev](https://clig.dev/)).
- **Docs readable by a model without a browser.** One topic per page, headings that say what is under them, full names instead of "it" across paragraphs, commands in fenced blocks, no information only in images. Serve Markdown and keep `llms.txt` current ([llms.txt](https://llmstxt.org/)).

## What only a human reviewer can judge

The lint catches words and marks. It cannot judge these; a reviewer who reads the string in context (screenshot or running app) must check each:

1. **Truth.** The text matches what the code does: the button does what its label says, the error names the real cause, the time estimate is real.
2. **The next step works.** The suggested command exists, has the right arguments and fixes the problem.
3. **Specific enough.** Could this sentence appear unchanged in another product? Then it is too generic.
4. **Tone fits the moment.** No cheer in a failure, no alarm in a routine notice, no humor where data may be lost.
5. **Padding the regex misses.** Groups of three, a closing "why it matters" line, a sentence that repeats the heading, a paraphrased `not X but Y`, hedges like "may vary depending on your setup".
6. **Rhythm.** Neither long monotone sentences joined by "and" nor a row of short sentences of the same length.
7. **Dash replacements.** The mark chosen matches the meaning (colon for a list, parentheses for an aside), not a mechanical swap.
8. **Glossary sense.** The right term for this meaning (a glossary lint can check spelling, not sense).
9. **pt-BR reads as written in Portuguese**, not translated: word order, contractions with `de`/`em`, possessives, agreement with English loanwords, no stiff subjunctive. See `pt-BR.md`.
10. **Fits the layout in both languages.** Portuguese text runs longer than English. Check the narrowest window and the phone.
11. **Accessible names.** Every button and link makes sense read alone by a screen reader ([Primer: descriptive buttons](https://primer.style/accessibility/patterns/primer-components/descriptive-buttons)).
12. **Privacy.** Notifications and anything sent to the push relay carry only the event kind and the `<agent> · <folder>` title, never a prompt, reply, transcript or session title (`crates/atlas-host/src/agents/push.rs`, `docs/mobile.md`).

## Sources

Style guides: [Apple HIG, Writing](https://developer.apple.com/design/human-interface-guidelines/writing) · [Google developer style guide, tone](https://developers.google.com/style/tone) and [voice](https://developers.google.com/style/voice) · [Google, error messages course](https://developers.google.com/tech-writing/error-messages) · [Microsoft Writing Style Guide, dashes](https://learn.microsoft.com/en-us/style-guide/punctuation/dashes-hyphens/) · [Microsoft pt-BR localization style guide (PDF)](https://download.microsoft.com/download/8/e/3/8e349e32-9eb9-4b63-9909-7586b94a24dd/por-bra-StyleGuide.pdf) · [GitHub Primer, content](https://primer.style/product/getting-started/foundations/content) · [Shopify app design, content](https://shopify.dev/docs/apps/design/content) and [Polaris, actionable language (archived)](https://web.archive.org/web/20241202110733/https://polaris.shopify.com/content/actionable-language) · [Atlassian, error messages](https://atlassian.design/content/writing-guidelines/writing-error-messages) and [empty states](https://atlassian.design/content/writing-guidelines/empty-state) · [Vercel Web Interface Guidelines](https://vercel.com/design/guidelines) · [Mailchimp, voice and tone](https://styleguide.mailchimp.com/voice-and-tone/) · [GOV.UK A to Z](https://guidance.publishing.service.gov.uk/writing-to-gov-uk-standards/style-guides/a-to-z-style-guide/) · [gov.br Design System, microcopy](https://www.gov.br/ds/padroes/writing/microcopy?tab=visao-geral) · [Lei 15.263/2025, Linguagem Simples](https://www2.camara.leg.br/legin/fed/lei/2025/lei-15263-14-novembro-2025-798293-publicacaooriginal-177011-pl.html).

Research on UX: [NN/g, error messages](https://www.nngroup.com/articles/error-message-guidelines/) · [NN/g, empty states](https://www.nngroup.com/articles/empty-state-interface-design/) · [NN/g, confirmation dialogs](https://www.nngroup.com/articles/confirmation-dialog/).

Developer tools: [clig.dev](https://clig.dev/) · [rustc diagnostics guide](https://rustc-dev-guide.rust-lang.org/diagnostics.html) · [Stripe API errors](https://docs.stripe.com/api/errors) · [Anthropic, writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents) · [llms.txt](https://llmstxt.org/).

Machine-text research: [Wikipedia: Signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing) · [Kobak et al. 2025](https://arxiv.org/abs/2406.07016) · [Juzek and Ward 2025](https://arxiv.org/abs/2412.11385) · [Reinhart et al., PNAS 2025](https://pmc.ncbi.nlm.nih.gov/articles/PMC11874169) · [Liang et al. 2024](https://arxiv.org/abs/2403.07183) · [Paech et al., Antislop, ICLR 2026](https://arxiv.org/abs/2510.15061) · [The Economist via Folha, 2026](https://www1.folha.uol.com.br/mercado/2026/08/como-identificar-textos-gerados-por-ia.shtml) · [Sam Kriss, NYT Magazine, 2025](https://www.nytimes.com/2025/12/03/magazine/chatbot-writing-style.html).
