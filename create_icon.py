from PIL import Image, ImageDraw, ImageFilter

def create_omnitune_icon():
    size = 256
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # 1. Background Rounded Squircle with gradient/glow
    margin = 12
    radius = 52
    
    # Outer subtle glow
    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    g_draw = ImageDraw.Draw(glow)
    g_draw.rounded_rectangle([margin-4, margin-4, size-margin+4, size-margin+4], radius=radius+4, fill=(59, 130, 246, 70))
    glow = glow.filter(ImageFilter.GaussianBlur(8))
    img = Image.alpha_composite(glow, img)
    draw = ImageDraw.Draw(img)

    # Main Card Base
    draw.rounded_rectangle([margin, margin, size-margin, size-margin], radius=radius, fill=(15, 23, 42, 255), outline=(59, 130, 246, 200), width=4)

    # Inner subtle highlight ring
    draw.rounded_rectangle([margin+5, margin+5, size-margin-5, size-margin-5], radius=radius-4, outline=(30, 58, 138, 120), width=2)

    # 2. Electric Lightning Bolt shape
    # Points centered in 256x256
    # Coordinates of lightning bolt:
    points = [
        (138, 38),   # top tip
        (82, 138),   # mid left notch
        (128, 138),  # mid inner indent
        (108, 218),  # bottom tip
        (178, 118),  # mid right notch
        (134, 118),  # mid inner right
    ]

    # Bolt Shadow/Glow
    bolt_glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg_draw = ImageDraw.Draw(bolt_glow)
    bg_draw.polygon(points, fill=(245, 158, 11, 160))
    bolt_glow = bolt_glow.filter(ImageFilter.GaussianBlur(6))
    img = Image.alpha_composite(img, bolt_glow)
    draw = ImageDraw.Draw(img)

    # Main Bolt (Gold/Amber gradient fill simulation)
    draw.polygon(points, fill=(251, 191, 36, 255), outline=(254, 243, 199, 255), width=2)

    # High-energy core dot
    draw.ellipse([126, 126, 138, 138], fill=(255, 255, 255, 220))

    # Save multi-size icon
    sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    img.save("app.ico", format="ICO", sizes=sizes)
    print("Created app.ico successfully!")

if __name__ == "__main__":
    create_omnitune_icon()
