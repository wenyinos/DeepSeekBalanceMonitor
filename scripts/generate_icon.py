"""Generate app icon in the same style as the tray icon - multi-resolution ICO."""
from PIL import Image, ImageDraw, ImageFont

_BASE = 256
_R = 28
_FILL = (60, 105, 102, 255)  # darker teal
_OUTLINE = (255, 255, 255, 60)

img = Image.new("RGBA", (_BASE, _BASE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

draw.rounded_rectangle([0, 0, _BASE, _BASE], radius=_R, fill=_FILL)
draw.rounded_rectangle([0, 0, _BASE, _BASE], radius=_R, outline=_OUTLINE, width=3)

try:
    font = ImageFont.truetype("segoeuib.ttf", 160)
except Exception:
    try:
        font = ImageFont.truetype("segoeui.ttf", 160)
    except Exception:
        try:
            font = ImageFont.truetype("arialbd.ttf", 150)
        except Exception:
            try:
                font = ImageFont.truetype("arial.ttf", 150)
            except Exception:
                font = ImageFont.load_default()

draw.text((_BASE / 2, _BASE / 2), "D", fill=(255, 255, 255, 255),
          font=font, anchor="mm")

ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64),
             (96, 96), (128, 128), (256, 256)]
img.save("app_icon.ico", format="ICO", sizes=ico_sizes)
print(f"app_icon.ico generated - sizes: {[f'{w}x{h}' for w, h in ico_sizes]}")
