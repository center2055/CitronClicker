from PIL import Image

# Open the image
img = Image.open('strawberry.png').convert("RGBA")

# Get data
data = img.getdata()
new_data = []

# Replace white-ish background with transparent
for item in data:
    # Check if pixel is close to white
    if item[0] > 240 and item[1] > 240 and item[2] > 240:
        new_data.append((255, 255, 255, 0))
    else:
        new_data.append(item)

img.putdata(new_data)

# Crop to square
w, h = img.size
if w != h:
    size = min(w, h)
    img = img.crop(((w - size) // 2, (h - size) // 2, (w + size) // 2, (h + size) // 2))

# Save transparent PNG
img.save('strawberry_transparent.png', 'PNG')

# Save ICO
img.save('strawberry.ico', format='ICO', sizes=[(256, 256), (128, 128), (64, 64), (32, 32), (16, 16)])
