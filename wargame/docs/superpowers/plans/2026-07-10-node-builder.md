# Move builder (guided form) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `/build` dashboard page to author single-step attack/defense moves through a form, validated by the arsenal validator, saved to `~/.purple-range/tools/` (folder-as-truth: playable in the next match, persistent across sessions), loaded fail-soft alongside the built-ins.

**Architecture:** Authored moves are `.ron` `ToolDef` files in a home folder. A new `registry_with_authored(dir)` = built-ins + valid authored files (malformed ones skipped with a warning). The interactive play/catalog path uses it; the CLI/balance path stays on built-ins-only `default_registry()`. A flat `MoveDraft` JSON DTO from the form maps to a one-node `ToolDef`, validated then serialized to RON and written. The form's dropdowns come from a `/api/vocabulary` endpoint assembled from the engine, so they never drift.

**Tech Stack:** Rust, axum, serde, serde_json, `ron` (already deps). Vanilla HTML/JS front-end matching the existing dark dashboard.

## Global Constraints

- Crate: `purple-wargame` at `~/Developer/production/purple-range/wargame`. Test with `cargo test`; build pristine (no warnings).
- **`default_registry()` stays built-ins-only (exactly 16).** Do NOT change it. The golden/taxonomy/balance guards depend on it. Authored moves load only through the new `registry_with_authored`.
- Authored folder: `~/.purple-range/tools/` (`$HOME/.purple-range/tools`). Created on first save. Home folder, not the repo.
- **Ids are sanitized to `[a-z0-9_]+` (length ≤ 64) before ever touching a file path.** Reject anything else. Never build a path from unsanitized input.
- A move id that collides with a built-in id, or an existing authored id, is rejected on save. Built-ins are read-only (no overwrite, no delete).
- A malformed authored `.ron` at load is skipped with a logged warning — never fatal. Built-ins remain fail-loud inside `default_registry()`.
- Techniques come from the existing `Technique` enum only. Effects exposed: all except `Produce`. Access: LAN, no auth.
- Work on a branch `move-builder` cut from `main`. Commit after every task.

---

## File structure

- **Modify** `src/tool.rs` — add `Serialize` to `ToolDef`/`Node`/`Guard` (so a move can be written back to RON).
- **Modify** `src/category.rs` — add `Category::from_key`. **Modify** `src/facts.rs` — add `Fact::from_key`. **Modify** `src/effects.rs` — add `StateFlag::from_key` + an effect-descriptor list.
- **Modify** `src/arsenal.rs` — add `authored_dir()`, `registry_with_authored(dir)`, `to_ron(&ToolDef)`, and a `vocabulary()` JSON assembler.
- **Create** `src/builder.rs` — the `MoveDraft` DTO, `draft_to_tooldef`, `slug`, and save/list/delete file helpers.
- **Modify** `src/main.rs` — add the 4 endpoints + `/build` route; point the play/catalog path at `registry_with_authored`.
- **Modify** `src/lib.rs` — add `pub mod builder;`.
- **Create** `public/build.html` — the form page.
- **Modify** `public/wargame.html` — add a "Build" link.

---

## Task 1: Make a move writable back to RON

**Files:** Modify `src/tool.rs`; add a helper + test in `src/arsenal.rs`.

**Interfaces:**
- Produces: `ToolDef`/`Node`/`Guard` derive `Serialize`; `arsenal::to_ron(&ToolDef) -> Result<String, String>`.

- [ ] **Step 1: Write the failing test.** Add to the `tests` module in `src/arsenal.rs`:

```rust
    #[test]
    fn tool_round_trips_through_ron_serialization() {
        // parse a built-in, serialize it back to RON, re-parse — must be identical.
        let original = parse_tool(TOOL_FILES[3]).unwrap(); // kerberoast (3-node composite)
        let ron = to_ron(&original).expect("serialize");
        let reparsed = parse_tool(&ron).unwrap_or_else(|e| panic!("re-parse failed: {e}\n---\n{ron}"));
        assert_eq!(serde_json::to_value(&original).unwrap(), serde_json::to_value(&reparsed).unwrap());
    }
```

- [ ] **Step 2: Run it — expect FAIL** (`to_ron` undefined; `ToolDef` not `Serialize`).

Run: `cargo test --lib arsenal::tests::tool_round_trips_through_ron_serialization`

- [ ] **Step 3: Add `Serialize`.** In `src/tool.rs`, change the three derive lines to add `Serialize` (they currently derive `Debug, Clone, Deserialize`):

```rust
// on struct Guard, struct Node, struct ToolDef:
#[derive(Debug, Clone, Serialize, Deserialize)]
```

And update the import at the top of `src/tool.rs`:

```rust
use serde::{Deserialize, Serialize};
```

- [ ] **Step 4: Add `to_ron`.** In `src/arsenal.rs`, add (above the tests):

```rust
/// Serialize a move to RON text for writing to an authored file. Emits struct names
/// (e.g. `ToolDef(...)`) so authored files read like the hand-written built-ins, and
/// re-parse cleanly via `parse_tool`.
pub fn to_ron(def: &ToolDef) -> Result<String, String> {
    let cfg = ron::ser::PrettyConfig::default().struct_names(true);
    ron::ser::to_string_pretty(def, cfg).map_err(|e| format!("RON serialize error: {e}"))
}
```

- [ ] **Step 5: Run tests — expect PASS.**

Run: `cargo test --lib arsenal::`

- [ ] **Step 6: Commit.**

```bash
git add src/tool.rs src/arsenal.rs
git commit -m "feat(wargame): ToolDef is Serialize + arsenal::to_ron (write moves back to RON)"
```

---

## Task 2: from_key helpers on the alphabet

