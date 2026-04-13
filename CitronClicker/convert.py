from PIL import Image
import os

img = Image.open('strawberry.png')
# Resize to a square if it isn't
w, h = img.size
if w != h:
    size = min(w, h)
    img = img.crop(((w - size) // 2, (h - size) // 2, (w + size) // 2, (h + size) // 2))
img.save('strawberry.ico', format='ICO', sizes=[(256, 256), (128, 128), (64, 64), (32, 32), (16, 16)])
