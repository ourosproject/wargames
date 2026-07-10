# Facts-as-Data v2 (Categorical) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the built v1 facts-as-data foundation to the full v2 categorical vocabulary — full ATT&CK + D3FEND categories and a two-level (cheap `SawCategory` / expensive `Identified`) detection model — then re-measure balance.

**Architecture:** The v1 substrate (`Fact` surfaced capability facts, `InstanceProbe` ground-truth Tier-2 gates, `Requirement` = `Category{fact}` | `Instance{probe}`, `Category` tier, `precondition` as a provided method over `requires()`) is KEPT. This plan (1) widens `Category` to 14 ATT&CK tactics + 6 D3FEND defensive tactics and maps every `Technique` to its attack tactic; (2) adds two parameterized detection probes — `SawCategory(Category)` (any alert in a tactic) and `Identified(Technique)` (a *deployed rule*, stricter than the existing `Detected`=alert) — plus an `AnyOf` requirement for disjunction; (3) rewires Blue's counter cards to the two-level model and D3FEND categories; (4) replaces the three hardcoded detection `Fact`s with the parameterized surface; (5) updates the migration oracle for the two cards whose semantics change on purpose; (6) re-measures the sim win-rate.

**Tech Stack:** Rust, `cargo test`, `cargo run -- cli model` against local Ollama (qwen2.5:7b).

## Global Constraints

- Sim only. No `LiveEnvironment` / RB3011 / live runs. No Ouros wiring.
- Extend, do not rebuild: keep the `Requirement`/`InstanceProbe`/`Category`/`precondition`-as-data machinery.
- Balance is **re-measured, not preserved** — some preconditions change by design (`enforce_aes`/`enforce_preauth`). The migration oracle changes with them; it is not a "keep 3/10" gate.
- `Technique::category()` mapping is load-bearing: `Pivot → LateralMovement`, `LateralMove → CredentialAccess`, `Recon`+`BloodHound → Discovery`, `Kerberoast`+`AsRepRoast`+`CredSpray → CredentialAccess`, `InitialAccess → InitialAccess`, `Exfil → Exfiltration`. This keeps `SawCategory(Discovery)` == old `ScoutDetected` and `SawCategory(InitialAccess)|SawCategory(LateralMovement)` == old `IntrusionDetected`.
- Model re-measure env: `WARGAME_MODEL_URL=http://127.0.0.1:11434/v1/chat/completions`, `WARGAME_MODEL=qwen2.5:7b`, `WARGAME_SEED=<n>`, `cargo run -- cli model`.

---

### Task 1: Widen `Category` (ATT&CK + D3FEND) + `Technique::category()`

**Files:**
- Modify: `wargame/src/category.rs` (enum + `key`/`chain_order`/`ENFORCED` + add `tactic_id`, `is_defensive`)
- Modify: `wargame/src/state.rs` (add `Technique::category()`)
- Test: inline `#[cfg(test)]` in both files

**Interfaces:**
- Produces: `Category` variants — attack (14 ATT&CK): `Reconnaissance, ResourceDevelopment, InitialAccess, Execution, Persistence, PrivilegeEscalation, DefenseEvasion, CredentialAccess, Discovery, LateralMovement, Collection, CommandAndControl, Exfiltration, Impact`; defense (D3FEND): the EXISTING `Detection` variant IS the D3FEND "Detect" lane (do NOT rename it — Blue cards reference `Category::Detection`), plus ADD `Harden, Isolate, Evict, Deceive, Model`. `Category::tactic_id(&self) -> &'static str` (`"TA0006"` for attack tactics, `""` for all defensive lanes). `Category::is_defensive(&self) -> bool` (true for `Detection, Harden, Isolate, Evict, Deceive, Model`). `Category::key` extended. `Technique::category(&self) -> Category`.
- Consumes: `Technique` (state.rs).
- NOTE: `DefenseEvasion` is an ATTACK tactic (TA0005), not a defensive lane — keep it non-defensive.
- NOTE: This task touches ONLY `category.rs` + `state.rs`. No `cards.rs` edits (the `Detection` variant is preserved, so all existing `Category::Detection` references keep compiling). Adding variants forces new match arms in `key()`/`chain_order()`/`is_defensive()`/`tactic_id()` — add them all or the file won't compile.

