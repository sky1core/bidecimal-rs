"""Verification-tool regressions; run with normal Python and with python -O."""
from pathlib import Path
import tempfile
import unittest
import check_interval_fixtures as checker


class FixtureValidation(unittest.TestCase):
    def setUp(self):
        self.lines = (Path(__file__).resolve().parents[1] / 'src' / 'interval_fixtures.tsv').read_text().splitlines()
        self.row = next(i for i, line in enumerate(self.lines) if line and not line.startswith('#'))

    def check(self, lines):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fixtures.tsv'
            path.write_text('\n'.join(lines) + '\n')
            return checker.check_fixtures(path)

    def mutated(self, column, value):
        lines = self.lines.copy()
        fields = lines[self.row].split('\t')
        fields[column] = value
        lines[self.row] = '\t'.join(fields)
        return lines

    def test_original_402(self):
        self.assertEqual(self.check(self.lines), 402)

    def test_invalid_schema_and_values_are_rejected(self):
        fields = self.lines[self.row].split('\t')
        for column, value in [(0, '2'), (1, '33'), (2, '-1'), (2, '1' + '0' * 16),
                              (3, hex(int(fields[3], 16) ^ 1)),
                              (4, hex(int(fields[4], 16) ^ 1))]:
            with self.subTest(column=column, value=value):
                with self.assertRaises(ValueError):
                    self.check(self.mutated(column, value))
        for count_delta in [-1, 1]:
            lines = self.lines.copy()
            if count_delta < 0:
                del lines[self.row]
            else:
                lines.append(lines[self.row])
            with self.subTest(count_delta=count_delta):
                with self.assertRaises(ValueError):
                    self.check(lines)
        lines = self.lines.copy()
        lines[self.row] += '\textra'
        with self.assertRaises(ValueError):
            self.check(lines)


if __name__ == '__main__':
    unittest.main()
