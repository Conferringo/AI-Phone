"""The OCR wire adapter must reject successful processes with the wrong format."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('linux_vision', Path(__file__).parent / 'phone-vision/tesseract.py')
vision = importlib.util.module_from_spec(spec); spec.loader.exec_module(vision)

HEADER = b'level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n'


class LinuxVisionTests(unittest.TestCase):
    def test_plain_text_from_missing_tesseract_config_is_an_explicit_failure(self):
        for data in (b'7319\n', b'', b'level\ttext\n5\t7319\n'):
            with self.assertRaisesRegex(ValueError, 'TSV'): vision.parse_regions(data, 320, 112)

    def test_valid_regions_preserve_confidence_and_image_bounds(self):
        data = HEADER + b'5\t1\t1\t1\t1\t1\t10\t20\t60\t30\t96.5\t7319\n'
        self.assertEqual(vision.parse_regions(data, 320, 112), [{'text': '7319', 'confidence': .965, 'bounds': {'left': 10, 'top': 20, 'right': 70, 'bottom': 50}}])
        self.assertEqual(vision.parse_regions(data, 30, 112), [])
        self.assertEqual(vision.parse_regions(data.replace(b'96.5', b'69.9'), 320, 112), [])
        self.assertEqual(vision.parse_regions(HEADER, 320, 112), [])


if __name__ == '__main__': unittest.main()