**Files:** Modify `src/category.rs`, `src/facts.rs`, `src/effects.rs`.

**Interfaces:**
- Produces: `Category::from_key(&str) -> Option<Category>`, `Fact::from_key(&str) -> Option<Fact>`, `StateFlag::from_key(&str) -> Option<StateFlag>`. (`Technique::from_key` already exists.)

- [ ] **Step 1: Write the failing tests.**

In `src/category.rs` tests module:
```rust
    #[test]
    fn category_from_key_round_trips() {
        for c in [Category::Harden, Category::Isolate, Category::Evict, Category::CredentialAccess, Category::Detection] {
            assert_eq!(Category::from_key(c.key()), Some(c));
        }
        assert_eq!(Category::from_key("nope"), None);
    }
```
In `src/facts.rs` tests module:
```rust
    #[test]
    fn fact_from_key_round_trips() {
        for f in Fact::ALL {
            assert_eq!(Fact::from_key(f.key()), Some(f));
        }
        assert_eq!(Fact::from_key("nope"), None);
    }
```
In `src/effects.rs` tests module:
```rust
    #[test]
    fn state_flag_from_key_round_trips() {
        for (k, want) in [("monitoring", StateFlag::Monitoring), ("path_severed", StateFlag::PathSevered), ("domain_admin", StateFlag::DomainAdmin)] {
            assert_eq!(StateFlag::from_key(k), Some(want));
        }
        assert_eq!(StateFlag::from_key("nope"), None);
    }
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.**

In `src/category.rs`, add to `impl Category` (mirror `key()` exactly):
```rust
    /// Inverse of `key()`.
    pub fn from_key(k: &str) -> Option<Category> {
        Some(match k {
            "initial_access" => Category::InitialAccess,
            "discovery" => Category::Discovery,
            "credential_access" => Category::CredentialAccess,
            "privilege_escalation" => Category::PrivilegeEscalation,
            "lateral_movement" => Category::LateralMovement,
            "exfiltration" => Category::Exfiltration,
            "detection" => Category::Detection,
            "defense_evasion" => Category::DefenseEvasion,
            "reconnaissance" => Category::Reconnaissance,
            "resource_development" => Category::ResourceDevelopment,
            "execution" => Category::Execution,
            "persistence" => Category::Persistence,
            "collection" => Category::Collection,
            "command_and_control" => Category::CommandAndControl,
            "impact" => Category::Impact,
            "harden" => Category::Harden,
            "isolate" => Category::Isolate,
            "evict" => Category::Evict,
            "deceive" => Category::Deceive,
            "model" => Category::Model,
            _ => return None,
        })
    }
```

In `src/facts.rs`, add to `impl Fact` (mirror `key()`):
```rust
    /// Inverse of `key()`.
    pub fn from_key(k: &str) -> Option<Fact> {
        Fact::ALL.into_iter().find(|f| f.key() == k)
    }
```

In `src/effects.rs`, add to `impl StateFlag`:
```rust
    /// Stable slug for this flag (matches the surfaced fact key it flips).
    pub fn key(&self) -> &'static str {
        match self {
            StateFlag::Monitoring => "monitoring",
            StateFlag::AutoResponse => "auto_response",
            StateFlag::PathSevered => "path_severed",
            StateFlag::AesEnforced => "aes_enforced",
            StateFlag::PreauthEnforced => "preauth_enforced",
            StateFlag::DomainAdmin => "domain_admin",
        }
    }
    /// Inverse of `key()`.
    pub fn from_key(k: &str) -> Option<StateFlag> {
        Some(match k {
            "monitoring" => StateFlag::Monitoring,
            "auto_response" => StateFlag::AutoResponse,
            "path_severed" => StateFlag::PathSevered,
            "aes_enforced" => StateFlag::AesEnforced,
            "preauth_enforced" => StateFlag::PreauthEnforced,
            "domain_admin" => StateFlag::DomainAdmin,
            _ => return None,
        })
    }
```

- [ ] **Step 4: Run — expect PASS** (`cargo test --lib category:: facts:: effects::`).

- [ ] **Step 5: Commit.**

```bash
git add src/category.rs src/facts.rs src/effects.rs
git commit -m "feat(wargame): from_key inverses for Category/Fact/StateFlag (builder reflection)"
```

---

## Task 3: Load authored moves (fail-soft) alongside built-ins

**Files:** Modify `src/arsenal.rs`.

**Interfaces:**
- Produces: `authored_dir() -> std::path::PathBuf`, `registry_with_authored(dir: &std::path::Path) -> CardRegistry`, and (internal) reuse of `parse_tool`/`validate`/`registry_from_sources`.

- [ ] **Step 1: Write the failing tests.** Add to `src/arsenal.rs` tests:

```rust
    #[test]
    fn registry_with_authored_includes_valid_and_skips_broken() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("pw_authored_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // a valid authored move
        let good = r#"ToolDef(id: "my_test_move", side: Blue, technique: Recon, category: Detection,
            summary: "test", gate: [Category(fact: Monitoring, want: false)], produces: [Monitoring],
            nodes: [ Node(id: "my_test_move", effect: SetFlag(Monitoring), ok_narrative: "on") ])"#;
        std::fs::File::create(dir.join("my_test_move.ron")).unwrap().write_all(good.as_bytes()).unwrap();
        // a broken authored move (bad RON)
        std::fs::File::create(dir.join("broken.ron")).unwrap().write_all(b"ToolDef( this is not ron").unwrap();

        let reg = registry_with_authored(&dir);
        assert_eq!(reg.len(), 17, "16 built-ins + 1 valid authored (broken one skipped)");
        assert!(reg.get("my_test_move").is_some());
        // built-ins-only registry is unchanged
        assert_eq!(default_registry().len(), 16);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_with_authored_handles_missing_dir() {
        let dir = std::env::temp_dir().join("pw_authored_does_not_exist_zzz");
        let _ = std::fs::remove_dir_all(&dir);
        let reg = registry_with_authored(&dir);
        assert_eq!(reg.len(), 16, "missing authored dir → just the built-ins");
    }
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.** Add to `src/arsenal.rs`:

```rust
use std::path::{Path, PathBuf};

/// The home folder that holds a player's authored moves: `$HOME/.purple-range/tools`.
pub fn authored_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".purple-range").join("tools")
}

/// The built-in arsenal PLUS every valid authored `.ron` in `dir`. A malformed or invalid
/// authored file is logged to stderr and skipped — never fatal (unlike the built-ins, which
/// `default_registry` panics on). A missing dir yields just the built-ins.
pub fn registry_with_authored(dir: &Path) -> CardRegistry {
    let mut reg = default_registry(); // built-ins (panics if a built-in is bad — intended)
    let existing: std::collections::HashSet<String> = reg.all_specs().iter().map(|s| s.id.clone()).collect();
    let mut seen = existing.clone();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return reg, // no authored dir yet
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "ron").unwrap_or(false))
        .collect();
    paths.sort(); // deterministic load order

    for path in paths {
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => { eprintln!("[arsenal] skip {}: {e}", path.display()); continue; }
        };
        let def = match parse_tool(&src) {
            Ok(d) => d,
            Err(e) => { eprintln!("[arsenal] skip {}: {e}", path.display()); continue; }
        };
        if let Err(errs) = validate(&def) {
            eprintln!("[arsenal] skip {}: {}", path.display(), errs.join("; "));
            continue;
        }
        if seen.contains(&def.id) {
            eprintln!("[arsenal] skip {}: id '{}' collides with an existing move", path.display(), def.id);
            continue;
        }
        seen.insert(def.id.clone());
        reg.register(Box::new(DataTool::new(def)));
    }
    reg
}
```

- [ ] **Step 4: Run — expect PASS** (`cargo test --lib arsenal::`).

- [ ] **Step 5: Commit.**

```bash
git add src/arsenal.rs
git commit -m "feat(wargame): registry_with_authored — fail-soft load of authored moves + authored_dir"
```

---

## Task 4: The vocabulary endpoint payload

**Files:** Modify `src/effects.rs` (effect descriptors), `src/arsenal.rs` (`vocabulary()` assembler).

**Interfaces:**
- Produces: `effects::effect_descriptors() -> Vec<serde_json::Value>` (excludes `Produce`); `arsenal::vocabulary() -> serde_json::Value`.

- [ ] **Step 1: Write the failing test.** Add to `src/arsenal.rs` tests:

```rust
    #[test]
    fn vocabulary_lists_the_palette_without_produce() {
        let v = vocabulary();
        let effects: Vec<&str> = v["effects"].as_array().unwrap().iter().map(|e| e["key"].as_str().unwrap()).collect();
        assert!(effects.contains(&"SetFlag") && effects.contains(&"GrantCred") && effects.contains(&"HuntGap"));
        assert!(!effects.contains(&"Produce"), "Produce is hidden until the canvas phase");
        // facts, probes, categories, techniques, sides all present and non-empty
        assert_eq!(v["facts"].as_array().unwrap().len(), 11);
        assert!(v["probes"].as_array().unwrap().len() >= 8);
        assert!(v["categories"].as_array().unwrap().iter().any(|c| c["key"] == "harden"));
        assert!(v["techniques"].as_array().unwrap().iter().any(|t| t["key"] == "kerberoast"));
        assert_eq!(v["sides"].as_array().unwrap().len(), 2);
        assert!(v["state_flags"].as_array().unwrap().iter().any(|f| f["key"] == "aes_enforced"));
    }
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement the effect descriptors.** In `src/effects.rs`, add:

```rust
/// Palette metadata for the builder: each authorable effect (Produce is excluded — it is only
/// meaningful inside a multi-step chain, which the guided form does not build). `params` names
/// the fields the form must collect; `kind` tells the form which input to render.
pub fn effect_descriptors() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({ "key": "Attempt", "label": "Perform the technique (no state change)",
                "desc": "Just performs the technique; the referee records it (this is how recon/bloodhound leave 'scouted' behind).", "params": [] }),
        json!({ "key": "Advance", "label": "Move one zone closer",
                "desc": "Takes the next forward hop toward the objective. Use {dest} in the narrative for the zone name.", "params": [] }),
        json!({ "key": "SetFlag", "label": "Flip a defense/progress switch on",
                "desc": "Turns one on/off switch true (e.g. AES enforced, path severed, monitoring).",
                "params": [ { "name": "flag", "kind": "state_flag" } ] }),
        json!({ "key": "GrantCred", "label": "Steal a credential",
                "desc": "Adds a cracked credential.",
                "params": [ { "name": "principal", "kind": "text" }, { "name": "secret", "kind": "text", "optional": true }, { "name": "via", "kind": "technique" } ] }),
        json!({ "key": "RevokeKnownCreds", "label": "Cancel detected stolen credentials",
                "desc": "Cancels any cracked credential whose technique the defender has already detected. It decides which — no parameters.", "params": [] }),
        json!({ "key": "HuntGap", "label": "Threat-hunt for the top undetected technique",
                "desc": "Finds the highest-value technique the attacker performed but you haven't detected, and surfaces it. It picks the target itself — no parameters.", "params": [] }),
        json!({ "key": "DeployDetection", "label": "Write a detection rule",
                "desc": "Writes a graded detection rule for a technique (the move takes a 'technique' parameter at play time).", "params": [] }),
        json!({ "key": "SeverForwardEdges", "label": "Cut the network in front of the attacker",
                "desc": "Drops the attacker's forward network edges. It computes which — no parameters.", "params": [] }),
    ]
}
```

- [ ] **Step 4: Implement `vocabulary()`.** In `src/arsenal.rs`, add:

```rust
/// The full palette the builder form renders from — assembled from the engine so it can never
/// drift from what the engine actually supports.
pub fn vocabulary() -> serde_json::Value {
    use serde_json::json;
    use crate::category::Category;
    use crate::effects::StateFlag;
    use crate::facts::Fact;
    use crate::state::{Side, Technique};

    let categories: Vec<serde_json::Value> = [
        Category::InitialAccess, Category::Discovery, Category::CredentialAccess, Category::PrivilegeEscalation,
        Category::LateralMovement, Category::Exfiltration, Category::Detection, Category::DefenseEvasion,
        Category::Reconnaissance, Category::ResourceDevelopment, Category::Execution, Category::Persistence,
        Category::Collection, Category::CommandAndControl, Category::Impact, Category::Harden, Category::Isolate,
        Category::Evict, Category::Deceive, Category::Model,
    ].iter().map(|c| json!({ "key": c.key(), "defensive": c.is_defensive(),
        "enforced": Category::ENFORCED.contains(c) })).collect();

    let techniques: Vec<serde_json::Value> = [
        Technique::InitialAccess, Technique::Recon, Technique::Pivot, Technique::Kerberoast, Technique::AsRepRoast,
        Technique::BloodHound, Technique::CredSpray, Technique::LateralMove, Technique::Exfil,
    ].iter().map(|t| json!({ "key": t.as_key(), "label": t.attack_name() })).collect();

    let facts: Vec<serde_json::Value> = Fact::ALL.iter().map(|f| json!({
        "key": f.key(), "question": f.question(),
        "side": if f.audience() == Side::Red { "Red" } else { "Blue" },
    })).collect();

    let state_flags: Vec<serde_json::Value> = [
        StateFlag::Monitoring, StateFlag::AutoResponse, StateFlag::PathSevered, StateFlag::AesEnforced,
        StateFlag::PreauthEnforced, StateFlag::DomainAdmin,
    ].iter().map(|f| json!({ "key": f.key() })).collect();

    // Probes the form can gate on. `arg` = what the form must collect for this probe.
    let probes = json!([
        { "key": "SawCategory", "arg": "category", "label": "Seen any activity in a tactic (cheap)" },
        { "key": "Identified", "arg": "technique", "label": "Has a deployed detection rule for a technique" },
        { "key": "Vuln", "arg": "technique", "label": "The attack path for a technique is planted this scenario" },
        { "key": "Performed", "arg": "technique", "label": "The attacker has performed a technique" },
        { "key": "Detected", "arg": "technique", "label": "Blue has an alert for a technique" },
        { "key": "HasForwardPath", "arg": null, "label": "The attacker has a forward hop to take" },
        { "key": "LateralPathPlanted", "arg": null, "label": "A DCSync-able ACL path exists this scenario" },
        { "key": "CredCompromiseKnown", "arg": null, "label": "A stolen credential the defender has detected exists" },
        { "key": "UndetectedActivity", "arg": null, "label": "Some performed technique is not yet detected" },
        { "key": "UndetectedAlert", "arg": null, "label": "Some alert has no detection rule yet" },
    ]);

    json!({
        "sides": ["Red", "Blue"],
        "categories": categories,
        "techniques": techniques,
        "facts": facts,
        "state_flags": state_flags,
        "probes": probes,
        "effects": crate::effects::effect_descriptors(),
    })
}
```

- [ ] **Step 5: Run — expect PASS** (`cargo test --lib arsenal::tests::vocabulary_lists_the_palette_without_produce`).

- [ ] **Step 6: Commit.**

```bash
git add src/effects.rs src/arsenal.rs
git commit -m "feat(wargame): vocabulary() palette + effect descriptors for the builder"
```

---

## Task 5: The builder core (DTO → ToolDef, sanitize, save/list/delete)

**Files:** Create `src/builder.rs`; modify `src/lib.rs`.

**Interfaces:**
- Produces:
  - `builder::slug(name: &str) -> String` (sanitized `[a-z0-9_]`, ≤64, collapses runs of non-alnum to `_`).
  - `builder::draft_to_tooldef(draft: &MoveDraft) -> Result<ToolDef, String>` (single-node move; also sets `params_schema` for `DeployDetection`).
  - `builder::check(draft, reg) -> Result<ToolDef, Vec<String>>` (validate structural + collision, NO write — backs the Validate button).
  - `builder::save(dir, draft, reg) -> Result<ToolDef, Vec<String>>` (`check` → write `<id>.ron`).
  - `builder::list(dir) -> serde_json::Value` (built-ins tagged read-only + authored tagged editable, with full defs).
  - `builder::delete(dir, id) -> Result<(), String>` (authored-only, sanitized id, refuse built-in ids).
  - `MoveDraft` and its row/effect sub-types (serde `Deserialize`).

- [ ] **Step 1: Add module.** In `src/lib.rs`: `pub mod builder;`.

- [ ] **Step 2: Write the failing tests.** Create `src/builder.rs` with the test module first:

```rust
//! The builder core: turn a form-shaped `MoveDraft` into a validated one-node `ToolDef`, and
//! save/list/delete authored move files. The front-end is a thin client over this + `arsenal`.

use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

