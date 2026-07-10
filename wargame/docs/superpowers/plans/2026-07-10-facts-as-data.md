# Facts-as-Data Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace opaque `fn precondition` closures with declarative, introspectable card data (a two-tier `Requirement` alphabet + a `Category` tier), preserving today's legal menu and 3/10 balance exactly.

**Architecture:** Two orthogonal tiers. **Tier 1 `Category`** = the ATT&CK-tactic kill-chain backbone every card declares membership in (the topology). **Tier 2 `Requirement`** = a card's legality expressed as data over the fact alphabet: `Requirement::Category{fact,want}` gates on a *surfaced* `Fact` (the existing, unchanged 14-fact table); `Requirement::Instance{probe,want}` gates on a *ground-truth* `InstanceProbe` never shown to the model (this is where instance-specific and topology gates live). `Card::precondition` becomes a provided method evaluating `requires()`. Cards name capabilities, never tools, so growing the arsenal is composition, not new primitives.

**Tech Stack:** Rust (edition per `wargame/Cargo.toml`), `serde`, `cargo test`. No new dependencies.

## Global Constraints

- **Sim only.** No `LiveEnvironment` changes, no RB3011, no live runs.
- **No Ouros wiring** of any kind.
- **Zero surfaced-fact change.** The `Fact::ALL` surfaced alphabet stays exactly the current 14 facts, so the model's `engagement_facts` prompt table is byte-identical. All new predicates are `InstanceProbe`s (ground-truth, unsurfaced). The only prompt/menu change is a `category` field on `CardSpec`.
- **Identical legality for the current arsenal.** Every existing card's `precondition` must return the same boolean for every reachable state — proven by the migration-equivalence test (Task 5) against preserved legacy closures. This is a one-time refactor proof, NOT a standing "menu can never change" invariant (adding sibling tools later may change menus — that is the feature).
- **Balance re-measured, not asserted-identical.** Expect the 10-seed model-Blue batch to stay **3/10 (seeds 1,3,7)**; a deviation is investigated and explained, not silently accepted (Task 11).
- Follow existing `facts.rs` / `cards.rs` patterns; no unrelated refactoring.
- Work on branch `facts-as-data` (already created off the baseline commit); commit per task.

---

## File Structure

- **Create** `wargame/src/category.rs` — the `Category` enum (Tier 1 topology backbone) + its ordering/keys + tests.
- **Modify** `wargame/src/lib.rs` — add `pub mod category;`.
- **Modify** `wargame/src/facts.rs` — add `Requirement`, `InstanceProbe`, their evaluators, and tests. `Fact` enum and `Fact::ALL` are UNCHANGED.
- **Modify** `wargame/src/card.rs` — `Card` trait gains `category()`, `requires()`, `produces()`, `detection_surface()`; `precondition` becomes provided; `CardSpec` gains `category`.
- **Modify** `wargame/src/cards.rs` — every card declares `category()` + `requires()` (+ `produces()`/`detection_surface()` where non-empty); delete `precondition` overrides.
- **Modify** `wargame/src/graph.rs` — `CompositeCard.precond: fn` field becomes `requires: Vec<Requirement>`.

---

### Task 1: `Category` enum (Tier 1 topology)

**Files:**
- Create: `wargame/src/category.rs`
- Modify: `wargame/src/lib.rs:16-28` (module list)
- Test: in-file `#[cfg(test)] mod tests` in `category.rs`

**Interfaces:**
- Produces: `pub enum Category { InitialAccess, Discovery, CredentialAccess, PrivilegeEscalation, LateralMovement, Exfiltration, Detection, DefenseEvasion }`; `Category::key(&self) -> &'static str`; `Category::chain_order(&self) -> Option<u8>` (kill-chain index; `None` for cross-cutting `Detection`/`DefenseEvasion`); `Category::ENFORCED: [Category; 6]` (categories that must hold ≥1 tool).

- [ ] **Step 1: Write the failing test**

```rust
// wargame/src/category.rs  (tests module)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_chain_is_strictly_ordered() {
        let chain = [
            Category::InitialAccess, Category::Discovery, Category::CredentialAccess,
            Category::PrivilegeEscalation, Category::LateralMovement, Category::Exfiltration,
        ];
        let orders: Vec<u8> = chain.iter().map(|c| c.chain_order().unwrap()).collect();
        assert!(orders.windows(2).all(|w| w[0] < w[1]), "kill chain must be strictly increasing");
    }

    #[test]
    fn cross_cutting_categories_have_no_chain_order() {
        assert_eq!(Category::Detection.chain_order(), None);
        assert_eq!(Category::DefenseEvasion.chain_order(), None);
    }

    #[test]
    fn keys_are_unique_and_stable() {
        let all = [
            Category::InitialAccess, Category::Discovery, Category::CredentialAccess,
            Category::PrivilegeEscalation, Category::LateralMovement, Category::Exfiltration,
            Category::Detection, Category::DefenseEvasion,
        ];
        let mut keys: Vec<&str> = all.iter().map(|c| c.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), all.len(), "every category needs a distinct key");
        assert_eq!(Category::CredentialAccess.key(), "credential_access");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd wargame && cargo test category:: 2>&1 | tail -5`
Expected: FAIL — `category.rs` / `Category` does not exist (compile error).

- [ ] **Step 3: Write minimal implementation**

