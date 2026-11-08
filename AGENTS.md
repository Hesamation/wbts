# AGENTS.md — Who Broke the Swarm (reframe, 2026-11-07)

## CRITICAL THINKING DIRECTIVE (read this first)
The handover (codex-handover-reframe-20261107.md) is a PROPOSAL, not gospel. It was
written by an agent ("muse") and has NOT been independently verified. You are a
critical thinker, not an executor. When you encounter a claim, number, or design
decision in the handover:
- If it involves money flow correctness → verify the math yourself before coding it.
- If it cites a paper → verify the quote/number against the actual paper.
- If it says "first" or "novel" → search for counterexamples before asserting.
- If a number seems off → flag it to the user; do not silently use it.
- If you're unsure whether something is true → write "UNVERIFIED" in the code/README
  and report it to the user. Never assert an unverified claim as fact.
- The builder (human) wants rigor over speed. A flagged uncertainty is better than
  a false claim that a judge catches.
- Source of truth order: (1) primary sources (papers, NEAR docs, explorer), (2)
  morning-update-synthesis, (3) reframe-sharpening-addendum, (4) handover. The
  handover is LAST. If it conflicts with a primary source, the primary source wins.

## Verified nuances (from independent verification, 2026-11-07)
- The DeepMind paper describes TWO mechanisms: (a) optimistic challenge model (which we
  implement) and (b) game-theoretic verification game (Schelling point/TrueBit). We
  implement (a) only. README must say "optimistic challenge model," not "verification game."
- The paper says "post-hoc error discovery—even outside the dispute window—triggers a
  retroactive update to reputation." We do NOT implement post-hoc updates outside the
  dispute window (deliberate simplification, design principle 4). README must acknowledge this.
- Directory has 257 projects submitted (not 8 — earlier count was page 1 only). Novelty still holds (0 blame/attribution entries). Plan's track competition numbers are unreliable; re-pull before submission. Competition is real but our niche is unoccupied.
- Prior art details for Writ, FlashFirm, CartelBreaker are UNVERIFIED. Spot-check repos
  before citing specific numbers (3 levels/16 agents, 2-of-3 evaluators).

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
- ACTUAL INSTALLED: Rust 1.99.1, cargo-near 0.23.0, near-cli-rs 0.30.1. Check actual versions when debugging or validating builds.

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
- Work with the builder directly through Codex; no other agents are assumed.
- Keep each work item focused and diffs small. Summarize changes and validation for
  builder review before starting another work item.
- If a gate fails: STOP and report. Do not widen scope, re-prompt around a failed
  gate, or invent a workaround.
- Every factual project claim must map to a supporting artifact.
- No mandatory STATUS.md reads, shared-agent updates, or duplicate codex-log entries.
  Record durable decisions or unresolved issues in relevant project documentation
  when needed; report routine progress and validation in the conversation.
- The builder owns scope decisions, Gate-1 hand-run, final review, video, and submission.
  Codex handles implementation, tests, debugging, research, and documentation within
  the authorized work item.

## Contract methods (pin before code)
register / anchor / file_claim / challenge / adjudicate / finalize +
resolve_panel (stub, documented no-op) / resolve_panel_with_vrf (design-only, no code) +
views: verify_run, get_reputation, get_claim, get_run.

