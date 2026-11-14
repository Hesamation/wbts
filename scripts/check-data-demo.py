#!/usr/bin/env python3
"""Independently verify exported evidence, proof roots and exact wallet arithmetic."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / "demo/evidence"


def sha(data):
    return hashlib.sha256(data).digest()


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def opens(root, key, opening):
    entry = opening["entry"]
    if entry is not None:
        if entry["kind"] != "fact" or entry["topic"].lower() != key:
            return False
        node = sha(b"\x00" + bytes.fromhex(opening["salt"]) + canonical(entry))
    else:
        node = sha(b"\x02")
    slot = int.from_bytes(sha(b"\x03" + canonical(["fact", key]))[:4], "big")
    empty = sha(b"\x02")
    assert len(opening["path"]) == 32
    for sibling in opening["path"]:
        other = bytes.fromhex(sibling) if sibling is not None else empty
        node = sha(b"\x01" + (node + other if slot % 2 == 0 else other + node))
        slot //= 2
        empty = sha(b"\x01" + empty + empty)
    return node.hex() == root


def audit(row):
    assert row["conserved"] is True
    fields = ["locked", "available", "in_flight", "withdrawn", "retained_storage_charges"]
    assert all(int(row[key]) >= 0 for key in fields)
    assert int(row["deposits"]) == sum(int(row[key]) for key in fields)


def main():
    manifest = json.loads((EVIDENCE / "manifest.json").read_text())
    wasm = ROOT / "data-escrow/target/near/wbts_data_escrow.wasm"
    assert sha(wasm.read_bytes()).hex() == manifest["wasm_sha256"], "re-export evidence from current WASM"
    assert len(manifest["cases"]) == 6
    calls = proofs = rejected = 0
    for case in manifest["cases"]:
        assert case["file"] == case["id"] + ".json"
        data = (EVIDENCE / case["file"]).read_bytes()
        assert sha(data).hex() == case["sha256"], case["file"]
        record = json.loads(data)
        assert record["wasm_sha256"] == manifest["wasm_sha256"]
        assert all(record["checks"].values())
        job = record["job"]
        assert int(job["price"]) == sum(int(job[key]) for key in ["paid", "refunded", "remaining"])
        audit(record["accounting"])
        for key in ["locked", "available", "in_flight"]:
            assert record["accounting"][key] == "0"
        deposited = 0
        for step in record["steps"]:
            calls += 1
            audit(step["accounting"])
            if step["success"]:
                deposited += int(step["deposit_yocto"])
            else:
                rejected += 1
            assert int(step["accounting"]["deposits"]) == deposited
            if step["method"] == "deliver":
                proofs += 1
                key = job["keys"][step["args"]["index"]]
                valid = opens(job["root"], key, step["args"]["opening"])
                assert valid == step["success"], "exported proof does not match contract outcome"
        economic_sum = 0
        storage_sum = 0
        for row in record["wallet_reconciliation"]:
            account = row["account"]
            gas = sum(int(step["gas_paid_yocto"]) for step in record["steps"] if step["signer"] == account)
            assert gas == int(row["gas_paid_yocto"])
            wallet = int(record["final_wallets"][account]) - int(record["initial_wallets"][account])
            assert wallet == int(row["wallet_delta_yocto"])
            economic = wallet + gas + int(row["retained_storage_paid_yocto"]) + int(row["available_yocto"])
            assert economic == int(row["economic_delta_yocto"])
            expected = int(job["paid"]) if account == job["seller"] else -int(job["paid"]) if account == job["buyer"] else 0
            assert economic == expected
            economic_sum += economic
            storage_sum += int(row["retained_storage_paid_yocto"])
        assert economic_sum == 0
        assert storage_sum == int(record["accounting"]["retained_storage_charges"])
    print(f"PASS: six contract runs, {calls} signed calls, {proofs} proof attempts, {rejected} expected rejections; roots and wallets independently rechecked.")


if __name__ == "__main__":
    main()
