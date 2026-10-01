"""Check requested live Rust memory after repeated compiler success/failure.

This runs a separate process per source, without rendering or native services.
It is not an RSS, GPU, GUI or whole-runtime memory plateau test.
"""
from pathlib import Path
import argparse
import json
import subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--profiler', type=Path, required=True)
args = parser.parse_args()
for name, valid in [('pages.plm', True), ('declared-services.plm', True),
                    ('service-lists.plm', True), ('service-field-that-does-not-exist.plm', False),
                    ('pages-invalid-after-expansion.plm', False)]:
    run = subprocess.run([str(args.profiler.resolve()), '--compile-check', str(root / 'tests' / name), '50'],
                         cwd=root, capture_output=True, text=True, encoding='utf-8', timeout=60)
    rows = [line for line in run.stdout.splitlines() if line.startswith('{')]
    assert rows, f'{name}: no compiler allocation report\n{run.stdout}\n{run.stderr}'
    data = json.loads(rows[-1])
    growth = data['after_bytes'] - data['before_bytes']
    assert data['valid'] == valid and data['iterations'] == 50, (name, data)
    assert run.returncode == 0 and growth <= 4096, f'{name}: retained {growth} bytes after 50 compilations'
    assert len(data['samples']) == 50 and max(data['samples']) <= data['before_bytes'] + 4096, (name, data)
    print(f'PASS: {name}, 50 compilations, retained {growth} requested Rust bytes')
