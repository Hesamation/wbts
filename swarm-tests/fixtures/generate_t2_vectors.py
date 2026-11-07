"""Independent exact-reference vectors: Python integers do not overflow at u128.
Reproduce with python3 swarm-tests/fixtures/generate_t2_vectors.py.
"""
import json
from pathlib import Path
from random import Random

rng = Random(20261108)
maximum = 2**128 - 1
pairs = [(0, 250), (1, 0), (1, 10_000), (9_999, 1), (10_000, 1),
         (maximum, 1), (maximum, 250), (maximum, 9_999), (maximum, 10_000)]
pairs += [(rng.randrange(maximum + 1), rng.randrange(10_001)) for _ in range(8)]
vectors = []
for total, bps in pairs:
    fee = total * bps // 10_000
    vectors.append({"total": str(total), "fee_basis_points": bps,
                    "output": json.dumps({"fee": str(fee), "net": str(total - fee)}, separators=(",", ":"))})
Path(__file__).with_name("t2_integer_vectors.json").write_text(json.dumps(vectors, indent=2) + "\n")
