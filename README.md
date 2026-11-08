# Who Broke the Swarm

Staked attribution on tamper-evident run logs, using the optimistic challenge model
with deterministic adjudication for direct T2 F3/F1 claims.

**Killer beat:** coordinator falsely blames a passing worker; a watcher challenges;
the claim is overturned — the reporter loses 0.25 NEAR, the challenger earns 0.20 NEAR,
and no innocent agent is blamed: the corrected attribution is `none`.

The 3-agent pipeline is the worked example. The current adjudicator supports direct
T2 F3/F1 claims; nested traversal and post-hoc reputation updates outside the dispute
window are not implemented.

## Status
Contract Phase 2.18 adds claimant-burden expiry for unresolved challenges.
Fresh deployment and CLI S2 regression passed with exact money and
baseline-relative reputation checks. See the [live evidence](codex-log/2.18-live.md),
[CLI instructions](cli/README.md) and [local status](STATUS.md).

## Tracks
swarms (primary) + business + security.

## License
TBD (any license allowed by hackathon rules).

---

*Further submission documentation remains Phase 9 work. Every external claim needs
supporting evidence.*
