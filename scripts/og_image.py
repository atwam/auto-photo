"""Renders the 1200x630 social preview card to web/public/og.png.

Usage: .venv/bin/pip install pillow && .venv/bin/python scripts/og_image.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parent.parent
assets = root / "crates/photo-core/assets"
bold = lambda s: ImageFont.truetype(str(assets / "Inter-Bold.ttf"), s)  # noqa: E731
regular = lambda s: ImageFont.truetype(str(assets / "Inter-Regular.ttf"), s)  # noqa: E731

W, H = 1200, 630
BG, FG, MUTED, ACCENT = "#0b0d10", "#f2f4f7", "#9aa3ad", "#49e3c4"

img = Image.new("RGB", (W, H), BG)
d = ImageDraw.Draw(img)

# Passport photo frame with a head-and-shoulders silhouette (right side).
fx, fy, fw, fh = 790, 95, 330, 424
photo = Image.new("RGB", (fw, fh), "#e9edf1")
p = ImageDraw.Draw(photo)
cx = fw / 2
p.ellipse((cx - 72, 92, cx + 72, 272), fill="#2a3038")
p.pieslice((cx - 150, 300, cx + 150, 600), 180, 360, fill="#2a3038")
mask = Image.new("L", (fw, fh), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, fw - 1, fh - 1), radius=22, fill=255)
img.paste(photo, (fx, fy), mask)
d.rounded_rectangle((fx - 14, fy - 14, fx + fw + 14, fy + fh + 14), radius=30, outline=ACCENT, width=6)
# Guide lines (eye line / chin / crown).
for y in (fy + 150, fy + 92, fy + 272):
    d.line((fx + 18, y, fx + fw - 18, y), fill=ACCENT, width=2)

# Brand + copy (left side).
x = 80
d.rounded_rectangle((x, 92, x + 40, 142), radius=8, outline=ACCENT, width=5)
d.ellipse((x + 12, 105, x + 28, 121), fill=ACCENT)
font = bold(44)
d.text((x + 62, 92), "auto", font=font, fill=FG)
w = d.textlength("auto", font=font)
d.text((x + 62 + w, 92), "·", font=font, fill=ACCENT)
w += d.textlength("·", font=font)
d.text((x + 62 + w, 92), "photo", font=font, fill=FG)

d.text((x, 205), "Passport & ID photos,", font=bold(62), fill=FG)
d.text((x, 280), "made on your device.", font=bold(62), fill=ACCENT)
d.multiline_text(
    (x, 385),
    "Auto crop to the official size, light & background\nfixes, compliance check, print-ready sheet.",
    font=regular(29),
    fill=MUTED,
    spacing=12,
)
d.text((x, 510), "Free · runs in your browser · nothing uploaded", font=bold(26), fill=FG)

out = root / "web/public/og.png"
img.save(out, optimize=True)
print(out, out.stat().st_size, "bytes")
