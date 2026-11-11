#!/usr/bin/env python3
"""Build the self-contained static site. Uses only the Python standard library."""
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent
DIST = ROOT / "dist"
DIST.mkdir(exist_ok=True)
for name in ("index.html", "privacy.html", "support.html", "styles.css", "script.js"):
    shutil.copy2(ROOT / name, DIST / name)
(DIST / "assets").mkdir(exist_ok=True)
icon = ROOT.parent / "assets" / "icon-rounded.png"
if not icon.is_file():
    # Standalone hosting source includes a copy of the shared brand asset.
    icon = ROOT / "assets" / "icon-rounded.png"
shutil.copy2(icon, DIST / "assets" / "icon-rounded.png")
print(f"Built Carryover website: {DIST}")