- [ ] **Step 1: Failing test — Technique→Category mapping (state.rs)**
```rust
#[test]
fn every_technique_maps_to_an_attack_category() {
    use Technique::*;
    assert_eq!(Pivot.category(), crate::category::Category::LateralMovement);
    assert_eq!(LateralMove.category(), crate::category::Category::CredentialAccess);
    assert_eq!(Recon.category(), crate::category::Category::Discovery);
    assert_eq!(BloodHound.category(), crate::category::Category::Discovery);
    assert_eq!(Kerberoast.category(), crate::category::Category::CredentialAccess);
    assert_eq!(InitialAccess.category(), crate::category::Category::InitialAccess);
    for t in [InitialAccess, Recon, Pivot, Kerberoast, AsRepRoast, BloodHound, CredSpray, LateralMove, Exfil] {
        assert!(!t.category().is_defensive(), "attack technique must map to an attack category");
    }
}
```

- [ ] **Step 2: Run — expect FAIL** (`cargo test -p purple-wargame every_technique_maps`): FAIL, `no method named category`.

- [ ] **Step 3: Implement — widen `Category`** in `category.rs`: keep the existing variants (`InitialAccess, Discovery, CredentialAccess, PrivilegeEscalation, LateralMovement, Exfiltration, Detection, DefenseEvasion`); ADD the missing ATT&CK tactics (`Reconnaissance, ResourceDevelopment, Execution, Persistence, Collection, CommandAndControl, Impact`) and the D3FEND lanes (`Harden, Isolate, Evict, Deceive, Model`). Extend `key()` for every new variant (`"reconnaissance"`, `"resource_development"`, `"execution"`, `"persistence"`, `"collection"`, `"command_and_control"`, `"impact"`, `"harden"`, `"isolate"`, `"evict"`, `"deceive"`, `"model"`). Add:
```rust
pub fn is_defensive(&self) -> bool {
    matches!(self, Category::Detection | Category::Harden | Category::Isolate
        | Category::Evict | Category::Deceive | Category::Model)
}
pub fn tactic_id(&self) -> &'static str {
    match self {
        Category::Reconnaissance => "TA0043", Category::ResourceDevelopment => "TA0042",
        Category::InitialAccess => "TA0001", Category::Execution => "TA0002",
        Category::Persistence => "TA0003", Category::PrivilegeEscalation => "TA0004",
        Category::DefenseEvasion => "TA0005", Category::CredentialAccess => "TA0006",
        Category::Discovery => "TA0007", Category::LateralMovement => "TA0008",
        Category::Collection => "TA0009", Category::CommandAndControl => "TA0011",
        Category::Exfiltration => "TA0010", Category::Impact => "TA0040",
        _ => "", // defensive tactics have no ATT&CK id
    }
}
```
Update `chain_order()` to return `Some(idx)` only for the linear attack chain `InitialAccess(0) < Discovery(1) < CredentialAccess(2) < PrivilegeEscalation(3) < LateralMovement(4) < Exfiltration(5)`, `None` for all other attack tactics AND all defensive tactics. Keep `ENFORCED` as the categories that must hold ≥1 tool (updated in Task 5 after re-categorization; for now leave the six current entries).

- [ ] **Step 4: Implement — `Technique::category()`** in `state.rs`:
```rust
pub fn category(&self) -> crate::category::Category {
    use crate::category::Category;
    match self {
        Technique::InitialAccess => Category::InitialAccess,
        Technique::Recon | Technique::BloodHound => Category::Discovery,
        Technique::Pivot => Category::LateralMovement,
        Technique::Kerberoast | Technique::AsRepRoast | Technique::CredSpray
            | Technique::LateralMove => Category::CredentialAccess,
        Technique::Exfil => Category::Exfiltration,
    }
}
```

- [ ] **Step 5: Update `category.rs` tests** — the existing `keys_are_unique_and_stable` array must list all 20 variants; `kill_chain_is_strictly_ordered` unchanged; add `defensive_tactics_have_no_attack_id_or_chain_order` asserting `Category::Harden.tactic_id()==""` and `Category::Harden.chain_order()==None`.

- [ ] **Step 6: Run — expect PASS** (`cargo test -p purple-wargame`): all green.

- [ ] **Step 7: Commit**
```bash
git add wargame/src/category.rs wargame/src/state.rs
git commit -m "feat(wargame): widen Category to ATT&CK+D3FEND, add Technique::category()"
```

---

### Task 2: Two-level detection probes + `AnyOf`

**Files:**
- Modify: `wargame/src/facts.rs` (`InstanceProbe` variants + `holds`, `Requirement::AnyOf` + `satisfied` + helpers, tests)

