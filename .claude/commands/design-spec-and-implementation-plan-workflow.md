---
name: design-spec-and-implementation-plan-workflow
description: Workflow command scaffold for design-spec-and-implementation-plan-workflow in wargames.
allowed_tools: ["Bash", "Read", "Write", "Grep", "Glob"]
---

# /design-spec-and-implementation-plan-workflow

Use this workflow when working on **design-spec-and-implementation-plan-workflow** in `wargames`.

## Goal

Documents a new feature or engine expansion by first writing a design spec, then an implementation plan, both committed as markdown files in the docs/superpowers directory.

## Common Files

- `wargame/docs/superpowers/specs/*.md`
- `wargame/docs/superpowers/plans/*.md`

## Suggested Sequence

1. Understand the current state and failure mode before editing.
2. Make the smallest coherent change that satisfies the workflow goal.
3. Run the most relevant verification for touched files.
4. Summarize what changed and what still needs review.

## Typical Commit Signals

- Write a design spec as a markdown file in wargame/docs/superpowers/specs/
- Commit the design spec with a docs(wargame): ...design spec message.
- Write an implementation plan as a markdown file in wargame/docs/superpowers/plans/
- Commit the implementation plan with a docs(wargame): ...implementation plan message.

## Notes

- Treat this as a scaffold, not a hard-coded script.
- Update the command if the workflow evolves materially.