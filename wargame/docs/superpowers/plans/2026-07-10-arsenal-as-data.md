# Moves as data (arsenal-as-data) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert all 16 attack/defense moves from Rust code into data files the engine reads and plays, proving each converted move behaves identically to the old code before deleting it.

**Architecture:** A move becomes a `ToolDef` (a small data record: identity, preconditions, facts-left-behind, and a dependency-ordered list of steps). Each step fires one `Effect` from a fixed vocabulary (the "write half" of the alphabet sub-project 1 started). A `DataTool` wraps a validated `ToolDef` and implements the existing `Card` trait, so the registry, referee, and fog-of-war views are untouched. Moves live as `.ron` text files baked into the binary at build time.

**Tech Stack:** Rust, serde, serde_json, `ron` (RON text format reader), the existing `axum`/`tokio` server.

## Global Constraints

- Crate: `purple-wargame` at `~/Developer/production/purple-range/wargame`. Run tests with `cargo test`.
- **Foundation = facts-as-data v2 (already in the tree, branch `arsenal-as-data` @ `cf2b838`).** v2 widened `Category` to the full ATT&CK tactics + D3FEND defensive lanes (`ENFORCED` is now 9 categories), added two-level detection probes (`InstanceProbe::SawCategory(Category)`, `InstanceProbe::Identified(Technique)`) and `Requirement::AnyOf(Vec<Requirement>)`, and retired the three hardcoded detection facts. Adopt it — do NOT rebuild it. Read `src/category.rs`, `src/facts.rs`, `src/cards.rs` before starting; the RON gates below are copied from the v2 `cards.rs`.
- Sim only — no live range / RB3011 in this sub-project. All tests use `SimEnvironment`.
- The surfaced fact list `Fact::ALL` (**11 facts**) and `blue_detection_rows` must stay byte-identical — do not add any variant to `Fact`. New legality atoms go on `InstanceProbe` (never surfaced).
- `Requirement` is **not `Copy`** (it now holds `AnyOf(Vec<Requirement>)`) — clone it where needed; the `.ron` gates may nest `AnyOf`.
- Every move keeps its existing `id` string and `technique()` — the referee keys blue scoring and flavor text on the `id`, and records `performed` from `technique()`.
- The move engine calls `env.act(node_id, …)` with the STEP's id. Single-step moves MUST name their step the same as the move id (so `SimEnvironment` and the future `LiveEnvironment` dispatch unchanged). Kerberoast's three steps are named `enum_spns`, `request_tgs`, `crack_hash` (the existing live-dispatch ids).
- Balance baseline to preserve: BLUE wins 3 of 10 on seeds {1,3,7} measured with the **deterministic heuristic** (`cli`, no model). Single-run model batches (`qwen2.5:7b`) are non-deterministic noise — do NOT use them to judge balance.
- Reviewer note (from sub-project 1): use **Opus** for any review subagent that reads the attack-card code — Sonnet trips a content filter on kerberoast/DCSync.
- Commit after every task. Work on a branch `arsenal-as-data` cut from `main`.

---

## File structure

- **Create** `src/effects.rs` — the effect vocabulary (`StateFlag`, `Effect`, `EffectResult`, `apply`).
- **Create** `src/tool.rs` — the move-file types (`Guard`, `Node`, `ToolDef`), `DataTool` (implements `Card`), and the interpreter (`play`).
- **Create** `src/arsenal.rs` — RON loader, validator (`validate`, `validate_set`), the embedded file list, and `default_registry()` (moved here).
- **Create** `tools/*.ron` — the 16 move files.
- **Create** `tests/arsenal_equivalence.rs` — the migration proof (data-tool vs legacy, then vs committed goldens).
- **Modify** `src/facts.rs` — add `InstanceProbe::Vuln(Technique)`; add `Deserialize` to `Fact`, `InstanceProbe`, `Requirement`.
- **Modify** `src/card.rs` — change `Card::id`/`Card::describe` return type from `&'static str` to `&str` (so a runtime-loaded tool can return its own `String`).
- **Modify** `src/graph.rs` — keep `Context` + `resolve_order`; remove `Primitive` and `CompositeCard` (Task 8).
- **Modify** `src/lib.rs` — add the new modules; remove `cards` (Task 8).
- **Modify** `src/main.rs` — import `default_registry` from `arsenal` (Task 8).
- **Delete** `src/cards.rs` (Task 8).

---

## Task 1: Read-half extensions in facts.rs

**Files:**
- Modify: `src/facts.rs`

**Interfaces:**
- Produces: `InstanceProbe::Vuln(Technique)` (a probe true when `state.vuln(t)`); `Deserialize` on `Fact`, `InstanceProbe`, `Requirement`.

- [ ] **Step 1: Write the failing test** — add to the `tests` module at the bottom of `src/facts.rs`:

```rust
    #[test]
    fn vuln_probe_tracks_misconfigs() {
        let mut s = base();
        // base() has the default misconfigs (Kerberoast, AsRepRoast, LateralMove)
        assert!(InstanceProbe::Vuln(Technique::Kerberoast).holds(&s));
        s.misconfigs.clear();
        assert!(!InstanceProbe::Vuln(Technique::Kerberoast).holds(&s));
    }

    #[test]
    fn requirement_deserializes_from_ron() {
        let r: Requirement = ron::from_str("Category(fact: ReachesDc, want: true)").unwrap();
        assert_eq!(r, Requirement::have(Fact::ReachesDc));
        let p: Requirement = ron::from_str("Instance(probe: Vuln(Kerberoast), want: true)").unwrap();
        assert_eq!(p, Requirement::Instance { probe: InstanceProbe::Vuln(Technique::Kerberoast), want: true });
        // the v2 AnyOf disjunction must round-trip too (it is what segment's gate uses)
        let a: Requirement = ron::from_str(
            "AnyOf([Instance(probe: SawCategory(InitialAccess), want: true), Instance(probe: SawCategory(LateralMovement), want: true)])"
        ).unwrap();
        assert_eq!(a, Requirement::any_of(vec![
            Requirement::saw_category(crate::category::Category::InitialAccess),
            Requirement::saw_category(crate::category::Category::LateralMovement),
        ]));
    }
```

- [ ] **Step 2: Add the `ron` dependency** (needed by the test above and Task 4):

Run: `cargo add ron@0.8`
Expected: `ron` added under `[dependencies]` in `Cargo.toml`.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib facts::tests::vuln_probe_tracks_misconfigs facts::tests::requirement_deserializes_from_ron`
Expected: FAIL — `Vuln` variant does not exist / `Requirement` does not implement `Deserialize`.

- [ ] **Step 4: Add the `Vuln` probe.** In `src/facts.rs`, add the variant to `InstanceProbe` (after `Detected(Technique)`):

```rust
    /// The attack path for this technique is planted in this scenario (`state.vuln(t)`).
    Vuln(Technique),
```

And add its arm in `InstanceProbe::holds`:

```rust
            InstanceProbe::Vuln(t) => s.vuln(*t),
```

- [ ] **Step 5: Add `Deserialize` derives.** Add `Deserialize` to these three derive lines in `src/facts.rs`. Note `Requirement` is NOT `Copy` (it holds `AnyOf(Vec<Requirement>)`) — keep its existing derive set and only append `Deserialize`:

```rust
// on enum Fact (currently: Debug, Clone, Copy, PartialEq, Eq, Serialize)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
// on enum InstanceProbe (currently: Debug, Clone, Copy, PartialEq, Eq, Serialize)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
// on enum Requirement (currently: Debug, Clone, PartialEq, Eq, Serialize — NO Copy)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
```

(`Deserialize` is already imported: `use serde::{Deserialize, Serialize};`.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib facts::`
Expected: PASS (all facts tests, including the two new ones).

- [ ] **Step 7: Commit**

```bash
git add src/facts.rs Cargo.toml Cargo.lock
git commit -m "feat(wargame): add Vuln probe + Deserialize to the fact alphabet"
```

---

## Task 2: The effect vocabulary (effects.rs)

**Files:**
- Create: `src/effects.rs`
- Modify: `src/lib.rs` (add `pub mod effects;`)

**Interfaces:**
- Consumes: `GameState`, `Context` (from `graph.rs`), `Cred`, `Alert`, `Detection`, `grade_rule`, `Technique` (from `state.rs`).
- Produces:
  - `enum StateFlag { Monitoring, AutoResponse, PathSevered, AesEnforced, PreauthEnforced, DomainAdmin }`
  - `enum Effect { Attempt, Advance, GrantCred{principal:String,secret:Option<String>,via:Technique}, SetFlag(StateFlag), RevokeKnownCreds, HuntGap, DeployDetection, SeverForwardEdges, Produce{key:String,value:serde_json::Value} }`
  - `struct EffectResult { success: bool, narrative: Option<String> }`
  - `Effect::apply(&self, state: &mut GameState, ctx: &mut Context, params: &Value, env_success: bool, env_narrative: &str, ok_narrative: &str) -> EffectResult`

- [ ] **Step 1: Add the module to lib.rs.** Read `src/lib.rs`, then add near the other `pub mod` lines:

```rust
pub mod effects;
```

- [ ] **Step 2: Write the failing test file.** Create `src/effects.rs` with ONLY the tests first (so it fails to compile → fails):