use crate::arsenal::{self, to_ron};
use crate::category::Category;
use crate::effects::{Effect, StateFlag};
use crate::facts::{Fact, InstanceProbe, Requirement};
use crate::state::{Side, Technique};
use crate::tool::{Guard, Node, ToolDef};

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> MoveDraft {
        MoveDraft {
            name: "My Test Move".into(), side: "Blue".into(), category: "harden".into(),
            technique: "kerberoast".into(), summary: "test".into(), narrative: "did it".into(),
            gate: vec![
                GateRow::Fact { fact: "aes_enforced".into(), want: false },
                GateRow::Probe { probe: "Identified".into(), arg: Some("kerberoast".into()), want: true },
            ],
            effect: EffectDraft { kind: "SetFlag".into(), params: json!({ "flag": "aes_enforced" }) },
            detection_surface: vec![],
            produces: vec!["aes_enforced".into()],
        }
    }

    #[test]
    fn slug_sanitizes_and_rejects_traversal() {
        assert_eq!(slug("My Phish!!"), "my_phish");
        assert_eq!(slug("../../etc/passwd"), "etc_passwd");
        assert_eq!(slug("A B  C"), "a_b_c");
        assert!(!slug("../../etc/passwd").contains('/') && !slug("../../etc").contains('.'));
    }

    #[test]
    fn draft_maps_to_a_valid_one_node_tooldef() {
        let def = draft_to_tooldef(&draft()).expect("map");
        assert_eq!(def.id, "my_test_move");
        assert_eq!(def.side, Side::Blue);
        assert_eq!(def.category, Category::Harden);
        assert_eq!(def.nodes.len(), 1);
        assert_eq!(def.nodes[0].id, "my_test_move");
        assert!(matches!(def.nodes[0].effect, Effect::SetFlag(StateFlag::AesEnforced)));
        assert_eq!(def.gate.len(), 2);
        arsenal::validate(&def).expect("the mapped move validates");
    }

    #[test]
    fn save_writes_lists_and_deletes() {
        let dir = std::env::temp_dir().join(format!("pw_builder_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        let def = save(&dir, &draft(), &reg).expect("save");
        assert_eq!(def.id, "my_test_move");
        assert!(dir.join("my_test_move.ron").exists());

        let listed = list(&dir);
        let ids: Vec<&str> = listed["moves"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"my_test_move") && ids.contains(&"kerberoast"));
        // built-in flagged read-only, authored editable
        let mine = listed["moves"].as_array().unwrap().iter().find(|m| m["id"] == "my_test_move").unwrap();
        assert_eq!(mine["authored"], true);
        let builtin = listed["moves"].as_array().unwrap().iter().find(|m| m["id"] == "kerberoast").unwrap();
        assert_eq!(builtin["authored"], false);

        delete(&dir, "my_test_move").expect("delete authored");
        assert!(!dir.join("my_test_move.ron").exists());
        assert!(delete(&dir, "kerberoast").is_err(), "cannot delete a built-in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_validates_without_writing() {
        let dir = std::env::temp_dir().join(format!("pw_builder_check_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        // a bad draft: claims to produce a fact its effect never establishes
        let mut bad = draft();
        bad.produces = vec!["path_severed".into()]; // SetFlag(AesEnforced) does not set path_severed
        let errs = check(&bad, &reg).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("path_severed")), "got {errs:?}");
        assert!(!dir.exists(), "check must not create the dir or write anything");
    }

    #[test]
    fn save_rejects_builtin_id_collision() {
        let dir = std::env::temp_dir().join(format!("pw_builder_collide_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        let mut d = draft();
        d.name = "kerberoast".into(); // collides with a built-in
        let errs = save(&dir, &d, &reg).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("kerberoast") && e.to_lowercase().contains("exist")), "got {errs:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 3: Run — expect FAIL.**

- [ ] **Step 4: Implement.** Insert above the tests in `src/builder.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct MoveDraft {
    pub name: String,
    pub side: String,
    pub category: String,
    pub technique: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub narrative: String,
    #[serde(default)]
    pub gate: Vec<GateRow>,
    pub effect: EffectDraft,
    #[serde(default)]
    pub detection_surface: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum GateRow {
    Fact { fact: String, want: bool },
    Probe { probe: String, #[serde(default)] arg: Option<String>, want: bool },
    AnyOf { of: Vec<GateRow> },
}

#[derive(Debug, Deserialize)]
pub struct EffectDraft {
    pub kind: String,
    #[serde(default)]
    pub params: Value,
}

/// Sanitize a display name into a safe move id: lowercase, `[a-z0-9]` kept, every other run
/// becomes a single `_`, trimmed of leading/trailing `_`, capped at 64. Never contains `/` or `.`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
    }
    while out.ends_with('_') { out.pop(); }
    out.truncate(64);
    while out.ends_with('_') { out.pop(); }
    out
}

fn technique(k: &str) -> Result<Technique, String> {
    Technique::from_key(k).ok_or_else(|| format!("unknown technique '{k}'"))
}

fn build_probe(name: &str, arg: &Option<String>) -> Result<InstanceProbe, String> {
    let t = || technique(arg.as_deref().unwrap_or(""));
    let cat = || Category::from_key(arg.as_deref().unwrap_or("")).ok_or_else(|| format!("unknown category '{:?}'", arg));
    Ok(match name {
        "SawCategory" => InstanceProbe::SawCategory(cat()?),
        "Identified" => InstanceProbe::Identified(t()?),
        "Vuln" => InstanceProbe::Vuln(t()?),
        "Performed" => InstanceProbe::Performed(t()?),
        "Detected" => InstanceProbe::Detected(t()?),
        "HasForwardPath" => InstanceProbe::HasForwardPath,
        "LateralPathPlanted" => InstanceProbe::LateralPathPlanted,
        "CredCompromiseKnown" => InstanceProbe::CredCompromiseKnown,
        "UndetectedActivity" => InstanceProbe::UndetectedActivity,
        "UndetectedAlert" => InstanceProbe::UndetectedAlert,
        other => return Err(format!("unknown probe '{other}'")),
    })
}

fn build_requirement(row: &GateRow) -> Result<Requirement, String> {
    Ok(match row {
        GateRow::Fact { fact, want } => {
            let f = Fact::from_key(fact).ok_or_else(|| format!("unknown fact '{fact}'"))?;
            Requirement::Category { fact: f, want: *want }
        }
        GateRow::Probe { probe, arg, want } => {
            Requirement::Instance { probe: build_probe(probe, arg)?, want: *want }
        }
        GateRow::AnyOf { of } => {
            Requirement::AnyOf(of.iter().map(build_requirement).collect::<Result<Vec<_>, _>>()?)
        }
    })
}

fn build_effect(e: &EffectDraft) -> Result<Effect, String> {
    let p = &e.params;
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    Ok(match e.kind.as_str() {
        "Attempt" => Effect::Attempt,
        "Advance" => Effect::Advance,
        "SetFlag" => {
            let f = s("flag").ok_or("SetFlag needs a 'flag'")?;
            Effect::SetFlag(StateFlag::from_key(&f).ok_or_else(|| format!("unknown flag '{f}'"))?)
        }
        "GrantCred" => Effect::GrantCred {
            principal: s("principal").ok_or("GrantCred needs a 'principal'")?,
            secret: s("secret"),
            via: technique(&s("via").ok_or("GrantCred needs a 'via' technique")?)?,
        },
        "RevokeKnownCreds" => Effect::RevokeKnownCreds,
        "HuntGap" => Effect::HuntGap,
        "DeployDetection" => Effect::DeployDetection,
        "SeverForwardEdges" => Effect::SeverForwardEdges,
        other => return Err(format!("unknown or unsupported effect '{other}'")),
    })
}

/// Map a form draft into a validated-shape single-node ToolDef (not yet run through `validate`).
pub fn draft_to_tooldef(draft: &MoveDraft) -> Result<ToolDef, String> {
    let id = slug(&draft.name);
    if id.is_empty() {
        return Err("name produced an empty id — use letters or digits".into());
    }
    let side = match draft.side.as_str() {
        "Red" => Side::Red, "Blue" => Side::Blue,
        other => return Err(format!("unknown side '{other}'")),
    };
    let category = Category::from_key(&draft.category).ok_or_else(|| format!("unknown category '{}'", draft.category))?;
    let tech = technique(&draft.technique)?;
    let gate = draft.gate.iter().map(build_requirement).collect::<Result<Vec<_>, _>>()?;
    let effect = build_effect(&draft.effect)?;
    let ok_surface = draft.detection_surface.iter().map(|k| technique(k)).collect::<Result<Vec<_>, _>>()?;
    let produces = draft.produces.iter()
        .map(|k| Fact::from_key(k).ok_or_else(|| format!("unknown fact '{k}'")))
        .collect::<Result<Vec<_>, _>>()?;
    // DeployDetection carries a params schema (it reads params.technique at play time).
    let params_schema = if matches!(effect, Effect::DeployDetection) {
        Some(json!({ "type": "object", "properties": { "technique": { "type": "string" } }, "required": ["technique"] }))
    } else { None };

    Ok(ToolDef {
        id: id.clone(), side, technique: tech, category, summary: draft.summary.clone(),
        gate, produces, params_schema,
        nodes: vec![Node {
            id, requires: vec![], produces_keys: vec![], guards: Vec::<Guard>::new(),
            effect, ok_surface, ok_narrative: draft.narrative.clone(),
        }],
    })
}

/// Validate a draft WITHOUT writing anything: map to a ToolDef, run structural validation, and
/// check the id doesn't collide (built-in or existing authored). This backs the non-destructive
/// "Validate" button. Returns the mapped ToolDef or the plain-language errors.
pub fn check(draft: &MoveDraft, reg: &crate::registry::CardRegistry) -> Result<ToolDef, Vec<String>> {
    let def = draft_to_tooldef(draft).map_err(|e| vec![e])?;
    arsenal::validate(&def)?; // runnable / no dangling / leaves-behind
    if reg.get(&def.id).is_some() {
        return Err(vec![format!("a move named '{}' already exists — pick a different name", def.id)]);
    }
    Ok(def)
}

/// Validate (via `check`) then persist the draft as `<id>.ron` in `dir`. Returns the saved
/// ToolDef or the plain-language errors.
pub fn save(dir: &Path, draft: &MoveDraft, reg: &crate::registry::CardRegistry) -> Result<ToolDef, Vec<String>> {
    let def = check(draft, reg)?;
    let ron = to_ron(&def).map_err(|e| vec![e])?;
    std::fs::create_dir_all(dir).map_err(|e| vec![format!("cannot create {}: {e}", dir.display())])?;
    let path = dir.join(format!("{}.ron", def.id));
    std::fs::write(&path, ron).map_err(|e| vec![format!("cannot write {}: {e}", path.display())])?;
    Ok(def)
}

/// The set of built-in ids (an authored move may not use one).
fn builtin_ids() -> std::collections::HashSet<String> {
    arsenal::default_registry().all_specs().iter().map(|s| s.id.clone()).collect()
}

/// List every move: built-ins (read-only) + authored (editable), each with its full definition
/// (as JSON) so the form can load one as a reference or for editing.
pub fn list(dir: &Path) -> Value {
    let builtins = builtin_ids();
    let reg = arsenal::registry_with_authored(dir);
    let moves: Vec<Value> = reg.all_specs().iter().map(|s| {
        json!({ "id": s.id, "side": format!("{:?}", s.side), "category": s.category.key(),
                "technique": s.technique.as_key(), "summary": s.summary,
                "authored": !builtins.contains(&s.id) })
    }).collect();
    json!({ "moves": moves })
}

/// Delete an authored move file. Rejects built-in ids and sanitizes the id before touching disk.
pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    let safe = slug(id);
    if safe != id || safe.is_empty() {
        return Err(format!("invalid id '{id}'"));
    }
    if builtin_ids().contains(&safe) {
        return Err(format!("'{safe}' is a built-in move and cannot be deleted"));
    }
    let path = dir.join(format!("{safe}.ron"));
    if !path.exists() {
        return Err(format!("no authored move '{safe}'"));
    }
    std::fs::remove_file(&path).map_err(|e| format!("cannot delete {}: {e}", path.display()))
}
```

- [ ] **Step 5: Run — expect PASS** (`cargo test --lib builder::`).

- [ ] **Step 6: Full suite + pristine.** Run `cargo test` and `cargo build` (0 warnings).

- [ ] **Step 7: Commit.**

```bash
git add src/builder.rs src/lib.rs
git commit -m "feat(wargame): builder core — MoveDraft→ToolDef mapping, sanitized save/list/delete"
```

---

## Task 6: HTTP endpoints + wire the play path

**Files:** Modify `src/main.rs`.

**Interfaces:**
- Consumes: `builder::{MoveDraft, save, list, delete}`, `arsenal::{vocabulary, registry_with_authored, authored_dir}`.
- Produces routes: `GET /api/vocabulary`, `GET /api/tools`, `POST /api/tools`, `POST /api/tools/validate`, `DELETE /api/tools/:id`, `GET /build`.

- [ ] **Step 1: Add handlers.** In `src/main.rs`, add these handlers (near `catalog`), and add imports `use purple_wargame::builder::{self, MoveDraft};` and `use purple_wargame::arsenal::{vocabulary, registry_with_authored, authored_dir};` (adjust to the file's existing `use` style):

```rust
async fn vocabulary_api() -> Json<Value> {
    Json(vocabulary())
}

async fn list_tools() -> Json<Value> {
    Json(builder::list(&authored_dir()))
}

async fn save_tool(Json(draft): Json<MoveDraft>) -> Json<Value> {
    let dir = authored_dir();
    let reg = registry_with_authored(&dir);
    match builder::save(&dir, &draft, &reg) {
        Ok(def) => Json(json!({ "ok": true, "id": def.id })),
        Err(errs) => Json(json!({ "ok": false, "errors": errs })),
    }
}

/// Non-destructive validation for the "Validate" button — checks the draft but writes nothing.
async fn validate_tool(Json(draft): Json<MoveDraft>) -> Json<Value> {
    let reg = registry_with_authored(&authored_dir());
    match builder::check(&draft, &reg) {
        Ok(def) => Json(json!({ "ok": true, "id": def.id })),
        Err(errs) => Json(json!({ "ok": false, "errors": errs })),
    }
}

async fn delete_tool(Path(id): Path<String>) -> Json<Value> {
    match builder::delete(&authored_dir(), &id) {
        Ok(()) => Json(json!({ "ok": true })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

async fn build_page() -> Html<&'static str> {
    Html(include_str!("../public/build.html"))
}
```

- [ ] **Step 2: Register routes.** In `main()`'s router, add:

```rust
        .route("/build", get(build_page))
        .route("/api/vocabulary", get(vocabulary_api))
        .route("/api/tools", get(list_tools).post(save_tool))
        .route("/api/tools/validate", post(validate_tool))
        .route("/api/tools/:id", axum::routing::delete(delete_tool))
```

- [ ] **Step 3: Wire the play/catalog path to authored moves.** Change `catalog` and the human-match referee to include authored moves; leave `run_cli` and the autonomous `game` SSE on the built-ins.

In `catalog`, change `let reg = default_registry();` to:
```rust
    let reg = registry_with_authored(&authored_dir());
```

Find the `new_match` handler (it builds a `Referee`). Change its referee construction from `default_registry()` / `new_referee()` to use `registry_with_authored(&authored_dir())`. Add a helper near `new_referee`:
```rust
fn play_referee() -> Referee {
    Referee { rules: RuleSet { max_rounds: 8, ..RuleSet::default() }, registry: registry_with_authored(&authored_dir()) }
}
```
and use `play_referee()` where `new_match` builds the match's referee. Leave `new_referee()` (built-ins) used by `run_cli` and `game` untouched.

- [ ] **Step 4: Create a placeholder page so the crate compiles.** Create `public/build.html` with a minimal placeholder (the real page is Task 7) so `include_str!` resolves:

```html
<!DOCTYPE html><html><head><meta charset="utf-8"><title>Build</title></head>
<body><p>Builder loading…</p></body></html>
```

- [ ] **Step 5: Build + manual endpoint check.**

Run: `cargo build` (0 warnings), then start the server and check the endpoints:
```bash
WARGAME_PORT=4851 ./target/debug/purple-wargame &
sleep 1
curl -s localhost:4851/api/vocabulary | head -c 200; echo
curl -s localhost:4851/api/tools | head -c 300; echo
curl -s -X POST localhost:4851/api/tools -H 'content-type: application/json' \
  -d '{"name":"My Test","side":"Blue","category":"detection","technique":"recon","summary":"t","narrative":"on","gate":[{"kind":"Fact","fact":"monitoring","want":false}],"effect":{"kind":"SetFlag","params":{"flag":"monitoring"}},"detection_surface":[],"produces":["monitoring"]}'
echo
curl -s localhost:4851/api/tools | python3 -c "import sys,json; print([m['id'] for m in json.load(sys.stdin)['moves'] if m['authored']])"
curl -s -X DELETE localhost:4851/api/tools/my_test
kill %1
```
Expected: vocabulary JSON; tools list of 16; POST returns `{"ok":true,"id":"my_test"}`; the authored list shows `['my_test']`; DELETE returns ok. Clean up: `rm -f ~/.purple-range/tools/my_test.ron`.

- [ ] **Step 6: Full suite green, then commit.**

```bash
git add src/main.rs public/build.html
git commit -m "feat(wargame): /api/vocabulary + /api/tools CRUD + /build route; play path loads authored moves"
```

---

## Task 7: The Build page (front-end)

**Files:** Replace `public/build.html`; modify `public/wargame.html` (add a Build link).

This task has no unit tests — it is built and verified by driving it in a browser. Match the existing dark dashboard aesthetic (read `public/wargame.html` for its palette/type/spacing and reuse them; background `#070a12`).

**The page must:**

- On load, `GET /api/vocabulary` and `GET /api/tools`; render from that data (never hard-code the lists).
- Provide a form that collects a `MoveDraft` and these fields:
  - **Name** (text) → show the derived id live (lowercase `[a-z0-9_]`); warn if it matches an id already in the tools list.
  - **Side** (Red/Blue), **Category**, **Technique** — selects from vocabulary. Group categories by defensive/attack; show reserved ones as available.
  - **Gate** — a repeatable list of rows; each row is either a Fact (a fact select + have/lack) or a Probe (a probe select; if the probe's `arg` is `technique` or `category`, show that select; a want toggle). Support one level of an "Any of" group. Show a soft inline warning if a Red move gates on a Blue-audience fact or vice-versa (from each fact's `side`).
  - **Effect** — a select of the vocabulary effects; render only the chosen effect's `params` (state_flag select / text / technique select), showing its `desc`.
  - **Detection surface** — multi-select of techniques; default to the chosen technique.
  - **Narrative** — text.
  - **Leaves behind (produces)** — multi-select of facts; **auto-fill** a sensible default when the effect/technique changes (SetFlag→its flag's fact; GrantCred→has_cred; Advance→foothold; technique Recon→scouted; BloodHound→path_mapped+scouted), editable.
- Build the `MoveDraft` JSON exactly matching the Task-5 DTO (`gate` rows tagged with `kind: "Fact"|"Probe"|"AnyOf"`; `effect: {kind, params}`).
- **Validate button** (non-destructive): POST the draft to `/api/tools/validate`; on `ok:false` show `errors` in plain language inline, on `ok:true` show a green "valid" note. Writes nothing.
- **Save button**: POST the draft to `/api/tools`; on `ok:false` show the same inline errors, on `ok:true` it's written — refresh the moves list and show the "Test in a match" affordance.
- **Live preview**: show the JSON draft being built (a readable box), so the user sees what will be saved.
- **Moves list**: render `/api/tools`; built-ins (`authored:false`) shown as read-only chips that, when clicked, load their summary into the form as a starting reference (fetch not required — the list already carries id/side/category/technique/summary; a "start from this" fills those selects). Authored moves (`authored:true`) get **Delete** (confirm dialog → `DELETE /api/tools/:id`, then refresh the list).
- On successful save, show a success note and a **"Test in a match"** button/link that opens the play dashboard (`/`) — starting a new match there will include the saved move.

**Also:** in `public/wargame.html`, add a visible **Build** link (to `/build`) in the header/nav, matching the existing controls.

- [ ] **Step 1: Read `public/wargame.html`** to learn the dashboard's CSS variables, fonts, and header structure to match.

- [ ] **Step 2: Write `public/build.html`** implementing the above (vanilla HTML/CSS/JS, no external deps — the dashboard is air-gapped; do not add CDN links).

- [ ] **Step 3: Add the Build link** to `public/wargame.html`.

- [ ] **Step 4: Verify in a browser.** Start the server (`./target/debug/purple-wargame`), open `http://localhost:4850/build`, and confirm end-to-end:
  - form renders from the vocabulary;
  - author a Blue move (e.g. a `monitor`-like SetFlag move with a distinct name), Save → success;
  - the move appears in the list as authored with a Delete button;
  - `~/.purple-range/tools/<id>.ron` exists on disk;
  - open `/`, start a **new** match, and the new move appears on the Blue menu and is playable;
  - stop and restart the server; the move is still listed (persistence);
  - a deliberately bad move (e.g. produces a fact nothing sets) shows a plain-language error and is NOT saved;
  - the Build link on the main dashboard works.
  Record what was exercised.

- [ ] **Step 5: Commit.**

```bash
git add public/build.html public/wargame.html
git commit -m "feat(wargame): the /build node-builder form page (author → validate → save → play)"
```

---

## Self-review notes (for the implementer)

- **Never touch `default_registry()`** — the built-in guards (golden equivalence, taxonomy `reg.len()==16`, balance 3/10) depend on it being exactly the 16 embedded moves. Authored moves flow only through `registry_with_authored`.
- **The DTO `gate` tag is `kind`** with values `"Fact"`, `"Probe"`, `"AnyOf"` (serde `#[serde(tag = "kind")]` on `GateRow`). The front-end must emit exactly those.
- **Path safety is load-bearing:** `slug` is the only thing that turns user input into a filename; `save`/`delete` build paths only from a slugged id. Do not add any code path that writes/deletes using a raw id.
- **Fail-soft vs fail-loud:** `registry_with_authored` skips bad authored files with an `eprintln` warning; `default_registry` still panics on a bad built-in. Keep that asymmetry.
- The `game` autonomous SSE and `run_cli` stay on built-ins by design (the AI demo and the balance measurement must not see experimental moves); only `new_match` (human play) and `catalog` include authored moves.