**Interfaces:**
- Consumes: `Technique::category()` (Task 1), `GameState::has_detection`, `GameState::alerts`.
- Produces: `InstanceProbe::SawCategory(Category)`, `InstanceProbe::Identified(Technique)`; `Requirement::AnyOf(Vec<Requirement>)`; helpers `Requirement::saw_category(Category)`, `Requirement::identified(Technique)`, `Requirement::any_of(Vec<Requirement>)`.

- [ ] **Step 1: Failing test** (facts.rs `#[cfg(test)]`):
```rust
#[test]
fn saw_category_fires_on_any_in_category_alert_identified_needs_a_rule() {
    use crate::category::Category;
    let mut s = base();
    // an alert for kerberoast → SawCategory(CredentialAccess) true, Identified(Kerberoast) false
    s.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "x".into(), rule_id: "r".into(), level: 5 });
    assert!(InstanceProbe::SawCategory(Category::CredentialAccess).holds(&s));
    assert!(!InstanceProbe::SawCategory(Category::Discovery).holds(&s));
    assert!(!InstanceProbe::Identified(Technique::Kerberoast).holds(&s), "alert alone is not identification");
    // a deployed technique rule → Identified true
    s.detections.push(crate::state::Detection { id: "d".into(), technique: Technique::Kerberoast,
        deployed_round: 1, technique_based: true, fidelity: "robust".into() });
    assert!(InstanceProbe::Identified(Technique::Kerberoast).holds(&s));
}

#[test]
fn any_of_is_a_disjunction() {
    use crate::category::Category;
    let mut s = base();
    let r = Requirement::any_of(vec![
        Requirement::saw_category(Category::InitialAccess),
        Requirement::saw_category(Category::LateralMovement),
    ]);
    assert!(!r.satisfied(&s));
    s.alerts.push(Alert { round: 1, technique: Technique::Pivot, source: "x".into(), rule_id: "r".into(), level: 5 });
    assert!(r.satisfied(&s), "one branch (LateralMovement via Pivot) satisfies AnyOf");
}
```

- [ ] **Step 2: Run — expect FAIL** (`no variant SawCategory` / `no fn any_of`).

- [ ] **Step 3: Implement** — add to `InstanceProbe` enum + `holds`:
```rust
/// Blue has seen ANY alert whose technique belongs to this tactic (cheap, category-level).
SawCategory(crate::category::Category),
/// Blue has a DEPLOYED technique-based rule for this exact technique (expensive, instance-level).
Identified(Technique),
```
```rust
InstanceProbe::SawCategory(c) => s.alerts.iter().any(|a| a.technique.category() == *c),
InstanceProbe::Identified(t) => s.has_detection(*t),
```
Add to `Requirement` enum: `AnyOf(Vec<Requirement>)`. Update `satisfied`:
```rust
Requirement::AnyOf(rs) => rs.iter().any(|r| r.satisfied(s)),
```
Add helpers:
```rust
pub fn saw_category(c: crate::category::Category) -> Self { Requirement::Instance { probe: InstanceProbe::SawCategory(c), want: true } }
pub fn identified(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Identified(t), want: true } }
pub fn any_of(rs: Vec<Requirement>) -> Self { Requirement::AnyOf(rs) }
```
(Note: `Requirement` currently `derive(Copy)`. `AnyOf(Vec<..>)` makes it non-`Copy` — remove `Copy` from the `Requirement` derive; keep `Clone`. `InstanceProbe` stays `Copy`. Fix any resulting move errors by cloning where `Requirement` was copied — grep `Requirement` usage in cards.rs/registry.rs; `requires()` returns owned `Vec` so callers already own them.)

- [ ] **Step 4: Run — expect PASS**.

- [ ] **Step 5: Commit**
```bash
git add wargame/src/facts.rs
git commit -m "feat(wargame): two-level detection probes (SawCategory/Identified) + AnyOf requirement"
```

---

### Task 3: Rewire Blue counters to two-level + D3FEND categories

