# Balance re-measure — arsenal-as-data (moves as RON data)

**Date:** 2026-07-10
**Method:** deterministic heuristic (`WARGAME_SEED=<n> ./target/debug/purple-wargame cli`, no model).
Single-run model batches (qwen2.5:7b) are non-deterministic noise and are NOT used to judge balance.

**Result: BLUE 3/10 — wins on seeds 1, 3, 7. Matches the established baseline exactly.**

| seed | winner | detail |
|------|--------|--------|
| 1 | BLUE | held the line 8 rounds |
| 2 | RED  | Domain Admin in 4 rounds |
| 3 | BLUE | held the line 8 rounds |
| 4 | RED  | Domain Admin in 4 rounds |
| 5 | RED  | Domain Admin in 4 rounds |
| 6 | RED  | Domain Admin in 4 rounds |
| 7 | BLUE | held the line 8 rounds |
| 8 | RED  | Domain Admin in 4 rounds |
| 9 | RED  | Domain Admin in 4 rounds |
| 10 | RED | Domain Admin in 4 rounds |

Turning all 16 moves from Rust code into `.ron` data files is a pure representation change:
the win rate is unchanged (3/10 on {1,3,7}), the per-move equivalence tests proved each move
plays byte-identically to its former Rust card, and the frozen golden fixtures guard against
future drift. Because the heuristic run is deterministic, any future deviation from this table
is a real behavior change to investigate.

---

# Balance re-measure — arsenal primitives expansion (25-tool arsenal)

**Date:** 2026-07-10
**Method:** identical — deterministic heuristic, DEFAULT ruleset (DA-only win), no model.
Arsenal grown from 16 to 25 tools (new red families phish/deploy_implant/establish_c2/
exfil_data/ransomware + blue counters evict/block_egress/backups/block_c2) and the objective/
posture/stealth facts, the `Evict` effect, and data-defined `WinCondition`s.

**Result: BLUE 3/10 — wins on seeds 1, 3, 7. Zero drift from the pre-expansion baseline.**

| seed | winner | detail |
|------|--------|--------|
| 1 | BLUE | held the line 8 rounds |
| 2 | RED  | Domain Admin in 4 rounds |
| 3 | BLUE | held the line 8 rounds |
| 4 | RED  | Domain Admin in 4 rounds |
| 5 | RED  | Domain Admin in 4 rounds |
| 6 | RED  | Domain Admin in 4 rounds |
| 7 | BLUE | held the line 8 rounds |
| 8 | RED  | Domain Admin in 4 rounds |
| 9 | RED  | Domain Admin in 4 rounds |
| 10 | RED | Domain Admin in 4 rounds |

Why the DA race is undisturbed by nine new tools: (1) the DEFAULT win condition is still
`[have(DomainAdmin)]` — the new objective wins (silent heist, ransomware, etc.) only fire under
a custom ruleset, proved separately in `tests/compound_win.rs`; (2) the deterministic Red/Blue
heuristic preference lists are unchanged, so neither side reaches for a new tool during the
DA-beeline; (3) every new red primitive gates on a fact that today's default scenarios never
plant a path to (no exfil objective, no impact objective in the DA game), so they stay illegal
and off the menu. The new alphabet is purely *additive*: it enlarges what a custom ruleset can
express without perturbing the shipped game. Any future deviation from this table is a real
behavior change to investigate.