```rust
// wargame/src/category.rs  (top of file)
//! Tier-1 taxonomy: the ATT&CK-tactic kill chain. A `Category` is the *stage* a tool
//! belongs to (Red) or the attacker stage it counters / the cross-cutting Detection lane
//! (Blue). This is the fixed topology; tools compose over it. Facts key on category
//! progress, never on a specific tool — that is what keeps the arsenal open-ended.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    InitialAccess,
    Discovery,
    CredentialAccess,
    PrivilegeEscalation,
    LateralMovement,
    Exfiltration,
    /// Blue's cross-cutting monitoring/hunting lane — not a single attack stage.
    Detection,
    /// Red's cross-cutting evasion lane — reserved, no tool yet.
    DefenseEvasion,
}

impl Category {
    /// Categories that must each hold at least one tool in the current arsenal.
    pub const ENFORCED: [Category; 6] = [
        Category::InitialAccess,
        Category::Discovery,
        Category::CredentialAccess,
        Category::PrivilegeEscalation,
        Category::LateralMovement,
        Category::Detection,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            Category::InitialAccess => "initial_access",
            Category::Discovery => "discovery",
            Category::CredentialAccess => "credential_access",
            Category::PrivilegeEscalation => "privilege_escalation",
            Category::LateralMovement => "lateral_movement",
            Category::Exfiltration => "exfiltration",
            Category::Detection => "detection",
            Category::DefenseEvasion => "defense_evasion",
        }
    }

    /// Position in the linear kill chain; `None` for cross-cutting lanes.
    pub fn chain_order(&self) -> Option<u8> {
        match self {
            Category::InitialAccess => Some(0),
            Category::Discovery => Some(1),
            Category::CredentialAccess => Some(2),
            Category::PrivilegeEscalation => Some(3),
            Category::LateralMovement => Some(4),
            Category::Exfiltration => Some(5),
            Category::Detection | Category::DefenseEvasion => None,
        }
    }
}
```

Then add to `wargame/src/lib.rs` after `pub mod card;` (keep alphabetical-ish with the others):

```rust
pub mod category;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd wargame && cargo test category:: 2>&1 | tail -5`
Expected: PASS — 3 tests ok.

- [ ] **Step 5: Commit**

```bash
git add wargame/src/category.rs wargame/src/lib.rs
git commit -m "feat(wargame): add Category tier-1 taxonomy (kill-chain topology)"
```

---

### Task 2: `Requirement` + `InstanceProbe` (Tier 2, in facts.rs)

**Files:**
- Modify: `wargame/src/facts.rs` (append types + tests; `Fact`/`Fact::ALL` untouched)
- Test: `facts.rs` tests module

**Interfaces:**
- Consumes: `Fact` and `Fact::holds` (existing), `Technique`, `GameState` accessors.
- Produces:
  - `pub enum InstanceProbe { Performed(Technique), Detected(Technique), HasForwardPath, LateralPathPlanted, CredCompromiseKnown, UndetectedActivity, UndetectedAlert }` with `fn holds(&self, s: &GameState) -> bool`.
  - `pub enum Requirement { Category { fact: Fact, want: bool }, Instance { probe: InstanceProbe, want: bool } }` with `fn satisfied(&self, s: &GameState) -> bool` and constructors `have(Fact)`, `lack(Fact)`, `did(Technique)`, `not_yet(Technique)`, `fingerprinted(Technique)`, `probe(InstanceProbe)`, `no_probe(InstanceProbe)`.

- [ ] **Step 1: Write the failing test**

```rust
// wargame/src/facts.rs  (add to the existing tests module)
    #[test]
    fn instance_probe_performed_and_detected() {
        let mut s = base();
        assert!(!InstanceProbe::Performed(Technique::Recon).holds(&s));
        s.performed.push(Technique::Recon);
        assert!(InstanceProbe::Performed(Technique::Recon).holds(&s));
        assert!(!InstanceProbe::Detected(Technique::Recon).holds(&s), "performed != detected");
        s.alerts.push(crate::state::Alert { round: 1, technique: Technique::Recon, source: "m".into(), rule_id: "r".into(), level: 8 });
        assert!(InstanceProbe::Detected(Technique::Recon).holds(&s));
    }

    #[test]
    fn requirement_category_and_instance_respect_want() {
        let mut s = base();
        // ReachesDc false at start → have(ReachesDc) unsatisfied, lack(ReachesDc) satisfied
        assert!(!Requirement::have(Fact::ReachesDc).satisfied(&s));
        assert!(Requirement::lack(Fact::ReachesDc).satisfied(&s));
        // instance: not_yet(Recon) satisfied until performed
        assert!(Requirement::not_yet(Technique::Recon).satisfied(&s));
        s.performed.push(Technique::Recon);
        assert!(!Requirement::not_yet(Technique::Recon).satisfied(&s));
        assert!(Requirement::did(Technique::Recon).satisfied(&s));
    }

    #[test]
    fn undetected_probes_track_gaps() {
        let mut s = base();
        assert!(!InstanceProbe::UndetectedActivity.holds(&s));
        s.performed.push(Technique::Kerberoast);
        assert!(InstanceProbe::UndetectedActivity.holds(&s), "performed but unseen");
        s.alerts.push(crate::state::Alert { round: 1, technique: Technique::Kerberoast, source: "m".into(), rule_id: "r".into(), level: 8 });
        assert!(!InstanceProbe::UndetectedActivity.holds(&s), "now seen");
        assert!(InstanceProbe::UndetectedAlert.holds(&s), "alert has no detection rule yet");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd wargame && cargo test facts::tests::instance_probe 2>&1 | tail -5`
