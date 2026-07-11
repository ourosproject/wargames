```markdown
# wargames Development Patterns

> Auto-generated skill from repository analysis

## Overview

This skill teaches you how to contribute effectively to the `wargames` Rust codebase, which implements a modular game engine for simulating and analyzing wargame mechanics. You'll learn the project's coding conventions, how to propose and implement new features, expand the arsenal of tools, validate balance, and maintain high-quality tests. The repository emphasizes clear documentation, modular Rust code, and a workflow-driven approach to collaborative development.

## Coding Conventions

- **File Naming:**  
  Use `snake_case` for all Rust source and test files.  
  *Example:*  
  ```
  src/game_state.rs
  tests/compound_win.rs
  ```

- **Import Style:**  
  Use relative imports within the crate.  
  *Example:*  
  ```rust
  mod referee;
  use crate::rules::RuleSet;
  ```

- **Export Style:**  
  Use named exports for modules and functions.  
  *Example:*  
  ```rust
  pub mod arsenal;
  pub fn evaluate_move(...) { ... }
  ```

- **Commit Messages:**  
  Use [Conventional Commits](https://www.conventionalcommits.org/), with prefixes: `feat`, `docs`, `test`.  
  *Example:*  
  ```
  feat(wargame): add compound win condition logic
  docs(wargame): add design spec for new effect
  test(wargame): validate arsenal expansion
  ```

## Workflows

### Design Spec and Implementation Plan Workflow
**Trigger:** When proposing a significant new feature or engine change  
**Command:** `/new-design-plan`

1. Write a design spec as a markdown file in `wargame/docs/superpowers/specs/`.
2. Commit with a message like:  
   ```
   docs(wargame): add design spec for [feature]
   ```
3. Write an implementation plan as a markdown file in `wargame/docs/superpowers/plans/`.
4. Commit with a message like:  
   ```
   docs(wargame): add implementation plan for [feature]
   ```

*Example directory structure:*
```
wargame/docs/superpowers/specs/new_feature.md
wargame/docs/superpowers/plans/new_feature_plan.md
```

---

### Arsenal Expansion Workflow
**Trigger:** When adding new tools, effects, or techniques to the arsenal  
**Command:** `/expand-arsenal`

1. Create new `.ron` files for each tool in `wargame/tools/`.
2. Update `wargame/src/arsenal.rs` to wire in the new tools.
3. Update or add relevant tests in `wargame/tests/` (e.g., `taxonomy.rs`, `precondition_equivalence.rs`).
4. Update supporting files as needed (e.g., bump arsenal count, update produces-lint logic).
5. Commit with a message like:  
   ```
   feat(wargame): add [tool] to arsenal
   ```

*Example:*
```
wargame/tools/emp_blast.ron
wargame/src/arsenal.rs
wargame/tests/taxonomy.rs
```

---

### Feature Implementation and Test Workflow
**Trigger:** When implementing a new game mechanic, rule, or fact  
**Command:** `/new-engine-feature`

1. Update relevant Rust source files in `wargame/src/` (e.g., `main.rs`, `referee.rs`, `rules.rs`, `state.rs`, `facts.rs`, `effects.rs`).
2. Update or add relevant test files in `wargame/tests/` (including fixtures in `wargame/tests/fixtures/*.json` as needed).
3. Commit with a message like:  
   ```
   feat(wargame): implement [feature]
   ```

*Example code snippet:*
```rust
// src/effects.rs
pub fn apply_emp_blast(state: &mut GameState) { ... }
```

---

### Test and Balance Validation Workflow
**Trigger:** When validating correctness or measuring balance after a feature/arsenal change  
**Command:** `/validate-balance`

1. Add or update test files in `wargame/tests/` (e.g., `compound_win.rs`, `balance_note.md`).
2. Commit with a message like:  
   ```
   test(wargame): add balance validation for [feature]
   ```

*Example:*
```
wargame/tests/compound_win.rs
wargame/tests/balance_note.md
```

## Testing Patterns

- **Test Framework:**  
  The specific Rust test framework is not specified, but standard Rust test conventions apply.

- **Test File Naming:**  
  Place test files in `wargame/tests/` using `snake_case` and `.rs` extension.  
  *Example:*  
  ```
  wargame/tests/taxonomy.rs
  wargame/tests/precondition_equivalence.rs
  ```

- **Test Structure:**  
  Use Rust's built-in test module structure.  
  *Example:*  
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn test_emp_blast_effect() {
          // test logic
      }
  }
  ```

- **Fixtures:**  
  Place JSON fixtures in `wargame/tests/fixtures/` as needed.

## Commands

| Command              | Purpose                                                      |
|----------------------|--------------------------------------------------------------|
| /new-design-plan     | Start a new design spec and implementation plan workflow     |
| /expand-arsenal      | Add new tools/effects to the arsenal                         |
| /new-engine-feature  | Implement a new engine feature or rule                       |
| /validate-balance    | Add or update tests and balance validation                   |
```