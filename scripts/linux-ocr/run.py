"""Run the canonical OCR implementation with the bundled Pillow and engine."""
import os
from pathlib import Path
import runpy
import sys

root = Path(__file__).resolve().parent
sys.path.insert(0, str(root / 'python'))
os.environ['PATH'] = str(root / 'launchers')
os.environ['TESSDATA_PREFIX'] = str(root / 'tessdata')
runpy.run_path(str(root / 'vision.py'), run_name='__main__')
