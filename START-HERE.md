# FactSettle — start here

**Current build, November 15, 2026:** proof-backed payments for agent data jobs on
NEAR. The visible product was renamed from Who Broke the Swarm; repository paths,
contract identifiers and historical evidence retain their original names.

A buyer funds specific fact requests against a seller-published dataset root.
The seller accepts. Each valid opening credits its agreed share without another
buyer approval. Proven absence or expired unfinished work returns the relevant
share to the buyer. Withdrawals complete through checked transfer callbacks.

## Review the product

1. [README](README.md): problem, judge walkthrough, reproduction and limits.
2. [Execution room](demo/README.md): six recorded local contract executions.
3. [Judge guide](demo/guide.html): short review route and evidence boundaries.
4. [Contract and client](data-escrow/README.md): exact behavior and commands.

The hosted viewer is a static replay, not a live testnet deployment. Its proof
checker runs in the browser. The contract proves consistency with the chosen
dataset, not the dataset's external truth, general reasoning or fresh inference.
Delivered entries and proofs are public. Scripted actors use distinct simulator
signing accounts; no live customer or independent commercial operator is claimed.

## Evidence and current checks

The contract release at `3c19289` passed [GitHub CI](https://github.com/Hesamation/wbts/actions/runs/43631842501):
42 simulator tests, seven CLI tests, builds, lint and the six-case exporter.
The export contains 39 signed calls, six proof attempts and two expected
rejections. `python3 scripts/check-data-demo.py` independently checks hashes,
proof roots and exact accounting. These results are not an audit or live receipts.
Presentation-only changes after that release do not change the recorded WASM.

## Continue development

- Preserve the pinned dependencies and the source/evidence distinction.
- Run checks proportionate to actual changes. Viewer changes need JavaScript,
  link/evidence checks and browser verification; unchanged contracts do not need
  another full toolchain build merely for a rename.
- Routine private commits and pushes are authorized. Do not rewrite shared history.
- The human authorized public demo hosting on November 15. The GitHub repository
  remains private under the scoped hosting decision. Do not silently broaden it.
- Video comes last under the latest instruction. X posting and submission remain
  human-owned; deployment of a NEAR contract or token spending needs its own
  applicable authorization.
- Keep local status, submission data and `codex-log/` ignored and out of Git.

## Earlier project and research

The original optimistic T2 attribution contract in `contract/` is preserved
unchanged. It is separate from `data-escrow/` and is not the current product pitch.
No migration of its stakes or deployed state is implemented.

[Archived WBTS handoff](WBTS-HISTORY.md) preserves the earlier mechanism,
measurements, paused-state investigation and continuation proposals.
[Research register](handoff/RESEARCH.md) links the historical supporting material.
Read those for a specific question, rather than treating old status as current.
