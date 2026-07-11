---
name: arsenal-expansion-workflow
description: Workflow command scaffold for arsenal-expansion-workflow in wargames.
allowed_tools: ["Bash", "Read", "Write", "Grep", "Glob"]
---

# /arsenal-expansion-workflow

Use this workflow when working on **arsenal-expansion-workflow** in `wargames`.

## Goal

Adds new tools/primitives to the arsenal, including .ron data files, updates to arsenal.rs, and corresponding tests and taxonomy updates.

## Common Files

- `wargame/tools/*.ron`
- `wargame/src/arsenal.rs`
- `wargame/tests/taxonomy.rs`
- `wargame/tests/precondition_equivalence.rs`

## Suggested Sequence

1. Understand the current state and failure mode before editing.
2. Make the smallest coherent change that satisfies the workflow goal.
3. Run the most relevant verification for touched files.
4. Summarize what changed and what still needs review.

## Typical Commit Signals

- Create new .ron files for each new tool in wargame/tools/
- Update wargame/src/arsenal.rs to wire in the new tools.
- Update or add relevant tests in wargame/tests/ (e.g., taxonomy.rs, precondition_equivalence.rs).
- Update supporting files (e.g., bump arsenal count, update produces-lint logic).

## Notes

- Treat this as a scaffold, not a hard-coded script.
- Update the command if the workflow evolves materially.