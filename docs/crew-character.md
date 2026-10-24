# Crew character — handoff for Matheus

## Straw hat default

Every crewmate's default hat is our own straw hat:

- Rounded brim: wide, flat ellipse
- Simple dome crown
- A band line in the bot's accent color — **not red** (the One Piece red band is off-limits)
- A thin globe-meridian arc on the crown

This is the **first hat** in the Hat row, displayed before the existing hat options.

## Editor layout

Reference screenshot: `/Users/luancoleto/Library/Application Support/CleanShot/media/media_5T60I230nd/CleanShot 2026-10-23 at 01.48.32@2x.png`

- Big live preview at the top — updates on every pick with a small "pop" animation on change.
- Below the preview: crewmate name + one-line role.
- Each row shows the row label on the left and the selected option's name on the right (e.g. "Shell · Mint", "Hat · Beret", "Eyewear · Glasses", "Extras · Bow").
- Each row contains option tiles; the selected tile gets a tinted background + border highlight.
- The first tile of every row is **None** (rendered as a slash mark).
- The agents list beside the editor shows each agent's full look (hat, eyewear, extras) in its row avatar, and "Drafting •••" in accent color while that agent is working.

## Rows to implement

| Row | Options |
|---|---|
| Shell | Color swatches (palette) + custom hex input |
| Hat | Straw (default, new), then existing hats |
| Eyewear | None, Glasses, Shades, Round glasses, Eyepatch |
| Extras | None, Scarf, Bandana, Earring, Compass pendant |

## "Surprise me"

- Randomizes all four rows at once.
- The live preview animates through the change (small pop or morph, not a hard cut).

## hostd look validation

The `look` object sent to `bot_create` / `bot_update` / `bot_list` will need to accept the new `eyewear` and `extras` fields once you add them to the editor. Validation lives in hostd; the agents on the desktop side are not touching it — that's your call.