Expected: FAIL — `InstanceProbe`/`Requirement` not found (compile error).

- [ ] **Step 3: Write minimal implementation**

```rust
// wargame/src/facts.rs  (append after the Fact impl block, before the tests module)

/// A ground-truth legality gate that is NEVER surfaced to a model. This is Tier-2: the place
/// instance-specific gates (`Detected(Kerberoast)` — the precise counter fingerprint) live,
/// alongside topology/aggregate gates that must not leak into an agent's fact table
/// (`UndetectedActivity` would tell Blue hidden work exists). Keeping these out of `Fact::ALL`
/// is why the surfaced fact table is unchanged by this refactor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InstanceProbe {
    /// Red has already performed this exact technique (once-only guards).
    Performed(Technique),
    /// Blue has fingerprinted this exact technique (the precise-counter gate).
    Detected(Technique),
    /// Red has a forward hop to pivot/breach into.
    HasForwardPath,
    /// A DCSync-able ACL path exists in this scenario (the escalation misconfig).
    LateralPathPlanted,
    /// There is a cracked credential whose acquiring technique Blue has detected.
    CredCompromiseKnown,
    /// Some technique Red performed is not yet visible to Blue (a coverage gap to hunt).
    UndetectedActivity,
    /// Some alert Blue holds has no technique-based detection rule yet.
    UndetectedAlert,
}

impl InstanceProbe {
    pub fn holds(&self, s: &GameState) -> bool {
        match self {
            InstanceProbe::Performed(t) => s.performed_technique(*t),
            InstanceProbe::Detected(t) => s.blue_knows(*t),
            InstanceProbe::HasForwardPath => !s.next_hops().is_empty(),
            InstanceProbe::LateralPathPlanted => s.vuln(Technique::LateralMove),
            InstanceProbe::CredCompromiseKnown => s.creds.iter().any(|c| c.cracked && s.blue_knows(c.via)),
            InstanceProbe::UndetectedActivity => s.performed.iter().any(|t| !s.blue_knows(*t)),
            InstanceProbe::UndetectedAlert => s.alerts.iter().any(|a| !s.has_detection(a.technique)),
        }
    }
}

/// A card's legality expressed as data. Tier-1 `Category` requirements gate on a surfaced
/// [`Fact`]; Tier-2 `Instance` requirements gate on a ground-truth [`InstanceProbe`]. A card
/// is legal when all its requirements are satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Requirement {
    Category { fact: Fact, want: bool },
    Instance { probe: InstanceProbe, want: bool },
}

impl Requirement {
    pub fn have(fact: Fact) -> Self { Requirement::Category { fact, want: true } }
    pub fn lack(fact: Fact) -> Self { Requirement::Category { fact, want: false } }
    pub fn did(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Performed(t), want: true } }
    pub fn not_yet(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Performed(t), want: false } }
    pub fn fingerprinted(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Detected(t), want: true } }
    pub fn probe(p: InstanceProbe) -> Self { Requirement::Instance { probe: p, want: true } }
    pub fn no_probe(p: InstanceProbe) -> Self { Requirement::Instance { probe: p, want: false } }

    pub fn satisfied(&self, s: &GameState) -> bool {
        match self {
            Requirement::Category { fact, want } => fact.holds(s) == *want,
            Requirement::Instance { probe, want } => probe.holds(s) == *want,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd wargame && cargo test facts:: 2>&1 | tail -6`
Expected: PASS — existing 7 fact tests + 3 new tests ok.

- [ ] **Step 5: Commit**

```bash
git add wargame/src/facts.rs
git commit -m "feat(wargame): add Requirement/InstanceProbe two-tier legality alphabet"
```

---

### Task 3: `Card` trait + `CardSpec` plumbing (behavior-preserving)

**Files:**
- Modify: `wargame/src/card.rs:36-44` (`CardSpec`), `:86-125` (`Card` trait)
- Test: rely on the existing suite staying green (no behavior change yet)

**Interfaces:**
- Produces on `Card`: `fn category(&self) -> Category` (TEMP default `Category::Detection` — every card overrides it in Tasks 6–9; the default is removed in Task 10 to force compile-time declaration), `fn requires(&self) -> Vec<Requirement>` (default `vec![]`), `fn produces(&self) -> Vec<Fact>` (default `vec![]`), `fn detection_surface(&self) -> Vec<Technique>` (default `vec![]`). `precondition` becomes a provided method (still overridable): `self.requires().iter().all(|r| r.satisfied(state))`.
- Produces on `CardSpec`: added field `pub category: Category`.

Rationale for scope: `requires()`/`produces()`/`detection_surface()` are added as trait methods (programmatically introspectable via the registry) but only `category` is added to the serialized `CardSpec`. Wiring the full requires/produces into `CardSpec`'s prompt form is deferred to the forest sub-project that consumes them (YAGNI; also keeps the prompt change minimal so balance stays measurable).

- [ ] **Step 1: Write the failing test**