```rust
//! The write-half of the alphabet: the fixed set of state changes a move step can make.
//! Each `Effect` reproduces exactly one old `play()` body, so converting a move to data
//! cannot change behavior.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::graph::Context;
use crate::state::{grade_rule, Alert, Cred, Detection, GameState, Technique};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Cred, GameState, Host};

    fn base() -> GameState {
        GameState::new(vec![Host {
            id: "edge".into(), zone: "internet".into(), label: "edge".into(),
            foothold: false, reachable_by_red: true,
        }])
    }
    fn ctx() -> Context { Context::new() }

    #[test]
    fn set_flag_flips_the_right_boolean() {
        let mut s = base();
        let r = Effect::SetFlag(StateFlag::PathSevered).apply(&mut s, &mut ctx(), &Value::Null, true, "", "sev");
        assert!(r.success);
        assert!(s.acl_path_fixed);
    }

    #[test]
    fn set_flag_does_nothing_when_env_fails() {
        let mut s = base();
        let r = Effect::SetFlag(StateFlag::Monitoring).apply(&mut s, &mut ctx(), &Value::Null, false, "", "on");
        assert!(!r.success);
        assert!(!s.monitoring);
    }

    #[test]
    fn advance_takes_the_next_hop_and_templates_dest() {
        let mut s = base();
        s.edges.push(("internet".into(), "vlan20".into()));
        let r = Effect::Advance.apply(&mut s, &mut ctx(), &Value::Null, true, "", "pivoted into {dest} — closer");
        assert!(s.holds("vlan20"));
        assert_eq!(r.narrative.as_deref(), Some("pivoted into vlan20 — closer"));
    }

    #[test]
    fn grant_cred_pushes_a_cracked_credential() {
        let mut s = base();
        let e = Effect::GrantCred { principal: "range.local\\svc".into(), secret: Some("pw".into()), via: Technique::Kerberoast };
        let r = e.apply(&mut s, &mut ctx(), &Value::Null, true, "", "cracked");
        assert!(r.success);
        assert_eq!(s.creds.len(), 1);
        assert!(s.creds[0].cracked);
        assert_eq!(s.creds[0].via, Technique::Kerberoast);
    }

    #[test]
    fn revoke_known_creds_only_cancels_detected_ones() {
        let mut s = base();
        s.creds.push(Cred { principal: "seen".into(), secret: None, cracked: true, via: Technique::Kerberoast });
        s.creds.push(Cred { principal: "unseen".into(), secret: None, cracked: true, via: Technique::AsRepRoast });
        s.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "m".into(), rule_id: "r".into(), level: 8 });
        let r = Effect::RevokeKnownCreds.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(r.success);
        assert!(!s.creds[0].cracked, "detected cred cancelled");
        assert!(s.creds[1].cracked, "undetected cred untouched");
    }

    #[test]
    fn revoke_known_creds_reports_failure_when_none_match() {
        let mut s = base();
        let r = Effect::RevokeKnownCreds.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(!r.success);
    }

    #[test]
    fn hunt_surfaces_the_highest_value_undetected_technique() {
        let mut s = base();
        s.performed.push(Technique::Recon);
        s.performed.push(Technique::Kerberoast); // higher value
        let r = Effect::HuntGap.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(r.success);
        assert!(s.blue_knows(Technique::Kerberoast));
        assert!(!s.blue_knows(Technique::Recon), "only the top target this turn");
    }

    #[test]
    fn deploy_detection_writes_a_graded_rule_from_params() {
        let mut s = base();
        let p = serde_json::json!({ "technique": "kerberoast" });
        let r = Effect::DeployDetection.apply(&mut s, &mut ctx(), &p, true, "", "");
        assert!(r.success);
        assert!(s.has_detection(Technique::Kerberoast));
    }

    #[test]
    fn sever_forward_edges_removes_red_frontier() {
        let mut s = base();
        s.add_zone("vlan20");
        s.edges.push(("vlan20".into(), "vlan30".into()));
        let r = Effect::SeverForwardEdges.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(r.success);
        assert!(s.next_hops().is_empty(), "forward edge cut");
    }

    #[test]
    fn produce_writes_to_the_blackboard() {
        let mut s = base();
        let mut c = ctx();
        let e = Effect::Produce { key: "tgs_hash".into(), value: serde_json::json!("$krb5tgs$") };
        e.apply(&mut s, &mut c, &Value::Null, true, "", "");
        assert_eq!(c.get("tgs_hash"), Some(&serde_json::json!("$krb5tgs$")));
    }

    #[test]
    fn attempt_changes_no_state() {
        let mut s = base();
        let before = serde_json::to_value(&s).unwrap();
        let r = Effect::Attempt.apply(&mut s, &mut ctx(), &Value::Null, true, "", "did it");
        assert!(r.success);
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib effects::`
Expected: FAIL — `StateFlag`, `Effect`, `EffectResult` not defined.

- [ ] **Step 4: Write the implementation.** Insert above the `#[cfg(test)]` block in `src/effects.rs`:

```rust
/// The six on/off switches the game already tracks. Each maps to one boolean on `GameState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateFlag {
    Monitoring,
    AutoResponse,
    PathSevered,
    AesEnforced,
    PreauthEnforced,
    DomainAdmin,
}

impl StateFlag {
    fn set(&self, s: &mut GameState) {
        match self {
            StateFlag::Monitoring => s.monitoring = true,
            StateFlag::AutoResponse => s.auto_response = true,
            StateFlag::PathSevered => s.acl_path_fixed = true,
            StateFlag::AesEnforced => s.rc4_disabled = true,
            StateFlag::PreauthEnforced => s.preauth_required = true,
            StateFlag::DomainAdmin => s.red_reached_da = true,
        }
    }
}

/// The one thing a move step does. Every variant reproduces exactly one old `play()` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// Perform the technique but change no state (recon, bloodhound). The fact left behind
    /// (scouted / path mapped) comes from the referee recording `technique()`, not from here.
    Attempt,
    /// Take the next forward hop. `ok_narrative` may contain `{dest}` (replaced with the zone).
    Advance,
    /// Push a cracked credential.
    GrantCred { principal: String, secret: Option<String>, via: Technique },
    /// Flip one on/off switch true.
    SetFlag(StateFlag),
    /// Cancel any cracked credentials whose acquiring technique the defender has detected.
    RevokeKnownCreds,
    /// Surface the highest-value performed-but-undetected technique (records an alert + detection).
    HuntGap,
    /// Write a graded detection rule for `params.technique`.
    DeployDetection,
    /// Cut every edge out of a zone red holds into one it doesn't.
    SeverForwardEdges,
    /// Write a value to the blackboard (kerberoast's early steps pass values along).
    Produce { key: String, value: Value },
}

/// What applying an effect reports back: did it succeed, and (when the message is computed
/// from state) the narrative to show. `None` narrative means "let the step's default text stand".
pub struct EffectResult {
    pub success: bool,
    pub narrative: Option<String>,
}

