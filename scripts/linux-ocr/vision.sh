#!/bin/sh
set -eu
parent=${0%/*}
root=$(CDPATH= cd -- "$parent/../ocr" && pwd)
exec "${AI_PHONE_PYTHON:?Bundled Python is required for offline OCR}" -I -S -B "$root/run.py"