```rust
// wargame/src/card.rs  (add a tests module at the end)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;

    // A card that declares requirements but no precondition override uses the provided path.
    struct Dummy;
    impl Card for Dummy {
        fn id(&self) -> &'static str { "dummy" }
        fn side(&self) -> Side { Side::Blue }
        fn technique(&self) -> Technique { Technique::Recon }
        fn category(&self) -> Category { Category::Detection }
        fn describe(&self) -> &'static str { "dummy" }
        fn requires(&self) -> Vec<crate::facts::Requirement> {
            vec![crate::facts::Requirement::lack(crate::facts::Fact::Monitoring)]
        }
        fn play(&self, _s: &mut GameState, _p: &serde_json::Value, _e: &mut dyn Environment) -> Outcome {
            Outcome { success: true, narrative: String::new(), detection_surface: vec![] }
        }
    }

    #[test]
    fn provided_precondition_evaluates_requires() {
        let mut s = GameState::new(vec![]);
        assert!(Dummy.precondition(&s), "monitoring off → lack(Monitoring) satisfied");
        s.monitoring = true;
        assert!(!Dummy.precondition(&s), "monitoring on → lack(Monitoring) fails");
    }

    #[test]
    fn spec_carries_category() {
        assert_eq!(Dummy.spec().category, Category::Detection);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd wargame && cargo test card::tests 2>&1 | tail -8`
Expected: FAIL — `category` not a member of `Card`/`CardSpec`; `requires` unknown (compile error).

- [ ] **Step 3: Write minimal implementation**

In `wargame/src/card.rs`, add imports at top (after existing `use`):

```rust
use crate::category::Category;
use crate::facts::{Fact, Requirement};
```

Add `category` to `CardSpec`:

```rust
pub struct CardSpec {
    pub id: String,
    pub side: Side,
    pub technique: Technique,
    pub category: Category,
    pub summary: String,
    pub params_schema: serde_json::Value,
}
```

In the `Card` trait, add the new methods and make `precondition` provided. Replace the current required `fn precondition(&self, state: &GameState) -> bool;` with:

```rust
    /// Tier-1 kill-chain category this card belongs to / counters.
    /// TEMP default removed in the final task so every card must declare it.
    fn category(&self) -> Category { Category::Detection }

    /// Declarative legality — the facts/probes that must hold. Provided `precondition`
    /// evaluates these; cards should override this, not `precondition`.
    fn requires(&self) -> Vec<Requirement> { vec![] }

    /// Facts this card flips true on success (for the forest/builder; not consumed yet).
    fn produces(&self) -> Vec<Fact> { vec![] }

    /// Declared ATT&CK exposure — the instance signature blue could detect.
    fn detection_surface(&self) -> Vec<Technique> { vec![] }

    /// Legal in this state? Provided: all requirements satisfied. Overridable for cards not
    /// yet migrated to `requires()`.
    fn precondition(&self, state: &GameState) -> bool {
        self.requires().iter().all(|r| r.satisfied(state))
    }
```

Update the provided `spec()` to include the category:

```rust
    fn spec(&self) -> CardSpec {
        CardSpec {
            id: self.id().to_string(),
            side: self.side(),
            technique: self.technique(),
            category: self.category(),
            summary: self.describe().to_string(),
            params_schema: self.params_schema(),
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd wargame && cargo test 2>&1 | tail -8`
Expected: PASS — all existing tests + the 2 new `card::tests` pass. (Existing cards still override `precondition`, so behavior is unchanged; any code building a `CardSpec` literal directly, if present, must add `category` — grep `CardSpec {` to confirm none outside `spec()`.)

- [ ] **Step 5: Commit**

```bash
git add wargame/src/card.rs
git commit -m "feat(wargame): Card trait gains category/requires/produces; precondition provided"
```

---

### Task 4: `CompositeCard` carries `requires` data (graph.rs)

**Files:**
- Modify: `wargame/src/graph.rs:96-121` (`CompositeCard` struct + `Card` impl), `wargame/src/cards.rs:72-82` (`kerberoast_card`)
- Test: existing suite green + a legality check

**Interfaces:**
- Consumes: `Requirement`, `Category` from earlier tasks.
- Produces: `CompositeCard` fields `pub category: Category`, `pub requires: Vec<Requirement>`, `pub produces: Vec<Fact>`, `pub surface: Vec<Technique>` replacing `precond: fn(&GameState) -> bool`. Its `Card::category/requires/produces/detection_surface` return these fields; `precondition` uses the provided default (delete the override).

- [ ] **Step 1: Write the failing test**

```rust
// wargame/src/graph.rs  (add a tests module at the end)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;
    use crate::facts::Requirement;
    use crate::card::Card;

    #[test]
    fn composite_precondition_uses_requires_data() {
        let c = CompositeCard {
            id: "t", side: Side::Red, technique: Technique::Kerberoast, summary: "t",
            category: Category::CredentialAccess,
            requires: vec![Requirement::have(crate::facts::Fact::ReachesDc)],
            produces: vec![], surface: vec![], nodes: vec![],
        };
        let mut s = GameState::new(vec![]);
        assert!(!c.precondition(&s), "ReachesDc false → illegal");
        s.add_zone("vlan30");
        assert!(c.precondition(&s), "ReachesDc true → legal");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd wargame && cargo test graph::tests 2>&1 | tail -8`
Expected: FAIL — `CompositeCard` has no `requires`/`category` fields (compile error).

- [ ] **Step 3: Write minimal implementation**

In `wargame/src/graph.rs`, add imports: `use crate::facts::{Fact, Requirement};` and `use crate::category::Category;`. Replace the `precond` field and the `precondition` override:

```rust
pub struct CompositeCard {
    pub id: &'static str,
    pub side: Side,
    pub technique: Technique,
    pub summary: &'static str,
    pub category: Category,
    pub requires: Vec<Requirement>,
    pub produces: Vec<Fact>,
    pub surface: Vec<Technique>,
    pub nodes: Vec<Box<dyn Primitive>>,
}
```

In `impl Card for CompositeCard`, delete the `fn precondition` override (use the provided default) and add:

```rust
    fn category(&self) -> Category { self.category }
    fn requires(&self) -> Vec<Requirement> { self.requires.clone() }
    fn produces(&self) -> Vec<Fact> { self.produces.clone() }
    fn detection_surface(&self) -> Vec<Technique> { self.surface.clone() }
```

Update `kerberoast_card()` in `wargame/src/cards.rs` (add `use crate::facts::{Fact, Requirement};` to cards.rs imports, and `use crate::category::Category;`):

```rust
fn kerberoast_card() -> CompositeCard {
    CompositeCard {
        id: "kerberoast",
        side: Side::Red,
        technique: Technique::Kerberoast,
        summary: "Kerberoast: enum SPNs -> request TGS -> crack (fails vs AES)",
        category: Category::CredentialAccess,
        // Can't roast the domain until red has crossed the network to reach it.
        requires: vec![Requirement::have(Fact::ReachesDc)],
        produces: vec![Fact::HasCred], // on a successful crack
        surface: vec![Technique::Recon, Technique::Kerberoast], // enum_spns + request_tgs
        nodes: vec![Box::new(CrackHash), Box::new(EnumSpns), Box::new(RequestTgs)],
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd wargame && cargo test 2>&1 | tail -8`
Expected: PASS — all tests including `graph::tests`. (If `CompositeCard::new` or other `precond` references exist, grep `precond` and update; the read showed only these two sites.)

- [ ] **Step 5: Commit**

```bash
git add wargame/src/graph.rs wargame/src/cards.rs
git commit -m "feat(wargame): CompositeCard carries requires/category as data, not a fn pointer"
```

---

### Task 5: Migration-equivalence test harness (the safety net, established green)

**Files:**
- Create: `wargame/tests/precondition_equivalence.rs` (integration test)
- Test: itself

**Interfaces:**
- Consumes: `purple_wargame::registry::default_registry`, `GameState`, `Technique`, `Cred`, `Alert`.
- Produces: a seeded-state generator + a per-card legacy-closure oracle. Asserts `card.precondition(state) == legacy(card.id(), state)` for every card over N≥500 states. Established GREEN now (before card migration) by encoding the *current* closures as the oracle; it then guards Tasks 6–9.

- [ ] **Step 1: Write the test (it is the deliverable; it must pass against the pre-migration tree)**

```rust
// wargame/tests/precondition_equivalence.rs
//! One-time proof the facts-as-data migration preserves each existing card's legality.
//! The `legacy` fn mirrors the ORIGINAL closures (card.rs @ baseline commit) verbatim.
//! NOTE: not a standing invariant — adding sibling tools later may change menus by design.

use purple_wargame::registry::default_registry;
use purple_wargame::state::{Alert, Cred, GameState, Technique};

const TECHS: [Technique; 9] = [
    Technique::InitialAccess, Technique::Recon, Technique::Pivot, Technique::Kerberoast,
    Technique::AsRepRoast, Technique::BloodHound, Technique::CredSpray, Technique::LateralMove,
    Technique::Exfil,
];

// Cheap splitmix64 so the sweep is deterministic without external crates.
fn mix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Build a pseudo-random reachable-ish state: vary zones, creds, performed, alerts,
/// detections, and blue posture flags.
fn gen_state(seed: u64) -> GameState {
    let mut r = seed;
    let mut s = GameState::new(vec![]);
    // topology: maybe internal, maybe attack-ready, sometimes a forward hop
    if mix(&mut r) & 1 == 1 { s.add_zone("vlan10"); }
    if mix(&mut r) & 1 == 1 { s.add_zone("vlan30"); } // objective_zone by default → attack_ready
    // extra forward edge so next_hops() can be non-empty even when internal
    if mix(&mut r) & 1 == 1 { s.edges.push(("vlan10".into(), "vlan20".into())); }
    // performed techniques
    for t in TECHS { if mix(&mut r) & 1 == 1 { s.performed.push(t); } }
    // creds (cracked, with a via technique)
    if mix(&mut r) & 1 == 1 {
        let via = if mix(&mut r) & 1 == 1 { Technique::Kerberoast } else { Technique::AsRepRoast };
        s.creds.push(Cred { principal: "p".into(), secret: None, cracked: true, via });
    }
    // alerts (what blue has seen)
    for t in TECHS { if mix(&mut r) & 1 == 1 {
        s.alerts.push(Alert { round: 1, technique: t, source: "gen".into(), rule_id: "r".into(), level: 5 });
    }}
    // detections (rules written)
    for t in TECHS { if mix(&mut r) & 1 == 1 {
        s.detections.push(purple_wargame::state::Detection {
            id: "d".into(), technique: t, deployed_round: 1, technique_based: true, fidelity: "robust".into(),
        });
    }}
    // scenario misconfigs vary
    s.misconfigs = TECHS.iter().copied().filter(|_| mix(&mut r) & 1 == 1).collect();
    // blue posture
    s.monitoring = mix(&mut r) & 1 == 1;
    s.auto_response = mix(&mut r) & 1 == 1;
    s.rc4_disabled = mix(&mut r) & 1 == 1;
    s.preauth_required = mix(&mut r) & 1 == 1;
    s.acl_path_fixed = mix(&mut r) & 1 == 1;
    s.red_reached_da = mix(&mut r) & 1 == 1;
    s
}

/// The ORIGINAL preconditions, verbatim from the baseline `card.rs`/`cards.rs`.
fn legacy(id: &str, s: &GameState) -> bool {
    match id {
        "initial_access" => !s.has_internal() && !s.next_hops().is_empty(),
        "pivot" => s.has_internal() && !s.attack_ready() && !s.next_hops().is_empty(),
        "recon" => s.attack_ready() && !s.performed_technique(Technique::Recon),
        "kerberoast" => s.attack_ready(),
        "asrep_roast" => s.attack_ready(),
        "bloodhound" => s.has_cracked_cred() && !s.performed_technique(Technique::BloodHound),
        "escalate_da" => s.vuln(Technique::LateralMove) && s.has_cracked_cred()
            && s.performed_technique(Technique::BloodHound) && !s.red_reached_da && !s.acl_path_fixed,
        "monitor" => !s.monitoring,
        "active_response" => !s.auto_response,
        "remediate_acl" => !s.acl_path_fixed
            && s.alerts.iter().any(|a| matches!(a.technique, Technique::Recon | Technique::BloodHound)),
        "enforce_aes" => !s.rc4_disabled && s.blue_knows(Technique::Kerberoast),
        "enforce_preauth" => !s.preauth_required && s.blue_knows(Technique::AsRepRoast),
        "rotate_creds" => s.creds.iter().any(|c| c.cracked && s.blue_knows(c.via)),
        "hunt" => s.performed.iter().any(|t| !s.blue_knows(*t)),
        "deploy_detection" => s.alerts.iter().any(|a| !s.has_detection(a.technique)),
        "segment" => (s.blue_knows(Technique::Pivot) || s.blue_knows(Technique::InitialAccess))
            && !s.attack_ready() && !s.red_reached_da && !s.next_hops().is_empty(),
        other => panic!("unknown card id in oracle: {other}"),
    }
}

#[test]
fn every_card_precondition_matches_legacy_over_the_reachable_space() {
    let reg = default_registry();
    for seed in 0..800u64 {
        let s = gen_state(seed);
        for spec in reg.all_specs() {
            let card = reg.get(&spec.id).unwrap();
            assert_eq!(
                card.precondition(&s),
                legacy(&spec.id, &s),
                "card {} disagreed with legacy oracle at seed {}", spec.id, seed
            );
        }
    }
}
```