impl Effect {
    /// Apply the effect. `env_success` is the environment's result (always true in sim);
    /// `env_narrative` is the environment's message (empty in sim); `ok_narrative` is the
    /// step's default success text. The narrative precedence reproduces the old `realize()`
    /// helper: an environment message wins, else the effect's own / the step's default text.
    pub fn apply(
        &self,
        state: &mut GameState,
        ctx: &mut Context,
        params: &Value,
        env_success: bool,
        env_narrative: &str,
        ok_narrative: &str,
    ) -> EffectResult {
        match self {
            Effect::Attempt => EffectResult { success: env_success, narrative: None },

            Effect::Advance => {
                let dest = state.next_hops().into_iter().next();
                if env_success {
                    if let Some(z) = &dest {
                        state.add_zone(z);
                    }
                }
                let d = dest.unwrap_or_default();
                let fallback = ok_narrative.replace("{dest}", &d);
                let narrative = if env_narrative.trim().is_empty() { fallback } else { env_narrative.to_string() };
                EffectResult { success: env_success, narrative: Some(narrative) }
            }

            Effect::GrantCred { principal, secret, via } => {
                if env_success {
                    state.creds.push(Cred { principal: principal.clone(), secret: secret.clone(), cracked: true, via: *via });
                }
                EffectResult { success: env_success, narrative: None }
            }

            Effect::SetFlag(f) => {
                if env_success {
                    f.set(state);
                }
                EffectResult { success: env_success, narrative: None }
            }

            Effect::RevokeKnownCreds => {
                let mut rotated = vec![];
                for i in 0..state.creds.len() {
                    if state.creds[i].cracked && state.blue_knows(state.creds[i].via) {
                        state.creds[i].cracked = false;
                        rotated.push(state.creds[i].principal.clone());
                    }
                }
                let narrative = if env_narrative.trim().is_empty() {
                    format!("rotated {} — those tickets are dead", rotated.join(", "))
                } else {
                    format!("{} ({})", env_narrative, rotated.join(", "))
                };
                EffectResult { success: !rotated.is_empty(), narrative: Some(narrative) }
            }

            Effect::HuntGap => {
                let target = state.performed.iter().copied().filter(|t| !state.blue_knows(*t)).max_by_key(|t| t.value());
                if let Some(t) = target {
                    state.alerts.push(Alert { round: state.round, technique: t, source: "hunt".into(), rule_id: "velociraptor-hunt".into(), level: 8 });
                    state.mark_detected(t, "hunt");
                    let dynamic = format!("threat hunt surfaced {} — gap closed", t.as_key());
                    let narrative = if env_narrative.trim().is_empty() { dynamic } else { env_narrative.to_string() };
                    EffectResult { success: true, narrative: Some(narrative) }
                } else {
                    EffectResult { success: false, narrative: Some("threat hunt — nothing new".into()) }
                }
            }

            Effect::DeployDetection => {
                let key = params.get("technique").and_then(|v| v.as_str()).unwrap_or("");
                match Technique::from_key(key) {
                    Some(t) => {
                        let fidelity = grade_rule(state.seed, t, state.round).to_string();
                        state.detections.push(Detection { id: format!("rule-{}", t.as_key()), technique: t, deployed_round: state.round, technique_based: true, fidelity });
                        let dynamic = format!("deployed detection for {}", t.as_key());
                        let narrative = if env_narrative.trim().is_empty() { dynamic } else { env_narrative.to_string() };
                        EffectResult { success: true, narrative: Some(narrative) }
                    }
                    None => EffectResult { success: false, narrative: Some("nothing observed to detect".into()) },
                }
            }

            Effect::SeverForwardEdges => {
                let cut = state.next_hops();
                let dynamic = format!("re-segmented — dropped red's path into {}", cut.join(", "));
                let narrative = if env_narrative.trim().is_empty() { dynamic } else { env_narrative.to_string() };
                if env_success {
                    let held = state.red_zones.clone();
                    state.edges.retain(|(f, t)| !(held.iter().any(|z| z == f) && !held.iter().any(|z| z == t)));
                }
                EffectResult { success: env_success, narrative: Some(narrative) }
            }

            Effect::Produce { key, value } => {
                ctx.set(key, value.clone());
                EffectResult { success: env_success, narrative: None }
            }
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib effects::`
Expected: PASS (all effect tests).

- [ ] **Step 6: Commit**

```bash
git add src/effects.rs src/lib.rs
git commit -m "feat(wargame): add the write-half effect vocabulary (effects.rs)"
```

---

## Task 3: The move-file types + interpreter (tool.rs)

**Files:**
- Modify: `src/card.rs` (relax `id`/`describe` return type)
- Create: `src/tool.rs`
- Modify: `src/lib.rs` (add `pub mod tool;`)

**Interfaces:**
- Consumes: `Card`, `Outcome`, `Environment` (card.rs); `Context`, `resolve_order` (graph.rs); `Effect`, `EffectResult` (effects.rs); `Requirement`, `Fact` (facts.rs); `Category`; `Side`, `Technique`.
- Produces:
  - `struct Guard { req: Requirement, else_narrative: String, else_surface: Vec<Technique> }`
  - `struct Node { id: String, requires: Vec<String>, produces_keys: Vec<String>, guards: Vec<Guard>, effect: Effect, ok_surface: Vec<Technique>, ok_narrative: String }`
  - `struct ToolDef { id, side, technique, category, summary, gate: Vec<Requirement>, produces: Vec<Fact>, params_schema: Option<Value>, nodes: Vec<Node> }` (all `pub`, derives `Debug, Clone, Deserialize`)
  - `struct DataTool { def: ToolDef }` with `DataTool::new(def) -> Self`, implementing `Card`.

- [ ] **Step 1: Relax the `Card` trait return types.** In `src/card.rs`, change the two signatures in `trait Card` from `&'static str` to `&str`:

```rust
    fn id(&self) -> &str;
    // ...
    fn describe(&self) -> &str;
```

(The 16 legacy impls return string literals, which are `&'static str` and still satisfy `&str` — they compile unchanged. `resolve_order`/`registry`/`flavor` all take `&str`.)

- [ ] **Step 2: Add the module to lib.rs.** In `src/lib.rs` add:

```rust
pub mod tool;
```

- [ ] **Step 3: Write the failing test.** Create `src/tool.rs` with the test module first:

```rust
//! A move as data: a `ToolDef` (identity + preconditions + facts-left-behind + steps) wrapped
//! by `DataTool`, which implements the existing `Card` trait. The interpreter runs the steps in
//! dependency order, exactly reproducing the old hand-written `play()` bodies.

use serde::Deserialize;
use serde_json::Value;

use crate::card::{Card, Environment, Outcome};
use crate::category::Category;
use crate::effects::Effect;
use crate::facts::{Fact, Requirement};
use crate::graph::{resolve_order_keys, Context};
use crate::state::{GameState, Side, Technique};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SimEnvironment;
    use crate::state::{GameState, Host};

    fn base() -> GameState {
        GameState::new(vec![Host {
            id: "edge".into(), zone: "internet".into(), label: "edge".into(),
            foothold: false, reachable_by_red: true,
        }])
    }

    fn one_node(id: &str, effect: Effect, ok_narrative: &str, ok_surface: Vec<Technique>) -> Node {
        Node { id: id.into(), requires: vec![], produces_keys: vec![], guards: vec![], effect, ok_surface, ok_narrative: ok_narrative.into() }
    }

    #[test]
    fn single_node_tool_plays_its_effect_and_narrative() {
        let def = ToolDef {
            id: "monitor".into(), side: Side::Blue, technique: Technique::Recon,
            category: Category::Detection, summary: "watch".into(),
            gate: vec![Requirement::lack(Fact::Monitoring)], produces: vec![Fact::Monitoring],
            params_schema: None,
            nodes: vec![one_node("monitor", Effect::SetFlag(crate::effects::StateFlag::Monitoring), "monitoring ONLINE", vec![])],
        };
        let tool = DataTool::new(def);
        let mut s = base();
        let mut env = SimEnvironment::new();
        let o = tool.play(&mut s, &Value::Null, &mut env);
        assert!(o.success);
        assert!(s.monitoring);
        assert_eq!(o.narrative, "monitoring ONLINE");
    }

    #[test]
    fn multi_node_tool_runs_in_dependency_order_with_a_composite_narrative() {
        use crate::effects::StateFlag;
        let def = ToolDef {
            id: "chain".into(), side: Side::Red, technique: Technique::Kerberoast,
            category: Category::CredentialAccess, summary: "chain".into(),
            gate: vec![], produces: vec![], params_schema: None,
            nodes: vec![
                Node { id: "second".into(), requires: vec!["k".into()], produces_keys: vec![], guards: vec![],
                       effect: Effect::SetFlag(StateFlag::DomainAdmin), ok_surface: vec![Technique::LateralMove], ok_narrative: "b".into() },
                Node { id: "first".into(), requires: vec![], produces_keys: vec!["k".into()], guards: vec![],
                       effect: Effect::Produce { key: "k".into(), value: Value::Bool(true) }, ok_surface: vec![Technique::Recon], ok_narrative: "a".into() },
            ],
        };
        let tool = DataTool::new(def);
        let mut s = base();
        let mut env = SimEnvironment::new();
        let o = tool.play(&mut s, &Value::Null, &mut env);
        assert!(o.success);
        assert!(s.red_reached_da, "dependent node ran after its producer");
        assert_eq!(o.narrative, "[chain] dependency order: first[ok] -> second[ok]");
        assert_eq!(o.detection_surface, vec![Technique::Recon, Technique::LateralMove]);
    }

    #[test]
    fn guard_failure_stops_the_move_with_its_message_and_surface() {
        let def = ToolDef {
            id: "asrep_roast".into(), side: Side::Red, technique: Technique::AsRepRoast,
            category: Category::CredentialAccess, summary: "roast".into(),
            gate: vec![], produces: vec![], params_schema: None,
            nodes: vec![Node {
                id: "asrep_roast".into(), requires: vec![], produces_keys: vec![],
                guards: vec![Guard { req: Requirement::lack(Fact::PreauthEnforced), else_narrative: "AS-REP blocked — pre-auth enforced".into(), else_surface: vec![Technique::AsRepRoast] }],
                effect: Effect::Attempt, ok_surface: vec![Technique::AsRepRoast], ok_narrative: "cracked".into(),
            }],
        };
        let tool = DataTool::new(def);
        let mut s = base();
        s.preauth_required = true;
        let mut env = SimEnvironment::new();
        let o = tool.play(&mut s, &Value::Null, &mut env);
        assert!(!o.success);
        assert_eq!(o.narrative, "AS-REP blocked — pre-auth enforced");
        assert_eq!(o.detection_surface, vec![Technique::AsRepRoast]);
    }
}
```

- [ ] **Step 4: Add the dependency-ordering helper to graph.rs.** `resolve_order` currently takes `&[Box<dyn Primitive>]`. Add a key-based variant that works on the data nodes. In `src/graph.rs`, add:

```rust
/// Topologically order items described only by their (requires, produces) blackboard keys.
/// Returns indices in a legal execution order, or an error on a cycle / missing input.
pub fn resolve_order_keys(reqs: &[Vec<String>], prods: &[Vec<String>], initial: &HashSet<String>) -> Result<Vec<usize>, String> {
    let n = reqs.len();
    let mut scheduled = vec![false; n];
    let mut available = initial.clone();
    let mut order = Vec::new();
    loop {
        let mut progressed = false;
        for i in 0..n {
            if scheduled[i] {
                continue;
            }
            if reqs[i].iter().all(|r| available.contains(r)) {
                scheduled[i] = true;
                for p in &prods[i] {
                    available.insert(p.clone());
                }
                order.push(i);
                progressed = true;
            }
        }
        if order.len() == n {
            break;
        }
        if !progressed {
            return Err("unsatisfiable dependency graph (cycle or missing input)".into());
        }
    }
    Ok(order)
}
```

- [ ] **Step 5: Write the interpreter.** Insert above the `#[cfg(test)]` block in `src/tool.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Guard {
    pub req: Requirement,
    pub else_narrative: String,
    #[serde(default)]
    pub else_surface: Vec<Technique>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub produces_keys: Vec<String>,
    #[serde(default)]
    pub guards: Vec<Guard>,
    pub effect: Effect,
    #[serde(default)]
    pub ok_surface: Vec<Technique>,
    pub ok_narrative: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolDef {
    pub id: String,
    pub side: Side,
    pub technique: Technique,
    pub category: Category,
    pub summary: String,
    #[serde(default)]
    pub gate: Vec<Requirement>,
    #[serde(default)]
    pub produces: Vec<Fact>,
    #[serde(default)]
    pub params_schema: Option<Value>,
    pub nodes: Vec<Node>,
}

/// A move loaded from data. Implements `Card`, so it lives in the same registry and plays
/// exactly like the old hand-written cards.
pub struct DataTool {
    def: ToolDef,
}

impl DataTool {
    pub fn new(def: ToolDef) -> Self {
        Self { def }
    }
    pub fn def(&self) -> &ToolDef {
        &self.def
    }
}

impl Card for DataTool {
    fn id(&self) -> &str {
        &self.def.id
    }
    fn side(&self) -> Side {
        self.def.side
    }
    fn technique(&self) -> Technique {
        self.def.technique
    }
    fn describe(&self) -> &str {
        &self.def.summary
    }
    fn category(&self) -> Category {
        self.def.category
    }
    fn requires(&self) -> Vec<Requirement> {
        self.def.gate.clone()
    }
    fn produces(&self) -> Vec<Fact> {
        self.def.produces.clone()
    }
    fn detection_surface(&self) -> Vec<Technique> {
        let mut out = Vec::new();
        for n in &self.def.nodes {
            for t in &n.ok_surface {
                if !out.contains(t) {
                    out.push(*t);
                }
            }
        }
        out
    }
    fn params_schema(&self) -> Value {
        self.def.params_schema.clone().unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} }))
    }
    fn default_params(&self, state: &GameState) -> Value {
        // Only the "write a detection rule" move has params: default to the highest-value
        // observed-but-unruled technique (reproduces the old deploy_detection default).
        if self.def.nodes.iter().any(|n| matches!(n.effect, Effect::DeployDetection)) {
            let t = state.alerts.iter().map(|a| a.technique).filter(|t| !state.has_detection(*t)).max_by_key(|t| t.value());
            return match t {
                Some(t) => serde_json::json!({ "technique": t.as_key() }),
                None => serde_json::json!({}),
            };
        }
        serde_json::json!({})
    }
    fn play(&self, state: &mut GameState, params: &Value, env: &mut dyn Environment) -> Outcome {
        let multi = self.def.nodes.len() > 1;
        let reqs: Vec<Vec<String>> = self.def.nodes.iter().map(|n| n.requires.clone()).collect();
        let prods: Vec<Vec<String>> = self.def.nodes.iter().map(|n| n.produces_keys.clone()).collect();
        let mut ctx = Context::new();
        let order = match resolve_order_keys(&reqs, &prods, &ctx.keys()) {
            Ok(o) => o,
            Err(e) => return Outcome { success: false, narrative: format!("[{}] {}", self.def.id, e), detection_surface: vec![] },
        };

        let mut surface: Vec<Technique> = Vec::new();
        let mut steps: Vec<String> = Vec::new();
        let mut single_narrative = String::new();

        for i in order {
            let node = &self.def.nodes[i];

            // Guards: first failing guard ends the move (no environment call).
            if let Some(g) = node.guards.iter().find(|g| !g.req.satisfied(state)) {
                for t in &g.else_surface {
                    if !surface.contains(t) {
                        surface.push(*t);
                    }
                }
                let narrative = if multi {
                    steps.push(format!("{}[FAIL]", node.id));
                    format!("[{}] dependency order: {}", self.def.id, steps.join(" -> "))
                } else {
                    g.else_narrative.clone()
                };
                return Outcome { success: false, narrative, detection_surface: surface };
            }

            let env_out = env.act(&node.id, params, state);
            let er = node.effect.apply(state, &mut ctx, params, env_out.success, &env_out.narrative, &node.ok_narrative);

            for t in &node.ok_surface {
                if !surface.contains(t) {
                    surface.push(*t);
                }
            }
            steps.push(format!("{}[{}]", node.id, if er.success { "ok" } else { "FAIL" }));
            single_narrative = er.narrative.unwrap_or_else(|| {
                if env_out.narrative.trim().is_empty() { node.ok_narrative.clone() } else { env_out.narrative.clone() }
            });

            if !er.success {
                let narrative = if multi {
                    format!("[{}] dependency order: {}", self.def.id, steps.join(" -> "))
                } else {
                    single_narrative.clone()
                };
                return Outcome { success: false, narrative, detection_surface: surface };
            }
        }

        let narrative = if multi {
            format!("[{}] dependency order: {}", self.def.id, steps.join(" -> "))
        } else {
            single_narrative
        };
        Outcome { success: true, narrative, detection_surface: surface }
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib tool::`
Expected: PASS (all three interpreter tests).

- [ ] **Step 7: Verify the whole crate still builds and passes**

Run: `cargo test`
Expected: PASS (nothing else changed behavior; legacy cards still present).

- [ ] **Step 8: Commit**

```bash
git add src/card.rs src/graph.rs src/tool.rs src/lib.rs
git commit -m "feat(wargame): data-defined move (ToolDef) + interpreter (DataTool: Card)"
```

---

## Task 4: RON loader + validator (arsenal.rs)

**Files:**
- Create: `src/arsenal.rs`
- Modify: `src/lib.rs` (add `pub mod arsenal;`)

**Interfaces:**
- Consumes: `ToolDef`, `Node` (tool.rs); `resolve_order_keys` (graph.rs); `Category`; `Effect`, `StateFlag` (effects.rs); `Fact`, `Technique`.
- Produces:
  - `parse_tool(src: &str) -> Result<ToolDef, String>`
  - `validate(def: &ToolDef) -> Result<(), Vec<String>>`
  - `validate_set(defs: &[ToolDef]) -> Result<(), Vec<String>>`
  - `established_facts(def: &ToolDef) -> Vec<Fact>` (facts an effect flips OR the referee records from `technique`) — used by the produces lint.

- [ ] **Step 1: Add the module to lib.rs.** In `src/lib.rs` add:

```rust
pub mod arsenal;
```

- [ ] **Step 2: Write the failing tests.** Create `src/arsenal.rs` with the test module first:

```rust
//! Loads move files (RON text) into `ToolDef`s, checks them, and builds the registry.
//! The checker is the safety net for the future author-a-move front-ends, and its
//! structural checks are the first real consumers of the requires/produces data.

use std::collections::HashSet;

use crate::category::Category;
use crate::effects::{Effect, StateFlag};
use crate::facts::Fact;
use crate::graph::resolve_order_keys;
use crate::registry::CardRegistry;
use crate::tool::{DataTool, ToolDef};

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: &str = r#"ToolDef(
        id: "monitor", side: Blue, technique: Recon, category: Detection,
        summary: "watch", gate: [Category(fact: Monitoring, want: false)],
        produces: [Monitoring],
        nodes: [ Node(id: "monitor", effect: SetFlag(Monitoring), ok_narrative: "on") ],
    )"#;

    #[test]
    fn parses_a_valid_move_file() {
        let def = parse_tool(MONITOR).expect("should parse");
        assert_eq!(def.id, "monitor");
        assert_eq!(def.nodes.len(), 1);
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn rejects_a_dangling_blackboard_read() {
        let src = r#"ToolDef(id: "x", side: Red, technique: Recon, category: Discovery,
            summary: "s", nodes: [ Node(id: "a", requires: ["missing"], effect: Attempt, ok_narrative: "n") ])"#;
        let def = parse_tool(src).unwrap();
        let errs = validate(&def).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("dependency") || e.contains("missing")), "got {errs:?}");
    }

    #[test]
    fn rejects_a_move_claiming_a_fact_it_never_establishes() {
        // Claims PathSevered but only performs a technique — nothing sets it.
        let src = r#"ToolDef(id: "x", side: Red, technique: Recon, category: Discovery,
            summary: "s", produces: [PathSevered],
            nodes: [ Node(id: "x", effect: Attempt, ok_narrative: "n") ])"#;
        let def = parse_tool(src).unwrap();
        let errs = validate(&def).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("produces") && e.contains("path_severed")), "got {errs:?}");
    }

    #[test]
    fn set_rejects_duplicate_ids() {
        let a = parse_tool(MONITOR).unwrap();
        let b = parse_tool(MONITOR).unwrap();
        let errs = validate_set(&[a, b]).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("duplicate")), "got {errs:?}");
    }

    #[test]
    fn set_rejects_missing_category_coverage() {
        let def = parse_tool(MONITOR).unwrap(); // only Detection
        let errs = validate_set(&[def]).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("category")), "got {errs:?}");
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib arsenal::`
Expected: FAIL — `parse_tool`, `validate`, `validate_set` not defined.

- [ ] **Step 4: Write the loader + validator.** Insert above the `#[cfg(test)]` block in `src/arsenal.rs`:

```rust
/// Parse one move file (RON text) into a `ToolDef`.
pub fn parse_tool(src: &str) -> Result<ToolDef, String> {
    ron::from_str::<ToolDef>(src).map_err(|e| format!("RON parse error: {e}"))
}

/// The facts a move actually establishes: those an effect flips true, plus the ones the referee
/// derives from recording the move's `technique` (recon -> scouted; bloodhound -> path mapped/scouted).
pub fn established_facts(def: &ToolDef) -> Vec<Fact> {
    let mut out = Vec::new();
    let mut add = |f: Fact, out: &mut Vec<Fact>| {
        if !out.contains(&f) {
            out.push(f);
        }
    };
    for n in &def.nodes {
        match &n.effect {
            Effect::Advance => add(Fact::Foothold, &mut out), // taking the first internal hop yields a foothold
            Effect::GrantCred { .. } => add(Fact::HasCred, &mut out),
            Effect::SetFlag(StateFlag::Monitoring) => add(Fact::Monitoring, &mut out),
            Effect::SetFlag(StateFlag::AutoResponse) => add(Fact::AutoResponse, &mut out),
            Effect::SetFlag(StateFlag::PathSevered) => add(Fact::PathSevered, &mut out),
            Effect::SetFlag(StateFlag::AesEnforced) => add(Fact::AesEnforced, &mut out),
            Effect::SetFlag(StateFlag::PreauthEnforced) => add(Fact::PreauthEnforced, &mut out),
            Effect::SetFlag(StateFlag::DomainAdmin) => add(Fact::DomainAdmin, &mut out),
            _ => {}
        }
    }
    // Facts the referee records from `technique()`:
    match def.technique {
        crate::state::Technique::Recon => add(Fact::Scouted, &mut out),
        crate::state::Technique::BloodHound => {
            add(Fact::PathMapped, &mut out);
            add(Fact::Scouted, &mut out);
        }
        _ => {}
    }
    out
}

/// Per-move checks: runnable steps, no dangling reads, and every produced fact is really established.
pub fn validate(def: &ToolDef) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();

    let reqs: Vec<Vec<String>> = def.nodes.iter().map(|n| n.requires.clone()).collect();
    let prods: Vec<Vec<String>> = def.nodes.iter().map(|n| n.produces_keys.clone()).collect();

    // (1) runnable steps — the dependency graph resolves from an empty blackboard
    if let Err(e) = resolve_order_keys(&reqs, &prods, &HashSet::new()) {
        errs.push(format!("[{}] unrunnable steps: {e}", def.id));
    }

    // (2) no dangling reads — every required key is produced by some step in this move
    let produced: HashSet<&String> = def.nodes.iter().flat_map(|n| n.produces_keys.iter()).collect();
    for n in &def.nodes {
        for r in &n.requires {
            if !produced.contains(r) {
                errs.push(format!("[{}] step '{}' reads '{}' which no step in the move produces", def.id, n.id, r));
            }
        }
    }

    // (3) leaves-behind — every declared produced fact is established by an effect or the technique
    let established = established_facts(def);
    for f in &def.produces {
        if !established.contains(f) {
            errs.push(format!("[{}] produces '{}' but no effect or technique establishes it", def.id, f.key()));
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Whole-set checks: unique ids, and at least one move in each enforced category.
pub fn validate_set(defs: &[ToolDef]) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();

    let mut seen: HashSet<&str> = HashSet::new();
    for d in defs {
        if !seen.insert(d.id.as_str()) {
            errs.push(format!("duplicate move id '{}'", d.id));
        }
    }
    for cat in Category::ENFORCED {
        if !defs.iter().any(|d| d.category == cat) {
            errs.push(format!("no move in required category '{}'", cat.key()));
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Parse, validate, and register a set of move-file sources into a fresh registry.
pub fn registry_from_sources(sources: &[&str]) -> Result<CardRegistry, Vec<String>> {
    let mut defs = Vec::new();
    let mut errs = Vec::new();
    for src in sources {
        match parse_tool(src) {
            Ok(def) => defs.push(def),
            Err(e) => errs.push(e),
        }
    }
    for def in &defs {
        if let Err(mut e) = validate(def) {
            errs.append(&mut e);
        }
    }
    if let Err(mut e) = validate_set(&defs) {
        errs.append(&mut e);
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    let mut reg = CardRegistry::new();
    for def in defs {
        reg.register(Box::new(DataTool::new(def)));
    }
    Ok(reg)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib arsenal::`
Expected: PASS (all five loader/validator tests).

- [ ] **Step 6: Commit**

```bash
git add src/arsenal.rs src/lib.rs
git commit -m "feat(wargame): RON move-file loader + validator (arsenal.rs)"
```

---

## Task 5: Equivalence harness + first move (monitor)

This task builds the migration proof and proves the first move. It compares a `DataTool` against the still-present legacy card, playing both through `SimEnvironment` and asserting identical `Outcome` and identical resulting `GameState`.

**Files:**
- Create: `tools/monitor.ron`
- Create: `tests/arsenal_equivalence.rs`

**Interfaces:**
- Consumes: `purple_wargame::cards::default_registry` (legacy), `purple_wargame::arsenal::parse_tool`, `purple_wargame::tool::DataTool`, `purple_wargame::env::SimEnvironment`, `purple_wargame::card::{Card, Move}`, `purple_wargame::state::*`.
- Produces: `fn matrix() -> Vec<(String, GameState, Value)>` and `fn assert_equivalent(id, def_src)` reused by Tasks 6–7.

- [ ] **Step 1: Author the first move file.** Create `tools/monitor.ron`:

```ron
ToolDef(
    id: "monitor", side: Blue, technique: Recon, category: Detection,
    summary: "Bring continuous monitoring online (Velociraptor + Sysmon live)",
    gate: [Category(fact: Monitoring, want: false)],
    produces: [Monitoring],
    nodes: [ Node(
        id: "monitor", effect: SetFlag(Monitoring), ok_surface: [],
        ok_narrative: "continuous monitoring ONLINE — the range is now watched",
    ) ],
)
```

- [ ] **Step 2: Write the harness + the failing monitor test.** Create `tests/arsenal_equivalence.rs`:

```rust
//! Migration proof: each data move plays byte-for-byte identically to the legacy card it
//! replaces. We compare the `play()` outcome AND the resulting game state (the referee's
//! bookkeeping around play is unchanged, so play() is the exact unit under change).

use purple_wargame::card::{Card, Environment};
use purple_wargame::cards::default_registry;
use purple_wargame::env::SimEnvironment;
use purple_wargame::state::{Alert, Cred, Detection, GameState, Host, Technique};
use purple_wargame::tool::DataTool;
use serde_json::{json, Value};

/// A diverse set of states that, between them, exercise every effect and guard branch.
fn matrix() -> Vec<(String, GameState, Value)> {
    let mut out: Vec<(String, GameState, Value)> = Vec::new();

    let host = || Host { id: "edge".into(), zone: "internet".into(), label: "edge".into(), foothold: false, reachable_by_red: true };

    // s0: fresh
    out.push(("fresh".into(), GameState::new(vec![host()]), json!({})));

    // s1: foothold with a forward hop
    let mut s1 = GameState::new(vec![host()]);
    s1.add_zone("vlan20");
    s1.edges.push(("vlan20".into(), "vlan30".into()));
    out.push(("foothold+hop".into(), s1, json!({})));

    // s2: reaches DC (objective zone held), default misconfigs
    let mut s2 = GameState::new(vec![host()]);
    s2.add_zone("vlan30");
    out.push(("reaches_dc".into(), s2, json!({})));

    // s3: reaches DC but AES enforced + preauth enforced
    let mut s3 = GameState::new(vec![host()]);
    s3.add_zone("vlan30");
    s3.rc4_disabled = true;
    s3.preauth_required = true;
    out.push(("reaches_dc+hardened".into(), s3, json!({})));

    // s4: reaches DC but no roastable misconfigs planted
    let mut s4 = GameState::new(vec![host()]);
    s4.add_zone("vlan30");
    s4.misconfigs.clear();
    out.push(("reaches_dc+no_vuln".into(), s4, json!({})));

    // s5: holds a cracked cred (kerberoast) that blue has detected; path mapped
    let mut s5 = GameState::new(vec![host()]);
    s5.add_zone("vlan30");
    s5.creds.push(Cred { principal: "range.local\\svc_mssql".into(), secret: Some("Summer2024!".into()), cracked: true, via: Technique::Kerberoast });
    s5.performed.push(Technique::BloodHound);
    s5.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "baseline".into(), rule_id: "r".into(), level: 8 });
    out.push(("cred+detected+mapped".into(), s5, json!({})));

    // s6: red performed undetected recon + an intrusion alert exists
    let mut s6 = GameState::new(vec![host()]);
    s6.add_zone("vlan20");
    s6.edges.push(("vlan20".into(), "vlan30".into()));
    s6.performed.push(Technique::Recon);
    s6.performed.push(Technique::Kerberoast);
    s6.alerts.push(Alert { round: 1, technique: Technique::Pivot, source: "baseline".into(), rule_id: "r".into(), level: 8 });
    out.push(("undetected_activity".into(), s6, json!({})));

    // s7: scouting detected + a DCSync path planted (for remediate/escalate branches)
    let mut s7 = GameState::new(vec![host()]);
    s7.add_zone("vlan30");
    s7.alerts.push(Alert { round: 1, technique: Technique::Recon, source: "hunt".into(), rule_id: "r".into(), level: 8 });
    s7.creds.push(Cred { principal: "range.local\\svc_mssql".into(), secret: Some("x".into()), cracked: true, via: Technique::Kerberoast });
    s7.performed.push(Technique::BloodHound);
    out.push(("scout_detected+path".into(), s7, json!({})));

    // s8: reaches DC + DEPLOYED rules for kerberoast & asrep (identified:* true → unlocks
    // enforce_aes / enforce_preauth in v2's two-level detection model)
    let mut s8 = GameState::new(vec![host()]);
    s8.add_zone("vlan30");
    s8.detections.push(Detection { id: "rk".into(), technique: Technique::Kerberoast, deployed_round: 1, technique_based: true, fidelity: "robust".into() });
    s8.detections.push(Detection { id: "ra".into(), technique: Technique::AsRepRoast, deployed_round: 1, technique_based: true, fidelity: "robust".into() });
    out.push(("identified_rules".into(), s8, json!({})));

    // s9: foothold + an initial-access alert + a forward hop (saw:initial_access true, not at DC →
    // segment's AnyOf branch is legal)
    let mut s9 = GameState::new(vec![host()]);
    s9.add_zone("vlan20");
    s9.edges.push(("vlan20".into(), "vlan30".into()));
    s9.alerts.push(Alert { round: 1, technique: Technique::InitialAccess, source: "baseline".into(), rule_id: "r".into(), level: 8 });
    out.push(("saw_initial_access+hop".into(), s9, json!({})));

    out
}

/// Play a card on a clone of `state` through a fresh sim env; return (outcome, resulting state)
/// as JSON for exact comparison.
fn run(card: &dyn Card, state: &GameState, params: &Value) -> (Value, Value) {
    let mut s = state.clone();
    let mut env = SimEnvironment::new();
    let o = card.play(&mut s, params, &mut env);
    (serde_json::to_value(&o).unwrap(), serde_json::to_value(&s).unwrap())
}

/// Assert the data move `def_src` plays identically to the legacy card with the same id,
/// across the whole matrix.
pub fn assert_equivalent(id: &str, def_src: &str) {
    let legacy_reg = default_registry();
    let legacy = legacy_reg.get(id).unwrap_or_else(|| panic!("no legacy card '{id}'"));
    let def = purple_wargame::arsenal::parse_tool(def_src).unwrap_or_else(|e| panic!("parse '{id}': {e}"));
    purple_wargame::arsenal::validate(&def).unwrap_or_else(|e| panic!("validate '{id}': {e:?}"));
    let data = DataTool::new(def);

    for (label, state, params) in matrix() {
        // legality equivalence — this is what v2 changed for the blue counters (the gate),
        // so the data-tool's requires() must produce the identical legal/illegal verdict.
        assert_eq!(legacy.precondition(&state), data.precondition(&state), "legality mismatch for '{id}' in state '{label}'");
        // play equivalence — the effect + resulting state must match byte-for-byte.
        let (lo, ls) = run(legacy, &state, &params);
        let (do_, ds) = run(&data, &state, &params);
        assert_eq!(lo, do_, "outcome mismatch for '{id}' in state '{label}'");
        assert_eq!(ls, ds, "state mismatch for '{id}' in state '{label}'");
    }
}

#[test]
fn monitor_is_equivalent() {
    assert_equivalent("monitor", include_str!("../tools/monitor.ron"));
}
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test --test arsenal_equivalence`
Expected: PASS — `monitor_is_equivalent`.

If it fails, the mismatch message names the state and whether outcome or state differed — fix the `.ron` (usually an off-by-one in `ok_narrative` or a wrong `gate`) until identical. Do NOT change the legacy card.

- [ ] **Step 4: Commit**

```bash
git add tools/monitor.ron tests/arsenal_equivalence.rs
git commit -m "test(wargame): arsenal equivalence harness + first move (monitor)"
```

---

## Task 6: Author + prove the RED moves

Author the seven red move files and prove each equivalent. Use the EXACT strings from `src/cards.rs` (copy `ok_narrative` text verbatim — the equivalence test compares narratives).

**Files:**
- Create: `tools/initial_access.ron`, `tools/pivot.ron`, `tools/recon.ron`, `tools/kerberoast.ron`, `tools/asrep_roast.ron`, `tools/bloodhound.ron`, `tools/escalate_da.ron`
- Modify: `tests/arsenal_equivalence.rs` (add one test per move)

**Interfaces:**
- Consumes: `assert_equivalent` (Task 5).

- [ ] **Step 1: Author the seven red files.**

`tools/initial_access.ron`:
```ron
ToolDef(
    id: "initial_access", side: Red, technique: InitialAccess, category: InitialAccess,
    summary: "Breach the perimeter (edge exploit / phish) for a first foothold",
    gate: [Category(fact: Foothold, want: false), Instance(probe: HasForwardPath, want: true)],
    produces: [Foothold],
    nodes: [ Node(
        id: "initial_access", effect: Advance, ok_surface: [InitialAccess],
        ok_narrative: "breached the perimeter — landed an internal foothold",
    ) ],
)
```

`tools/pivot.ron`:
```ron
ToolDef(
    id: "pivot", side: Red, technique: Pivot, category: LateralMovement,
    summary: "Pivot across internal segmentation toward the domain",
    gate: [Category(fact: Foothold, want: true), Category(fact: ReachesDc, want: false), Instance(probe: HasForwardPath, want: true)],
    produces: [],
    nodes: [ Node(
        id: "pivot", effect: Advance, ok_surface: [Pivot],
        ok_narrative: "pivoted into {dest} — one segment closer to the DC",
    ) ],
)
```

`tools/recon.ron`:
```ron
ToolDef(
    id: "recon", side: Red, technique: Recon, category: Discovery,
    summary: "Enumerate the AD estate",
    gate: [Category(fact: ReachesDc, want: true), Instance(probe: Performed(Recon), want: false)],
    produces: [Scouted],
    nodes: [ Node(
        id: "recon", effect: Attempt, ok_surface: [Recon],
        ok_narrative: "recon — mapped the domain",
    ) ],
)
```

`tools/kerberoast.ron`:
```ron
ToolDef(
    id: "kerberoast", side: Red, technique: Kerberoast, category: CredentialAccess,
    summary: "Kerberoast: enum SPNs -> request TGS -> crack (fails vs AES)",
    gate: [Category(fact: ReachesDc, want: true)],
    produces: [HasCred],
    nodes: [
        Node(id: "enum_spns", produces_keys: ["spn_targets"],
             effect: Produce(key: "spn_targets", value: ["MSSQLSvc/dc01.range.local"]),
             ok_surface: [Recon], ok_narrative: "found svc_mssql"),
        Node(id: "request_tgs", requires: ["spn_targets"], produces_keys: ["tgs_hash"],
             effect: Produce(key: "tgs_hash", value: "$krb5tgs$"),
             ok_surface: [Kerberoast], ok_narrative: "got TGS"),
        Node(id: "crack_hash", requires: ["tgs_hash"],
             guards: [
               Guard(req: Instance(probe: Vuln(Kerberoast), want: true), else_narrative: "no roastable SPN in this environment", else_surface: []),
               Guard(req: Category(fact: AesEnforced, want: false), else_narrative: "AES enforced — ticket uncrackable", else_surface: []),
             ],
             effect: GrantCred(principal: "range.local\\svc_mssql", secret: Some("Summer2024!"), via: Kerberoast),
             ok_surface: [], ok_narrative: "cracked: Summer2024!"),
    ],
)
```

`tools/asrep_roast.ron`:
```ron
ToolDef(
    id: "asrep_roast", side: Red, technique: AsRepRoast, category: CredentialAccess,
    summary: "AS-REP roast a no-preauth user (fails if pre-auth enforced)",
    gate: [Category(fact: ReachesDc, want: true)],
    produces: [HasCred],
    nodes: [ Node(
        id: "asrep_roast",
        guards: [
          Guard(req: Instance(probe: Vuln(AsRepRoast), want: true), else_narrative: "no AS-REP-roastable user in this environment", else_surface: []),
          Guard(req: Category(fact: PreauthEnforced, want: false), else_narrative: "AS-REP blocked — pre-auth enforced", else_surface: [AsRepRoast]),
        ],
        effect: GrantCred(principal: "range.local\\jbecker", secret: Some("Baseball2023"), via: AsRepRoast),
        ok_surface: [AsRepRoast], ok_narrative: "AS-REP roast — cracked jbecker",
    ) ],
)
```

`tools/bloodhound.ron`:
```ron
ToolDef(
    id: "bloodhound", side: Red, technique: BloodHound, category: Discovery,
    summary: "Collect the AD graph, find the path to DA",
    gate: [Category(fact: HasCred, want: true), Instance(probe: Performed(BloodHound), want: false)],
    produces: [PathMapped, Scouted],
    nodes: [ Node(
        id: "bloodhound", effect: Attempt, ok_surface: [BloodHound],
        ok_narrative: "BloodHound — svc_mssql holds DCSync => krbtgt => DA",
    ) ],
)
```

`tools/escalate_da.ron`:
```ron
ToolDef(
    id: "escalate_da", side: Red, technique: LateralMove, category: PrivilegeEscalation,
    summary: "Abuse the ACL path to Domain Admin (gone if remediated)",
    gate: [
      Instance(probe: LateralPathPlanted, want: true),
      Category(fact: HasCred, want: true),
      Category(fact: PathMapped, want: true),
      Category(fact: DomainAdmin, want: false),
      Category(fact: PathSevered, want: false),
    ],
    produces: [DomainAdmin],
    nodes: [ Node(
        id: "escalate_da", effect: SetFlag(DomainAdmin), ok_surface: [LateralMove],
        ok_narrative: "DCSync via svc_mssql -> dumped krbtgt -> DOMAIN ADMIN",
    ) ],
)
```

- [ ] **Step 2: Add the failing tests.** Append to `tests/arsenal_equivalence.rs`:

```rust
#[test]
fn initial_access_is_equivalent() { assert_equivalent("initial_access", include_str!("../tools/initial_access.ron")); }
#[test]
fn pivot_is_equivalent() { assert_equivalent("pivot", include_str!("../tools/pivot.ron")); }
#[test]
fn recon_is_equivalent() { assert_equivalent("recon", include_str!("../tools/recon.ron")); }
#[test]
fn kerberoast_is_equivalent() { assert_equivalent("kerberoast", include_str!("../tools/kerberoast.ron")); }
#[test]
fn asrep_roast_is_equivalent() { assert_equivalent("asrep_roast", include_str!("../tools/asrep_roast.ron")); }
#[test]
fn bloodhound_is_equivalent() { assert_equivalent("bloodhound", include_str!("../tools/bloodhound.ron")); }
#[test]
fn escalate_da_is_equivalent() { assert_equivalent("escalate_da", include_str!("../tools/escalate_da.ron")); }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --test arsenal_equivalence`
Expected: PASS for all red moves (plus monitor). On a mismatch, the message names the move + state; fix the `.ron` to match the legacy card exactly. Never edit the legacy card.

- [ ] **Step 4: Commit**

```bash
git add tools/initial_access.ron tools/pivot.ron tools/recon.ron tools/kerberoast.ron tools/asrep_roast.ron tools/bloodhound.ron tools/escalate_da.ron tests/arsenal_equivalence.rs
git commit -m "test(wargame): author + prove the 7 red moves as data"
```

---

## Task 7: Author + prove the BLUE moves

Author the remaining eight blue move files and prove each equivalent (monitor already done).

**Files:**
- Create: `tools/active_response.ron`, `tools/remediate_acl.ron`, `tools/enforce_aes.ron`, `tools/enforce_preauth.ron`, `tools/rotate_creds.ron`, `tools/hunt.ron`, `tools/deploy_detection.ron`, `tools/segment.ron`
- Modify: `tests/arsenal_equivalence.rs`

- [ ] **Step 1: Author the eight blue files.**

`tools/active_response.ron`:
```ron
ToolDef(
    id: "active_response", side: Blue, technique: Recon, category: Detection,
    summary: "Arm active response — detections auto-contain (the bite)",
    gate: [Category(fact: AutoResponse, want: false)],
    produces: [AutoResponse],
    nodes: [ Node(id: "active_response", effect: SetFlag(AutoResponse), ok_surface: [],
        ok_narrative: "active response ARMED — a detected theft is contained instantly") ],
)
```

`tools/remediate_acl.ron` (v2: D3FEND `Harden` lane; gated on any *seen* discovery activity):
```ron
ToolDef(
    id: "remediate_acl", side: Blue, technique: LateralMove, category: Harden,
    summary: "Remove the GenericAll->DA path / tier admins",
    gate: [Category(fact: PathSevered, want: false), Instance(probe: SawCategory(Discovery), want: true)],
    produces: [PathSevered],
    nodes: [ Node(id: "remediate_acl", effect: SetFlag(PathSevered), ok_surface: [],
        ok_narrative: "revoked svc_mssql DCSync on the domain — path to DA severed") ],
)
```

`tools/enforce_aes.ron` (v2: `Harden` lane; requires a DEPLOYED rule — `Identified`, not just an alert):
```ron
ToolDef(
    id: "enforce_aes", side: Blue, technique: Kerberoast, category: Harden,
    summary: "Disable RC4 / enforce AES — Kerberoast tickets uncrackable",
    gate: [Category(fact: AesEnforced, want: false), Instance(probe: Identified(Kerberoast), want: true)],
    produces: [AesEnforced],
    nodes: [ Node(id: "enforce_aes", effect: SetFlag(AesEnforced), ok_surface: [],
        ok_narrative: "RC4 disabled, AES enforced — roast tickets are now junk") ],
)
```

`tools/enforce_preauth.ron` (v2: `Harden` lane; requires a DEPLOYED rule — `Identified`):
```ron
ToolDef(
    id: "enforce_preauth", side: Blue, technique: AsRepRoast, category: Harden,
    summary: "Enforce Kerberos pre-auth — AS-REP roasting yields nothing",
    gate: [Category(fact: PreauthEnforced, want: false), Instance(probe: Identified(AsRepRoast), want: true)],
    produces: [PreauthEnforced],
    nodes: [ Node(id: "enforce_preauth", effect: SetFlag(PreauthEnforced), ok_surface: [],
        ok_narrative: "pre-auth enforced on jbecker — AS-REP dead") ],
)
```

`tools/rotate_creds.ron` (v2: `Evict` lane; produces nothing — cancelling a cred is a removal, not a fact flip):
```ron
ToolDef(
    id: "rotate_creds", side: Blue, technique: Kerberoast, category: Evict,
    summary: "Rotate credentials known to be compromised",
    gate: [Instance(probe: CredCompromiseKnown, want: true)],
    produces: [],
    nodes: [ Node(id: "rotate_creds", effect: RevokeKnownCreds, ok_surface: [], ok_narrative: "") ],
)
```

`tools/hunt.ron`:
```ron
ToolDef(
    id: "hunt", side: Blue, technique: Recon, category: Detection,
    summary: "Threat-hunt telemetry for an undetected technique (closes a gap)",
    gate: [Instance(probe: UndetectedActivity, want: true)],
    produces: [],
    nodes: [ Node(id: "hunt", effect: HuntGap, ok_surface: [], ok_narrative: "") ],
)
```

`tools/deploy_detection.ron`:
```ron
ToolDef(
    id: "deploy_detection", side: Blue, technique: Kerberoast, category: Detection,
    summary: "Write a technique-based detection for observed activity",
    gate: [Instance(probe: UndetectedAlert, want: true)],
    produces: [],
    params_schema: Some({ "type": "object", "properties": { "technique": { "type": "string" } }, "required": ["technique"] }),
    nodes: [ Node(id: "deploy_detection", effect: DeployDetection, ok_surface: [], ok_narrative: "") ],
)
```

`tools/segment.ron` (v2: `Isolate` lane; gated on any seen initial-access OR lateral-movement activity):
```ron
ToolDef(
    id: "segment", side: Blue, technique: Pivot, category: Isolate,
    summary: "Re-segment — firewall-drop red's frontier before it reaches the DC",
    gate: [
      AnyOf([
        Instance(probe: SawCategory(InitialAccess), want: true),
        Instance(probe: SawCategory(LateralMovement), want: true),
      ]),
      Category(fact: ReachesDc, want: false),
      Category(fact: DomainAdmin, want: false),
      Instance(probe: HasForwardPath, want: true),
    ],
    produces: [],
    nodes: [ Node(id: "segment", effect: SeverForwardEdges, ok_surface: [], ok_narrative: "") ],
)
```

- [ ] **Step 2: Add the failing tests, plus a deploy_detection param case.** Append to `tests/arsenal_equivalence.rs`:

```rust
#[test]
fn active_response_is_equivalent() { assert_equivalent("active_response", include_str!("../tools/active_response.ron")); }
#[test]
fn remediate_acl_is_equivalent() { assert_equivalent("remediate_acl", include_str!("../tools/remediate_acl.ron")); }
#[test]
fn enforce_aes_is_equivalent() { assert_equivalent("enforce_aes", include_str!("../tools/enforce_aes.ron")); }
#[test]
fn enforce_preauth_is_equivalent() { assert_equivalent("enforce_preauth", include_str!("../tools/enforce_preauth.ron")); }
#[test]
fn rotate_creds_is_equivalent() { assert_equivalent("rotate_creds", include_str!("../tools/rotate_creds.ron")); }
#[test]
fn hunt_is_equivalent() { assert_equivalent("hunt", include_str!("../tools/hunt.ron")); }
#[test]
fn segment_is_equivalent() { assert_equivalent("segment", include_str!("../tools/segment.ron")); }

// deploy_detection: prove equivalence with explicit params (valid, invalid) as well as defaults.
#[test]
fn deploy_detection_is_equivalent() {
    assert_equivalent("deploy_detection", include_str!("../tools/deploy_detection.ron"));
}
#[test]
fn deploy_detection_param_cases_match_legacy() {
    use purple_wargame::cards::default_registry;
    let src = include_str!("../tools/deploy_detection.ron");
    let def = purple_wargame::arsenal::parse_tool(src).unwrap();
    let data = DataTool::new(def);
    let legacy_reg = default_registry();
    let legacy = legacy_reg.get("deploy_detection").unwrap();
    for p in [json!({"technique": "kerberoast"}), json!({"technique": "not_a_technique"}), json!({})] {
        for (label, state, _) in matrix() {
            let (lo, ls) = run(legacy, &state, &p);
            let (do_, ds) = run(&data, &state, &p);
            assert_eq!(lo, do_, "deploy_detection outcome mismatch, params {p}, state '{label}'");
            assert_eq!(ls, ds, "deploy_detection state mismatch, params {p}, state '{label}'");
        }
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --test arsenal_equivalence`
Expected: PASS for all 16 moves. Fix any `.ron` mismatch to match the legacy card exactly.

- [ ] **Step 4: Verify the whole suite is green**

Run: `cargo test`
Expected: PASS (legacy + data both present, all equivalent).

- [ ] **Step 5: Commit**

```bash
git add tools/*.ron tests/arsenal_equivalence.rs
git commit -m "test(wargame): author + prove the 9 blue moves as data (all 16 equivalent)"
```

---

## Task 8: Freeze goldens, swap the registry, delete the old code

Now that all 16 are proven, capture the legacy outputs as committed golden fixtures (so the equivalence test stands without legacy code), switch the live registry to the data loader, and delete the Rust move code.

**Files:**
- Modify: `src/arsenal.rs` (add `TOOL_FILES` + `default_registry`)
- Modify: `src/main.rs` (import from `arsenal`)
- Modify: `src/lib.rs` (remove `pub mod cards;`)
- Modify: `src/graph.rs` (delete `Primitive` + `CompositeCard`)
- Delete: `src/cards.rs`
- Modify: `tests/arsenal_equivalence.rs` (add a golden generator + switch the standing assert to goldens)
- Create: `tests/fixtures/arsenal_goldens.json`

- [ ] **Step 1: Add the embedded file list + `default_registry` to arsenal.rs.** Append to `src/arsenal.rs` (above tests):

```rust
/// The move files, baked into the binary so it stays a single static executable.
pub const TOOL_FILES: &[&str] = &[
    include_str!("../tools/initial_access.ron"),
    include_str!("../tools/pivot.ron"),
    include_str!("../tools/recon.ron"),
    include_str!("../tools/kerberoast.ron"),
    include_str!("../tools/asrep_roast.ron"),
    include_str!("../tools/bloodhound.ron"),
    include_str!("../tools/escalate_da.ron"),
    include_str!("../tools/monitor.ron"),
    include_str!("../tools/active_response.ron"),
    include_str!("../tools/remediate_acl.ron"),
    include_str!("../tools/enforce_aes.ron"),
    include_str!("../tools/enforce_preauth.ron"),
    include_str!("../tools/rotate_creds.ron"),
    include_str!("../tools/hunt.ron"),
    include_str!("../tools/deploy_detection.ron"),
    include_str!("../tools/segment.ron"),
];

/// Build the game's card library from the embedded move files. Panics with the full list of
/// problems if any file is malformed — a broken arsenal must fail loudly at startup, not
/// silently drop a move.
pub fn default_registry() -> CardRegistry {
    match registry_from_sources(TOOL_FILES) {
        Ok(reg) => reg,
        Err(errs) => panic!("arsenal failed validation:\n  - {}", errs.join("\n  - ")),
    }
}
```

- [ ] **Step 2: Generate the golden fixtures.** Add this `#[ignore]`d generator to `tests/arsenal_equivalence.rs` (it reads the legacy cards while they still exist and writes the goldens):

```rust
// Run once, before deleting the legacy cards: `cargo test --test arsenal_equivalence bless_goldens -- --ignored`
#[test]
#[ignore]
fn bless_goldens() {
    use purple_wargame::cards::default_registry;
    let reg = default_registry();
    let ids = ["initial_access","pivot","recon","kerberoast","asrep_roast","bloodhound","escalate_da",
               "monitor","active_response","remediate_acl","enforce_aes","enforce_preauth","rotate_creds","hunt","deploy_detection","segment"];
    let mut golden = serde_json::Map::new();
    let param_cases = |id: &str| -> Vec<Value> {
        if id == "deploy_detection" { vec![json!({"technique":"kerberoast"}), json!({"technique":"not_a_technique"}), json!({})] } else { vec![json!({})] }
    };
    for id in ids {
        let card = reg.get(id).unwrap();
        for (label, state, _) in matrix() {
            for (pi, p) in param_cases(id).into_iter().enumerate() {
                let (o, s) = run(card, &state, &p);
                golden.insert(format!("{id}|{label}|{pi}"), json!({ "outcome": o, "state": s }));
            }
        }
    }
    std::fs::create_dir_all("tests/fixtures").unwrap();
    std::fs::write("tests/fixtures/arsenal_goldens.json", serde_json::to_string_pretty(&Value::Object(golden)).unwrap()).unwrap();
}
```

Run: `cargo test --test arsenal_equivalence bless_goldens -- --ignored`
Expected: PASS; `tests/fixtures/arsenal_goldens.json` written.

- [ ] **Step 3: Add the standing golden test** (does NOT depend on legacy). Append to `tests/arsenal_equivalence.rs`:

```rust
// Standing behavioral guard after the legacy code is gone: every data move still matches the
// frozen golden captured from the original hand-written cards.
#[test]
fn data_moves_match_frozen_goldens() {
    let golden: Value = serde_json::from_str(include_str!("fixtures/arsenal_goldens.json")).unwrap();
    let golden = golden.as_object().unwrap();
    let reg = purple_wargame::arsenal::default_registry();
    let param_cases = |id: &str| -> Vec<Value> {
        if id == "deploy_detection" { vec![json!({"technique":"kerberoast"}), json!({"technique":"not_a_technique"}), json!({})] } else { vec![json!({})] }
    };
    for (key, expected) in golden {
        let parts: Vec<&str> = key.split('|').collect();
        let (id, label, pi) = (parts[0], parts[1], parts[2].parse::<usize>().unwrap());
        let card = reg.get(id).unwrap_or_else(|| panic!("no move '{id}' in data registry"));
        let (_, state, _) = matrix().into_iter().find(|(l, _, _)| l == label).unwrap();
        let p = param_cases(id).into_iter().nth(pi).unwrap();
        let (o, s) = run(card, &state, &p);
        assert_eq!(&json!({ "outcome": o, "state": s }), expected, "move '{id}' drifted from golden in state '{label}' (params #{pi})");
    }
}
```

- [ ] **Step 4: Remove the legacy-dependent tests.** In `tests/arsenal_equivalence.rs`, delete `assert_equivalent`, `bless_goldens`, every `*_is_equivalent` test, and `deploy_detection_param_cases_match_legacy` (they reference `purple_wargame::cards`, which is being deleted). Keep `matrix`, `run`, and `data_moves_match_frozen_goldens`.

- [ ] **Step 5: Swap the live registry.** In `src/main.rs`, change:

```rust
use purple_wargame::cards::default_registry;
```
to:
```rust
use purple_wargame::arsenal::default_registry;
```

- [ ] **Step 6: Delete the legacy code.**

```bash
git rm src/cards.rs
```

In `src/lib.rs`, remove the line `pub mod cards;`.

In `src/graph.rs`, delete the `Primitive` trait, the `CompositeCard` struct + its `impl Card`, the old `resolve_order` (the `Box<dyn Primitive>` version) and its test, and now-unused imports (`Card`, `Environment`, `Outcome`, `Category`, `Fact`, `Requirement`, `Side` if unused). Keep `Context`, `PrimitiveResult`? — `PrimitiveResult` is only used by `Primitive`; delete it too. Keep `Context` and `resolve_order_keys` and the `HashSet`/`HashMap`/`Value` imports they need.

- [ ] **Step 7: Build and fix fallout.**

Run: `cargo build 2>&1 | head -40`
Expected: resolve any dangling references (e.g. a test in `main.rs:238` using `default_registry` — it now comes from `arsenal`; update that `use`/path too). Fix until clean.

- [ ] **Step 8: Run the full suite**

Run: `cargo test`
Expected: PASS — including `data_moves_match_frozen_goldens`, with `cards.rs` gone and the live game running off the data files.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor(wargame): load the arsenal from data files; delete the Rust move code"
```

---

## Task 9: Structural invariants over the data arsenal

Extend `tests/taxonomy.rs` so the loaded data arsenal is checked for the standing invariants (these now guard authored moves, including future model-authored ones).

**Files:**
- Modify: `tests/taxonomy.rs`

- [ ] **Step 1: Write the failing tests.** Append to `tests/taxonomy.rs`:

```rust
#[test]
fn data_arsenal_loads_and_passes_all_checks() {
    // default_registry panics if any file fails per-move or set validation.
    let reg = purple_wargame::arsenal::default_registry();
    assert_eq!(reg.len(), 16, "all 16 moves load");
}

#[test]
fn every_enforced_category_has_a_move() {
    use purple_wargame::category::Category;
    let defs: Vec<_> = purple_wargame::arsenal::TOOL_FILES.iter()
        .map(|s| purple_wargame::arsenal::parse_tool(s).unwrap())
        .collect();
    for cat in Category::ENFORCED {
        assert!(defs.iter().any(|d| d.category == cat), "no move in category {}", cat.key());
    }
}

#[test]
fn every_move_passes_per_move_validation() {
    for src in purple_wargame::arsenal::TOOL_FILES {
        let def = purple_wargame::arsenal::parse_tool(src).unwrap();
        purple_wargame::arsenal::validate(&def).unwrap_or_else(|e| panic!("validation failed: {e:?}"));
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --test taxonomy`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/taxonomy.rs
git commit -m "test(wargame): structural invariants over the data arsenal"
```

---

## Task 10: Balance verification (documented)

The win rate must still be 3/10 on seeds {1,3,7}. Measure with the **deterministic heuristic** (no model) — v2 established this is the reproducible signal; single-run model batches are non-deterministic noise.

**Files:**
- Create: `tests/balance_note.md` (record of the run)

- [ ] **Step 1: Build the binary**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 2: Run the 10-seed batch with the deterministic heuristic.** For each seed 1..10:

Run: `WARGAME_SEED=<n> ./target/debug/purple-wargame cli`
Look at the final line: `BLUE WINS — …` or `RED WINS — Domain Admin in N rounds`. (No `model` argument and no `WARGAME_MODEL_URL` — this is the deterministic heuristic, so each seed is fully reproducible.)

- [ ] **Step 3: Confirm the baseline.** Expected: BLUE wins on seeds 1, 3, 7 (exactly 3/10). Record each seed's winner in `tests/balance_note.md`. Because the run is deterministic, any deviation is a real behavior change — STOP and investigate (the equivalence + golden tests should have caught it first); do not accept a moved baseline as "fine".

- [ ] **Step 4: Commit**

```bash
git add tests/balance_note.md
git commit -m "test(wargame): record post-migration balance (3/10 on seeds 1,3,7)"
```

---

## Self-review notes (for the implementer)

- **Narrative fidelity:** the equivalence test compares the feed narrative exactly. Kerberoast's feed line is the composite `"[kerberoast] dependency order: enum_spns[ok] -> request_tgs[ok] -> crack_hash[ok]"` — that is intentional and unchanged from the legacy `CompositeCard`. Single-step moves use their own narrative.
- **Live-only cosmetic note:** the LIVE `rotate_creds` narrative formats as `"{env} (names)"`; the effect reproduces this. Sim (what the tests run) uses `"rotated names — those tickets are dead"`. Both are covered by the effect body.
- **Do not touch** the referee, the fact-surfacing, or `Fact::ALL`. If an equivalence test fails, the bug is in the `.ron` or the effect/interpreter, never in the legacy card.
- **After Task 8**, `cargo test` runs entirely off the data arsenal; the goldens are the standing behavioral guard and the balance run is the end-to-end check.
