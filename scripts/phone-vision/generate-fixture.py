"""Generate the native OCR test image from the recorded DejaVu font; no font is bundled."""
import argparse
import hashlib
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont


def generate(font, output, expected_sha256):
    if hashlib.sha256(font.read_bytes()).hexdigest() != expected_sha256:
        raise ValueError('Font differs from the recorded fixture source')
    image = Image.new('RGB', (320, 112), 'white')
    ImageDraw.Draw(image).text((20, 10), '7319', font=ImageFont.truetype(str(font), 80), fill='black')
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output, format='PNG')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font', type=Path, required=True)
    parser.add_argument('--font-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    generate(args.font, args.output, args.font_sha256)
