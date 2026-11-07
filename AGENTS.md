# AGENTS.md — Who Broke the Swarm (reframe, 2026-11-07)

This file is written from `codex-handover-reframe-20261107.md`. Do NOT reuse the
old BlameDAG AGENTS.md (reserve, TLA+, OutLayer gate are dead for this build).

## Pick
Who Broke the Swarm — staked attribution on tamper-evident run logs. Tracks:
swarms (primary) + business + security. Solo builder + Codex (ChatGPT Plus).

## Frozen scope (do not widen)
- General mechanism: any agent count, any delegation depth, deterministic-verifier
  tasks only, checkable failure classes F1–F5a only.
- The 3-agent planner/worker/reviewer pipeline is the WORKED EXAMPLE, not the limit.
- No reserve, no marketplace, no token, no reputation layer beyond the §1 score,
  no live users, no depth >2, no Intents integration, no cross-chain.
- Anything outside scope is a README roadmap line, not a build item.

## Constants (Appendix A of the handover is the authority — never copy from v1)
- S = 1.0 NEAR stake/agent/run; B = C = 0.25 NEAR; panel bond = 0.1 NEAR.
- Slash = 0.25 NEAR; split 80% winner / 20% treasury.
- Challenge window 600 blocks (~6 min); demo 60 blocks (~36 s); panel window 86,400 blocks (~14.4 h); F2 timeout D = 60 blocks (demo).
- p* = 44.3% (WITH gas convention, everywhere); r* = 55.7%; t* = 55.7% (p=1).
- Reputation: base 100, cap 120, floor 0; true blame -10; false claim filed -15;
  successful challenge +5 (capped +5/run); exoneration +2.
- 100-event run cost: 0.13–0.22 NEAR (~0.15 pitch), mostly refundable storage stake.

## Toolchain (pinned)
- Rust 1.97.1, near-sdk 5.29.1, cargo-near 0.21.1 (0.22.0 is the fallback).
- near-cli-rs latest; Node 22.12+; Docker for reproducible builds.
- wasm32-unknown-unknown target required.

## Never-do (grep the repo before submitting)
- Never write "first implementation" unqualified — only "First *on-NEAR* implementation
  of Intelligent AI Delegation's §4.8 challenge game".
- Never "formally verified" / "verified system" / "model-checked" / "TEE-verified" /
  "attestation verified" / "cryptographically attested".
- Never "reserve" / "solvency reserve" / BlameDAG payout numbers (140/55/70).
- Never port BlameDAG.tla — no TLA+ model in this build.
- Never reintroduce the reserve or BlameDAG payout math.
- Never submit, push, post, or deploy to mainnet without the builder's explicit
  confirmation.
- Never touch the `website` submission field (spam trap — leave EMPTY).
- Never work around a Codex cap with a second account.
- Never fake a testnet failure — injection only via a labelled test-only path or mock token.
- Never mock data in the UI.
- No token, marketplace, or reputation system beyond the §1 score.
- No depth >2, multiple primes, or Intents integration.

## Working rules
- One focused Codex prompt per work item; small diffs; builder reads each diff before
  the next prompt.
- Morning bursts only; check `/status` before every Codex burst.
- If a gate fails: STOP and report. Do not widen scope, re-prompt around a failed
  gate, or invent a workaround.
- Source of truth order: (1) morning-update-synthesis, (2) reframe-sharpening-addendum,
  (3) this handover. The v1 design doc is overridden by the addendum (C1–C13).
- Every claim maps to an artifact. A claim without an artifact is a liability.

## Role split
- Builder: decisions, Gate-1 hand-run, README honesty by hand, video, submission.
- Codex: Rust contract (one method/prompt), CLI, sim tests, toolchain debugging.
- omp: research synthesis, red-teaming, docs/README drafting, verification, orchestration.

## Contract methods (pin before code)
register / anchor / file_claim / challenge / adjudicate / finalize +
resolve_panel (stub, documented no-op) / resolve_panel_with_vrf (design-only, no code) +
views: verify_run, get_reputation, get_claim, get_run.

## Shared collaboration
- Read `../collab/STATUS.md` first. Write updates there and to `codex-log/`.
- Do not copy-paste prompts between agents; use the shared files.