- [ ] **Step 2: Run to verify it PASSES on the current tree**

Run: `cd wargame && cargo test --test precondition_equivalence 2>&1 | tail -6`
Expected: PASS — the harness matches the unmigrated cards (they still use their original overrides). This proves the oracle is faithful before we change anything. (If it fails now, the oracle is wrong — fix the oracle, not the cards.)

- [ ] **Step 3: Commit**

```bash
git add wargame/tests/precondition_equivalence.rs
git commit -m "test(wargame): migration-equivalence harness (legacy precondition oracle)"
```

---

### Task 6: Migrate RED progress cards (initial_access, pivot, recon, bloodhound)

**Files:**
- Modify: `wargame/src/cards.rs` (the four card impls)
- Test: `precondition_equivalence` stays green

**Interfaces:**
- Consumes: `Requirement`, `Fact`, `InstanceProbe`, `Category` (imports added in Task 4).
- Produces: each card gains `category()`/`requires()`/`produces()`/`detection_surface()`, drops `precondition`.

- [ ] **Step 1: Edit the four cards** (delete each `fn precondition`, add the methods)

```rust
// InitialAccess
    fn category(&self) -> Category { Category::InitialAccess }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::lack(Fact::Foothold), Requirement::probe(InstanceProbe::HasForwardPath)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::Foothold] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::InitialAccess] }

// Pivot
    fn category(&self) -> Category { Category::LateralMovement }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::have(Fact::Foothold), Requirement::lack(Fact::ReachesDc),
             Requirement::probe(InstanceProbe::HasForwardPath)]
    }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::Pivot] }
    // produces: advances position; ReachesDc only when the hop is the objective → left empty

// Recon
    fn category(&self) -> Category { Category::Discovery }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::have(Fact::ReachesDc), Requirement::not_yet(Technique::Recon)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::Scouted] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::Recon] }

// BloodHoundCollect
    fn category(&self) -> Category { Category::Discovery }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::have(Fact::HasCred), Requirement::not_yet(Technique::BloodHound)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::PathMapped, Fact::Scouted] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::BloodHound] }
```

Add `InstanceProbe` to the `use crate::facts::...` line in cards.rs: `use crate::facts::{Fact, InstanceProbe, Requirement};`

- [ ] **Step 2: Run equivalence + full suite**

Run: `cd wargame && cargo test 2>&1 | tail -8`
Expected: PASS — `precondition_equivalence` still green; migrated cards now route through `requires()`.

- [ ] **Step 3: Commit**

```bash
git add wargame/src/cards.rs
git commit -m "refactor(wargame): migrate red progress cards to requires() data"
```

---

### Task 7: Migrate RED attack card (asrep_roast, escalate_da)

**Files:** `wargame/src/cards.rs`
**Interfaces:** as Task 6.

- [ ] **Step 1: Edit** (delete each `fn precondition`, add methods)

```rust
// AsRepRoast
    fn category(&self) -> Category { Category::CredentialAccess }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::have(Fact::ReachesDc)] }
    fn produces(&self) -> Vec<Fact> { vec![Fact::HasCred] } // on success
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::AsRepRoast] }

// EscalateDa
    fn category(&self) -> Category { Category::PrivilegeEscalation }
    fn requires(&self) -> Vec<Requirement> {
        vec![
            Requirement::probe(InstanceProbe::LateralPathPlanted),
            Requirement::have(Fact::HasCred),
            Requirement::have(Fact::PathMapped),
            Requirement::lack(Fact::DomainAdmin),
            Requirement::lack(Fact::PathSevered),
        ]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::DomainAdmin] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::LateralMove] }
```

