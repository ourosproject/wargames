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
