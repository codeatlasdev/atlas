# Atlas earcons: provenance

Generated 2026-10-12 with the ElevenLabs Sound Effects (text-to-sound) API: `POST https://api.elevenlabs.io/v1/sound-generation`, model `eleven_text_to_sound_v2`,
`prompt_influence` 0.7, then trimmed, gain-matched and encoded by `bun studio sounds pick` (studio/src/sounds).
Nothing here was taken from another sound library or the internet.

**License.** The files are output of the ElevenLabs API produced under the maintainers' paid account. Per ElevenLabs'
terms (https://elevenlabs.io/terms-of-use) generated output may be used commercially on paid plans; this was
generated while the account was on the plan the maintainers hold, and the terms at that date apply. If the account
plan changes, re-check the terms before redistributing the files outside Atlas.

Post-processing of the feedback earcons: leading/trailing silence cut at -45 dBFS (3 ms kept before the attack), the tail
cut where the note has decayed 30 dB below its peak, gain set so the RMS of the finished file is -22 dBFS with
the peak under -3 dBFS, 5 ms fade in and out (30 ms, or a quarter of the cap, when a take is cut at
its length cap), mono, 48 kHz, AAC 48 kb/s in an .m4a container (WebKit decodes it natively; only the .m4a files ship).

The `boot` signature differs in one way: its tail is cut at 40 dB of decay (a bell rings on quietly and that is part of it).

| id | meaning | length | RMS dBFS | peak dBFS | centroid Hz | .m4a bytes | take |
|---|---|---|---|---|---|---|---|
| `boot` | Atlas opened (cold start, at most once an hour) | 485 ms | -22.4 | -7.7 | 460 | 3999 | take 0, bb2080fbae14b740 |
| `discovery-done` | Find my projects finished | 300 ms | -22.2 | -11.2 | 756 | 2829 | take 2, daa4467b81f868fb |
| `stack-up` | Every service of a stack is healthy | 300 ms | -22.2 | -9.8 | 559 | 2937 | take 3, 6a9f6edc1c2f3ced |
| `service-failed` | A service crashed | 280 ms | -22.4 | -10.3 | 726 | 2815 | take 1, 885603266f6b8283 |
| `agent-needs-you` | An agent is waiting for you | 300 ms | -22.2 | -13.0 | 758 | 2884 | take 1, cbe93d9fc23a32c2 |
| `agent-done` | An agent finished | 300 ms | -22.2 | -12.7 | 905 | 2876 | take 0, a68835cd1bcc6886 |
| `update` | An update is ready | 255 ms | -22.2 | -8.7 | 714 | 2573 | take 5, 4064d9d1d3966e9a |
| `error` | An action failed (soft) | 300 ms | -22.5 | -11.6 | 609 | 2832 | take 1, 28f14160b0d912b5 |
| `success` | Copied or saved | 225 ms | -22.7 | -9.5 | 1332 | 2454 | take 5, ac3c1ced359b2ee9 |
| `toggle` | A meaningful switch changed | 200 ms | -22.5 | -10.1 | 1005 | 2333 | take 1, 5651cacaffd4cc71 |

## Prompts

Every prompt carries the same timbre sentence ("warm glassy marimba mallet tone, soft attack, clean and dry") so the set reads as one brand.

- `boot`: three note ascending arpeggio resolving upward, calm and confident, a gentle bloom on the last note. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `discovery-done`: two quick soft rising notes, a fifth apart, gentle level, tidy and satisfying like a completed scan. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `stack-up`: two notes rising a fourth then settling, warm and resolved, a small celebration. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `service-failed`: two notes falling a minor third, slightly firmer attack, serious but not alarming. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `agent-needs-you`: two identical notes then a higher third, a polite tap-tap-ask, inviting. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `agent-done`: single soft note with a short bright overtone, a quiet checkmark. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `update`: two notes rising a major second, light and optimistic. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `error`: one low muted note with a tiny downward slide, a soft damped thud that dies away quickly, not a buzzer. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `success`: single very short high tick, tiny and crisp. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.
- `toggle`: one extremely short damped muted tick, a tiny soft wooden tap with almost no ring, dying away within a fifth of a second. warm glassy marimba mallet tone, soft attack, clean and dry, no reverb tail, no background noise, no voice, user interface sound effect.

## Regenerating

`bun studio sounds generate [id…] --variants 4` then `bun studio sounds pick [id…]`. The ElevenLabs key is read from the
environment or `ATLAS_STUDIO_ENV` at run time only. Takes are cached (gitignored) in studio/.cache/sounds. `pick` records
what it shipped in provenance.json beside this file and rebuilds this README from it for every earcon on disk.