- [ ] **Step 2: Run tests** — `cd wargame && cargo test 2>&1 | tail -8` → Expected PASS.
- [ ] **Step 3: Commit** — `git add wargame/src/cards.rs && git commit -m "refactor(wargame): migrate red attack cards to requires() data"`

---

### Task 8: Migrate BLUE posture/detection cards (monitor, active_response, hunt, deploy_detection)

**Files:** `wargame/src/cards.rs`
**Interfaces:** as Task 6.

- [ ] **Step 1: Edit** (delete `fn precondition`, add methods)

```rust
// ContinuousMonitoring
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::lack(Fact::Monitoring)] }
    fn produces(&self) -> Vec<Fact> { vec![Fact::Monitoring] }

// ActiveResponse
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::lack(Fact::AutoResponse)] }
    fn produces(&self) -> Vec<Fact> { vec![Fact::AutoResponse] }

// Hunt
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::probe(InstanceProbe::UndetectedActivity)] }
    // produces: raises observations (alerts/detections), not a clean fact flip → empty

// DeployDetection
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::probe(InstanceProbe::UndetectedAlert)] }
    // produces: detection coverage, not a fact → empty
```

- [ ] **Step 2: Run tests** — `cd wargame && cargo test 2>&1 | tail -8` → Expected PASS.
- [ ] **Step 3: Commit** — `git add wargame/src/cards.rs && git commit -m "refactor(wargame): migrate blue detection cards to requires() data"`

---

### Task 9: Migrate BLUE counter cards (remediate_acl, enforce_aes, enforce_preauth, rotate_creds, segment)

**Files:** `wargame/src/cards.rs`
**Interfaces:** as Task 6.

- [ ] **Step 1: Edit** (delete `fn precondition`, add methods)

```rust
// FixAclPath (remediate_acl) — keeps its explanatory comment above requires()
    fn category(&self) -> Category { Category::PrivilegeEscalation }
    fn requires(&self) -> Vec<Requirement> {
        // Category-gated: any discovery Blue has *seen* (ScoutDetected) unlocks the cut.
        vec![Requirement::lack(Fact::PathSevered), Requirement::have(Fact::ScoutDetected)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::PathSevered] }

// EnforceAes — instance-gated (the two-level point)
    fn category(&self) -> Category { Category::CredentialAccess }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::lack(Fact::AesEnforced), Requirement::fingerprinted(Technique::Kerberoast)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::AesEnforced] }

// EnforcePreauth — instance-gated
    fn category(&self) -> Category { Category::CredentialAccess }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::lack(Fact::PreauthEnforced), Requirement::fingerprinted(Technique::AsRepRoast)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::PreauthEnforced] }

// HardenCreds (rotate_creds)
    fn category(&self) -> Category { Category::CredentialAccess }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::probe(InstanceProbe::CredCompromiseKnown)] }
    // produces: invalidates a cred (removal), not a fact flip → empty

// Segment
    fn category(&self) -> Category { Category::LateralMovement }
    fn requires(&self) -> Vec<Requirement> {
        vec![
            Requirement::have(Fact::IntrusionDetected),
            Requirement::lack(Fact::ReachesDc),
            Requirement::lack(Fact::DomainAdmin),
            Requirement::probe(InstanceProbe::HasForwardPath),
        ]
    }
    // produces: removes forward edges (may flip HasForwardPath false) → empty
```

- [ ] **Step 2: Run tests** — `cd wargame && cargo test 2>&1 | tail -8` → Expected PASS (all 16 cards now migrated; `precondition_equivalence` green; no `fn precondition` overrides remain in cards.rs — grep to confirm: `grep -c 'fn precondition' wargame/src/cards.rs` → 0).
- [ ] **Step 3: Commit** — `git add wargame/src/cards.rs && git commit -m "refactor(wargame): migrate blue counter cards to requires() data"`

---

### Task 10: Make `category()` required + structural-invariant tests

**Files:**
- Modify: `wargame/src/card.rs` (remove the temporary `category()` default)
- Create: `wargame/tests/taxonomy.rs`
- Test: itself

**Interfaces:**
- Consumes: `default_registry`, `Category`.
- Produces: compile-time proof every card declares a category (removing the default), plus runtime taxonomy invariants.

- [ ] **Step 1: Write the failing test**

```rust
// wargame/tests/taxonomy.rs
use purple_wargame::category::Category;
use purple_wargame::registry::default_registry;
use std::collections::BTreeMap;

#[test]
fn every_enforced_category_holds_at_least_one_tool() {
    let reg = default_registry();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for spec in reg.all_specs() {
        *counts.entry(spec.category.key()).or_default() += 1;
    }
    for cat in Category::ENFORCED {
        assert!(counts.get(cat.key()).copied().unwrap_or(0) >= 1,
            "enforced category {} has no tool", cat.key());
    }
}

#[test]
fn each_card_has_the_expected_category() {
    let reg = default_registry();
    let expect: &[(&str, Category)] = &[
        ("initial_access", Category::InitialAccess),
        ("pivot", Category::LateralMovement),
        ("recon", Category::Discovery),
        ("kerberoast", Category::CredentialAccess),
        ("asrep_roast", Category::CredentialAccess),
        ("bloodhound", Category::Discovery),
        ("escalate_da", Category::PrivilegeEscalation),
        ("monitor", Category::Detection),
        ("active_response", Category::Detection),
        ("hunt", Category::Detection),
        ("deploy_detection", Category::Detection),
        ("remediate_acl", Category::PrivilegeEscalation),
        ("enforce_aes", Category::CredentialAccess),
        ("enforce_preauth", Category::CredentialAccess),
        ("rotate_creds", Category::CredentialAccess),
        ("segment", Category::LateralMovement),
    ];
    for (id, cat) in expect {
        let spec = reg.get(id).unwrap().spec();
        assert_eq!(spec.category, *cat, "card {id} has wrong category");
    }
}
```

