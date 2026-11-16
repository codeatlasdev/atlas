#!/usr/bin/env python3
"""Frame check of a recorded app launch (docs/mobile.md, "Launch").

    scripts/mobile-launch-frames.py RECORDING.mp4 [--fps 60] [--ground 0B1320]

Cuts the video into frames and sorts every frame into one of
  GROUND  the app's ground colour everywhere (the logo/caption box at the centre and the system bars are
          not looked at), the only thing a launch may show before the first screen;
  SOLID   one flat colour that is not the ground: a white, black or off-colour frame, the bug this exists for;
  OTHER   anything with structure: the launcher, the OS opening animation, the first real screen.
and prints the run-length timeline with times from the start of the video and the colour of each SOLID run
(the ground is matched as the recording shows it: screenrecord/simctl shift dark colours by a few steps).
Exit 1 when a SOLID run
sits between two GROUND runs or after the first one (a flash inside the launch).

Needs ffmpeg, Pillow and numpy.
"""
import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("video")
    ap.add_argument("--fps", type=int, default=60)
    ap.add_argument("--ground", default="0B1320", help="hex, AtlasColors.dark.ground")
    ap.add_argument("--tol", type=int, default=3, help="per-channel distance still called the ground")
    args = ap.parse_args()
    nominal = np.array([int(args.ground[i : i + 2], 16) for i in (0, 2, 4)], dtype=np.int16)

    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run(
            ["ffmpeg", "-v", "error", "-i", args.video, "-vf", f"fps={args.fps},scale=270:-2:flags=area", f"{tmp}/f%05d.png"],
            check=True,
        )
        files = sorted(Path(tmp).glob("f*.png"))
        stats = []  # per frame: (mean rgb, worst channel std) of the looked-at pixels
        for f in files:
            img = np.asarray(Image.open(f).convert("RGB"), dtype=np.int16)
            h, w, _ = img.shape
            keep = np.ones((h, w), dtype=bool)
            keep[: int(h * 0.07)] = False  # status bar
            keep[int(h * 0.94) :] = False  # navigation bar / home indicator
            keep[int(h * 0.30) : int(h * 0.70), int(w * 0.20) : int(w * 0.80)] = False  # logo + caption
            px = img[keep]
            stats.append((px.mean(axis=0), float(px.std(axis=0).max())))

    # The recorder's H.264 (limited range YUV) shifts dark colours by a few steps, so the ground is taken as
    # the colour the video itself shows for it: the first flat colour within 12 steps of the nominal ground.
    # What matters is that every later frame matches it, and the shift is printed.
    seen = next((m for m, sd in stats if sd < 4 and np.abs(m - nominal).max() <= 12), nominal)
    runs: list[list] = []  # [class, first_i, last_i, detail]
    for i, (m, sd) in enumerate(stats):
        d = np.abs(m - seen).max()
        if sd < 4 and d <= args.tol:
            cls, detail = "GROUND", ""
        elif sd < 4:
            cls, detail = "SOLID", "#%02X%02X%02X" % tuple(int(round(v)) for v in m)
        else:
            cls, detail = "OTHER", ""
        if runs and runs[-1][0] == cls and runs[-1][3] == detail:
            runs[-1][2] = i
        else:
            runs.append([cls, i, i, detail])

    ms = lambda i: round(i * 1000 / args.fps)
    print(f"{len(files)} frames at {args.fps} fps; ground as recorded: #%02X%02X%02X (nominal #{args.ground.upper()})" % tuple(int(round(v)) for v in seen))
    for cls, a, b, detail in runs:
        extra = detail
        if cls == "GROUND":
            ms_ = np.array([stats[i][0] for i in range(a, b + 1)])
            extra = "worst channel distance from the first ground frame: %.1f" % np.abs(ms_ - seen).max()
        print(f"{ms(a):6d}-{ms(b + 1):6d} ms  {cls:6s} {extra}")
    first_ground = next((k for k, r in enumerate(runs) if r[0] == "GROUND"), None)
    # A SOLID run before the first GROUND run is the OS opening the app (launcher wallpaper fading).
    bad = [r for k, r in enumerate(runs) if r[0] == "SOLID" and first_ground is not None and k > first_ground]
    if first_ground is None:
        # A resume shows the last screen straight away: nothing to match, only flat frames to rule out.
        flat = [r for r in runs if r[0] == "SOLID"]
        if flat:
            print("FLASH: flat off-ground frames:", ", ".join(f"{ms(r[1])}ms {r[3]}" for r in flat))
            return 1
        print("no ground frame (a resume shows the last screen) and no flat frame")
        return 0
    if bad:
        print("FLASH: flat off-ground frames inside the launch:", ", ".join(f"{ms(r[1])}ms {r[3]}" for r in bad))
        return 1
    print("no flat off-ground frame after the first ground frame")
    return 0


if __name__ == "__main__":
    sys.exit(main())
