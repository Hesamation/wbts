"""Synchronize/check CLI source identity; never accesses the network."""
import argparse
import hashlib
from pathlib import Path
import re

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true")
args = parser.parse_args()
manifest = bytearray(b"wbts/embedded-t2/source-manifest/v1\0")
for name in ("src/lib.rs", "src/t2.rs", "Cargo.toml", "Cargo.lock"):
    data = (root / "contract" / name).read_bytes()
    manifest.extend(len(data).to_bytes(8, "little"))
    manifest.extend(data)
pin = hashlib.sha256(manifest).hexdigest()
path = root / "cli/src/lib.rs"
source = path.read_text()
updated, count = re.subn(r'pub const PIN: &str = "[a-f0-9]{64}";', f'pub const PIN: &str = "{pin}";', source)
assert count == 1, "expected exactly one CLI verifier pin"
if args.check:
    if source != updated:
        raise SystemExit("CLI verifier identity is stale; run python3 scripts/sync-verifier-pin.py")
else:
    path.write_text(updated)
print(f"CLI verifier source identity: {pin}")