- [ ] **Step 2: Run to verify it passes with the default still present**

Run: `cd wargame && cargo test --test taxonomy 2>&1 | tail -6`
Expected: PASS (categories were set in Tasks 6–9).

- [ ] **Step 3: Remove the temporary default to force declaration**

In `wargame/src/card.rs`, change the provided `category()` back to a required method:

```rust
    /// Tier-1 kill-chain category this card belongs to / counters. Required — every card declares it.
    fn category(&self) -> Category;
```

- [ ] **Step 4: Run the full suite**

Run: `cd wargame && cargo test 2>&1 | tail -8`
Expected: PASS — compiles only because all 16 cards + `CompositeCard` declare `category()`. Any card missing it is now a compile error (the guarantee we want).

- [ ] **Step 5: Commit**

```bash
git add wargame/src/card.rs wargame/tests/taxonomy.rs
git commit -m "feat(wargame): require category() on every card + taxonomy invariants"
```

---

### Task 11: Balance re-measurement (verification, not a unit test)

**Files:** none (produces a recorded result; optionally append to `wargame/WARGAME.md`)

**Interfaces:** Consumes the built `purple-wargame` binary + local Ollama.

- [ ] **Step 1: Build release-ish debug binary**

Run: `cd wargame && cargo build 2>&1 | tail -3`
Expected: clean build.

- [ ] **Step 2: Run the 10-seed model-Blue batch**

Run (localhost Ollama, qwen2.5:7b; each match ~30–90s):

```bash
cd wargame
export WARGAME_MODEL_URL="http://localhost:11434/v1/chat/completions"
for seed in 1 2 3 4 5 6 7 8 9 10; do
  WARGAME_SEED=$seed WARGAME_MODEL_DEBUG=1 ./target/debug/purple-wargame cli model 2>/dev/null \
    | grep -iE 'BLUE WINS|RED WINS|winner' | tail -1 | sed "s/^/seed $seed: /"
done
```

Expected: **BLUE wins on seeds 1, 3, 7 (3/10)**, matching the pre-refactor baseline. BLUE should win iff it played a path-cut (`remediate_acl`/`segment`).

- [ ] **Step 3: Interpret**

- If **3/10 on {1,3,7}**: representation change moved nothing — done. Record the result.
- If **different**: do NOT accept silently. Diff which card's legality changed for the diverging seed (compare `grep '[model] Blue'` traces), confirm against the equivalence test, and explain the cause before closing. A menu change here would indicate an oracle/mapping error to fix, since no new tools were added.

- [ ] **Step 4: Commit the recorded result** (if you note it in WARGAME.md)

```bash
git add wargame/WARGAME.md
git commit -m "docs(wargame): record facts-as-data balance re-measurement (3/10)"
```

---

## Self-Review

**Spec coverage:**
- Two-tier alphabet (Category + Requirement/Instance) → Tasks 1–2. ✓
- Facts name capabilities, no instance-named facts → Task 2 (all instance/topology gates are `InstanceProbe`s; `Fact` unchanged). ✓
- `precondition` provided from `requires()`, cards drop overrides → Tasks 3, 6–9. ✓
- `CompositeCard` carries data not a fn pointer → Task 4. ✓
- Migration-equivalence for existing cards → Task 5 (+ green through 6–9). ✓
- Structural invariants (≥1 tool/category, kill-chain order, per-card category) → Tasks 1, 10. ✓
- `produces()`/`detection_surface()` declared → Tasks 4, 6–9. ✓
- Balance re-measured (3/10, deviation investigated) → Task 11. ✓
- Zero surfaced-fact change (prompt table byte-identical) → global constraint honored by routing new predicates through `InstanceProbe`. ✓

**Deliberate scope refinements vs spec (flag on handoff):**
1. `CardSpec` gains only `category` (not the full requires/produces/detection_surface serialized form) — those stay Card trait methods, introspectable via the registry; embedding them in the serialized prompt is deferred to the forest sub-project that consumes them (YAGNI + keeps the prompt change minimal so balance stays measurable).
2. `HasForwardPath`/`LateralPathPlanted` are implemented as `InstanceProbe`s (ground-truth, unsurfaced) rather than surfaced `Fact`s, to hold the "zero surfaced-fact change" guarantee. They remain category-agnostic gates; nothing about the two-level story changes.

**Placeholder scan:** none — every code step is complete.

**Type consistency:** `Requirement::{have,lack,did,not_yet,fingerprinted,probe,no_probe}`, `InstanceProbe::{Performed,Detected,HasForwardPath,LateralPathPlanted,CredCompromiseKnown,UndetectedActivity,UndetectedAlert}`, `Category::{... , ENFORCED, key, chain_order}` used identically across Tasks 1–10. `detection_surface()` name matches the existing `Outcome.detection_surface` field intent. ✓
