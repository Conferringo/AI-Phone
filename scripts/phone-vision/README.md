# Native OCR boundary

The helper accepts bounded PNG bytes and returns text regions plus a 32×64 grayscale grid. macOS uses Apple Vision; Linux uses Tesseract/Pillow. Confidence and coordinate checks apply before regions reach the phone runner.

`crates/phone-runner/fixtures/ocr-probe.png` is a generated 320×112 OCR self-test with ordinary rendered text `7319`. Its font and PNG hashes are recorded in `config/source-assets.json`; the DejaVu notice is retained without bundling a font file. Contributors can reproduce it with Pillow and the recorded font:

```sh
python3 scripts/phone-vision/generate-fixture.py --font /path/to/DejaVuSans.ttf --font-sha256 57f73e11f51999432bf7ab22ce55b6f945d5eca1bf824404cfa9ec2e3718c84e --output crates/phone-runner/fixtures/ocr-probe.png
```

The separate `crates/assistant/fixtures/vision-probe.png` remains the model-provider image probe. It uses pixel glyphs and is not the native OCR acceptance fixture. A successful self-test establishes startup and the sample's recognition; arbitrary fonts, app layouts and complete phone-use accuracy require separate evidence.