**Files:**
- Modify: `wargame/src/cards.rs` (Blue cards' `category()` and `requires()`)

**Interfaces:**
- Consumes: Task 1 (`Category` D3FEND variants), Task 2 (`saw_category`, `identified`, `any_of`).
- Produces: no new symbols; changes card metadata + legality.

- [ ] **Step 1: Failing test** (cards.rs `#[cfg(test)]` or a new inline test) — enforce_aes now needs a deployed rule, not a bare alert:
```rust
#[test]
fn enforce_aes_requires_a_deployed_rule_not_just_an_alert() {
    let reg = default_registry();
    let mut s = GameState::new(vec![]);
    s.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "x".into(), rule_id: "r".into(), level: 5 });
    assert!(!reg.get("enforce_aes").unwrap().precondition(&s), "alert alone must NOT unlock enforce_aes");
    s.detections.push(Detection { id: "d".into(), technique: Technique::Kerberoast, deployed_round: 1, technique_based: true, fidelity: "robust".into() });
    assert!(reg.get("enforce_aes").unwrap().precondition(&s), "a deployed kerberoast rule unlocks enforce_aes");
}
```

- [ ] **Step 2: Run — expect FAIL** (alert currently unlocks it via `fingerprinted`).

- [ ] **Step 3: Implement** — edit the Blue cards:
  - `enforce_aes`: `category()` → `Category::Harden`; `requires()` → `vec![Requirement::lack(Fact::AesEnforced), Requirement::identified(Technique::Kerberoast)]`.
  - `enforce_preauth`: `category()` → `Category::Harden`; `requires()` → `vec![Requirement::lack(Fact::PreauthEnforced), Requirement::identified(Technique::AsRepRoast)]`.
  - `remediate_acl`: `category()` → `Category::Harden`; `requires()` → `vec![Requirement::lack(Fact::PathSevered), Requirement::saw_category(Category::Discovery)]` (was `have(Fact::ScoutDetected)` — equivalent today, now parameterized so future discovery tools also unlock it).
  - `segment`: `category()` → `Category::Isolate`; `requires()` → `vec![Requirement::any_of(vec![Requirement::saw_category(Category::InitialAccess), Requirement::saw_category(Category::LateralMovement)]), Requirement::lack(Fact::ReachesDc), Requirement::lack(Fact::DomainAdmin), Requirement::probe(InstanceProbe::HasForwardPath)]` (was `have(Fact::IntrusionDetected)`).
  - `rotate_creds`: `category()` → `Category::Evict` (requires unchanged).
  - `monitor`, `hunt`, `deploy_detection`, `active_response`: LEAVE as `Category::Detection` (the D3FEND Detect lane) — no change.
  - Add `use crate::category::Category;` if not present.

- [ ] **Step 4: Run — expect PASS** (`cargo test -p purple-wargame enforce_aes_requires`), plus `cargo build`.

- [ ] **Step 5: Commit**
```bash
git add wargame/src/cards.rs
git commit -m "feat(wargame): rewire blue counters to two-level detection + D3FEND categories"
```

---

### Task 4: Retire hardcoded detection facts + surface two-level in the Blue prompt

**Files:**
- Modify: `wargame/src/facts.rs` (remove `ScoutDetected`/`RoastDetected`/`IntrusionDetected` from `Fact`, `ALL`, `key`, `question`, `audience`, `holds`; adjust the `fresh_state` test loop which iterates `Fact::ALL`)
- Modify: `wargame/src/referee.rs` (`ModelAgent::ask` / `view_for`: build the Blue detection surface from `SawCategory` over attack tactics + `Identified` over performed techniques)

**Interfaces:**
- Consumes: Task 2 probes.
- Produces: prompt `engagement_facts` for Blue now includes a `detection` sub-map keyed by tactic + identified techniques. `Fact::ALL` shrinks to 11.

- [ ] **Step 1: Failing test** (facts.rs) — the three facts are gone:
```rust
#[test]
fn hardcoded_detection_facts_are_retired() {
    assert_eq!(Fact::ALL.len(), 11);
    assert!(Fact::ALL.iter().all(|f| !matches!(f.key(), "scout_detected" | "roast_detected" | "intrusion_detected")));
}
```

- [ ] **Step 2: Run — expect FAIL** (still 14).

- [ ] **Step 3: Implement facts.rs** — delete the three variants everywhere (enum, `ALL` [now `[Fact; 11]`], `key`, `question`, `audience` blue arm, `holds`). Confirm nothing else references them (`grep -rn "ScoutDetected\|RoastDetected\|IntrusionDetected" wargame/src` → only remaining hits should be gone after Task 3 rewired the cards).

- [ ] **Step 4: Implement referee.rs** — where the Blue view builds `engagement_facts`, add a `detection` object so Blue still sees its detection state (fog-safe — it's Blue's own knowledge):
```rust
// Blue-only: category-level awareness + which techniques are fingerprinted (two-level).
if view.side == Side::Blue {
    use crate::category::Category;
    let cats = [Category::InitialAccess, Category::Discovery, Category::CredentialAccess,
                Category::LateralMovement, Category::PrivilegeEscalation, Category::Exfiltration];
    let saw: serde_json::Map<String, serde_json::Value> = cats.iter()
        .map(|c| (c.key().to_string(), json!(state.alerts.iter().any(|a| a.technique.category() == *c)))).collect();
    let identified: Vec<&str> = /* techniques with a deployed rule */ ...;
    // attach `saw_category` and `identified` into the situation json
}
```
(Implementer: thread `state`/`view` as available in `ask`; if `ask` only has `view`, add the precomputed detection rows to `AgentView` in `view_for` the same way `facts` is, as a `Vec<(String,bool)>` + `Vec<String>`. Follow the existing `AgentView.facts` pattern exactly.)

- [ ] **Step 5: Run — expect PASS**; `cargo build`; existing referee/facts tests green.

- [ ] **Step 6: Commit**
```bash
git add wargame/src/facts.rs wargame/src/referee.rs
git commit -m "feat(wargame): retire hardcoded detection facts, surface two-level detection to blue"
```

---

### Task 5: Update migration oracle + taxonomy invariants

**Files:**
- Modify: `wargame/tests/precondition_equivalence.rs` (oracle for the two changed cards)
- Modify: `wargame/tests/taxonomy.rs` (D3FEND categories, `ENFORCED`)

**Interfaces:** none new — test-only.

- [ ] **Step 1: Update the oracle** in `precondition_equivalence.rs` — the sweep should still pass for every card EXCEPT the two whose semantics changed on purpose. Change those two oracle arms to the new intent, and add a header note:
```rust
// v2: enforce_aes/enforce_preauth now require a DEPLOYED rule (Identified), not a bare alert.
"enforce_aes" => !s.rc4_disabled && s.has_detection(Technique::Kerberoast),
"enforce_preauth" => !s.preauth_required && s.has_detection(Technique::AsRepRoast),
```
Leave `remediate_acl` and `segment` oracle arms unchanged — they must STILL match (proof the category mapping preserved them). If the sweep fails on those, the `Technique::category()` mapping is wrong — fix Task 1, do not weaken the oracle.

- [ ] **Step 2: Run — expect PASS** (`cargo test -p purple-wargame --test precondition_equivalence`): 800-seed sweep green.

- [ ] **Step 3: Update `taxonomy.rs`** — read it first; update any assertion that pins Blue cards to old categories to the new ones, and update `Category::ENFORCED` (in category.rs) to the categories that now hold ≥1 tool: `InitialAccess, Discovery, CredentialAccess, PrivilegeEscalation, LateralMovement, Detection, Harden, Isolate, Evict`. (`Deceive`/`Model`/`Exfiltration` and the other empty ATT&CK lanes are reserved — NOT enforced.) Assert every `ENFORCED` category has ≥1 registered tool (red techniques via `Technique::category()` + blue cards via `category()`).

- [ ] **Step 4: Run — expect PASS** (`cargo test -p purple-wargame`): full suite green.

- [ ] **Step 5: Commit**
```bash
git add wargame/tests/precondition_equivalence.rs wargame/tests/taxonomy.rs wargame/src/category.rs
git commit -m "test(wargame): update migration oracle + taxonomy invariants for v2"
```

---

### Task 6: Re-measure balance (report, not a gate)

**Files:** none (measurement). Records results in the plan + memory.

- [ ] **Step 1: Build** `cargo build --release` (faster matches) or debug.
- [ ] **Step 2: Run the 10-seed model batch** with `WARGAME_MODEL_URL=http://127.0.0.1:11434/v1/chat/completions WARGAME_MODEL=qwen2.5:7b`, seeds 1–10, `WARGAME_SEED=<n> cargo run -- cli model`, capturing winner + whether Blue played a path-cut. (Reuse the batch script pattern from the session.)
- [ ] **Step 3: Record** the new Blue win-rate and per-category tool counts as the v2 baseline in this plan's results section and in the `purple-range-wargame-engine` memory. Expect it to differ from 3/10 (enforce_aes/preauth are now 3-step). No pass/fail — the number is the deliverable.

---

## Results (filled in by Task 6)

_TBD after execution._
