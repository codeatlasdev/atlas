Atlas is made by CodeAtlas LTDA. It is local-first: the CLI, Atlas Desktop and the phone app **Atlas: Dev Remote** have no accounts, telemetry, analytics, ads or crash reporting. We do not sell or share data, and we do not track you across apps or websites. Every network request the software makes is listed in [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md), next to the code that makes it.

<h2 id="mobile">The phone app (Atlas: Dev Remote)</h2>

The app shows and controls the Atlas running on your own computer. It has no account and talks only to the computers you pair it with.

- **On the phone:** a key generated on the phone (it never leaves it) and the list of paired computers (name, public key, network addresses), in the system's secure storage. "Forget this computer" deletes one; deleting the app deletes the rest (iOS can keep Keychain items).
- **Between the phone and your computer:** project and service state, logs, agent conversations, terminal output and what you type. End-to-end encrypted; only the two devices hold the keys. CodeAtlas never receives it.
- **Connection relays:** to find your computer, and to forward traffic when a direct connection is blocked, the app uses the public relays of number 0 (the makers of iroh). A relay sees both devices' IP addresses and public keys and how much data passes, never the content.
- **Notifications:** to tell you that a task finished or needs you, the app gets a device token from Firebase Cloud Messaging (Google). Your computer sends that token and the kind of event (`task_done` or `needs_you`) through `push.codeatlas.com.br`, operated by CodeAtlas, to Google. No message text, file names or prompts. The relay logs only the event kind and the result code. Turn notifications off in the system settings, or set `ATLAS_PUSH_RELAY=off` on the computer.
- **Camera:** used only to scan the pairing code; nothing is recorded or sent.
- **Face ID / biometrics:** checked by the system before you type in a terminal; the app only learns yes or no.

<h2 id="desktop">Atlas and Atlas Desktop</h2>

They check GitHub for updates (a plain request for the latest public release; Atlas Desktop has a switch to turn automatic checks off) and download releases from GitHub. Everything else stays on your computer. The commands, terminals, web pages and AI agents you run through Atlas make their own network requests; those are theirs, not Atlas's.

<h2 id="site">This website</h2>

A static site on Cloudflare Pages with no third-party scripts, fonts, ads or embeds. Cloudflare keeps standard server logs. If you sign up for release notes, we store your email address, the page language and the date to email you about releases, and remove it when you ask.

<h2 id="rights">Your data, your requests</h2>

Almost everything lives on your own devices, where you can delete it. For anything we hold (a release-notes email address, the relay's counts), or for any question, write to [luan@codeatlas.com.br](mailto:luan@codeatlas.com.br). Children: Atlas is a developer tool and is not directed to children under 13.

<h2 id="changes">Changes</h2>

Changes to this policy are made in the open, in the [history of this page](https://github.com/codeatlasdev/atlas/commits/main/site/src/data/legal/en/privacy.md). Last updated: 12 November 2026.
