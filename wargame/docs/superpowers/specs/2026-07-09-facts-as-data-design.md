# Categorical Facts-as-Data: tactic-keyed preconditions and effects

**Date:** 2026-07-09
**Status:** design, pending review (v2 — categorical vocabulary)
**Sub-project 1 of:** the co-evolution training ground (see North Star)

## North Star (why this exists)

The Purple Range wargame is being built toward one end: a reproducible **environment that
emits clean trajectories for training a cybersecurity-dedicated model**. Balance between Red
and Blue is not hand-tuned with constants; it emerges from a co-evolutionary arms race in
which both sides *grow* — Red finds a fast line, Blue learns to catch it, Red composes a new
line that routes around the catch — landing at cat-and-mouse. Growth happens through the
**node-based card builder**: new attacks/defenses composed from primitives, authored **both**
by a human (builder UI) and by a model (proposing a composition the engine validates).
Endgame: the same primitives fire for real on a lab network (`LiveEnvironment` already
dispatches real commands via `env.act`).

For any of that, a card's preconditions and effects must be **introspectable data** organized
by a vocabulary rich enough that many tools coexist per capability. That is this sub-project.

## Heilmeier pitch

- **What are you trying to do?** Make every attack and defense in the wargame describe itself
  as data — what it needs, what it achieves, what tactic it belongs to, how it can be caught —
  using ATT&CK/D3FEND categories, so many interchangeable tools can fill each category.
- **How is it done today, limits?** Each card gates itself with an opaque Rust closure, and the
  vocabulary is a flat list of *specific techniques* (Kerberoast, BloodHound…). Nothing can
  reason about a card without running it, and preconditions naming one technique cap the arsenal
  at that technique — which manufactures the exact chokepoint (Red forced through `bloodhound`)
  that makes the game one-sided.
- **What's new?** A two-tier vocabulary: **Category** (full ATT&CK tactics for attack, D3FEND
  tactics for defense) as the stable backbone, and **Tools** (cards) as many interchangeable
  instances per category. Preconditions/effects become declared data over **capability facts**
  (tool-agnostic) plus **two-level detection** (cheap category awareness vs. instance
  fingerprinting).
- **Who cares?** It's the substrate the node builder, the knowledge forest, and a future
  training loop all stand on. Without it, "load the VM with many tools so both models have a
  fighting chance" is impossible — there's no way for a second discovery tool to be a real
  substitute for the first.
- **Risks?** The migration changes some current game semantics (it is *not* representation-
  preserving), so balance shifts and must be re-measured, not asserted-identical. Parameterized
  facts add enum complexity.
- **Cost/time?** One implementation plan; a handful of tasks (Category+mapping, Fact model,
  Requirement, trait + card migration, tests/re-measure).
- **Exams?** Mid: every existing card still appears and plays, each populated category resolves,
  unit tests green. Final: a 10-seed model batch runs to completion and its win-rate + the
  per-category tool counts are reported (a new, legible baseline — expected to differ from 3/10).

## Problem

Two coupled defects in `cards.rs` / `card.rs` / `state.rs`:

1. **Opaque gating.** `fn precondition(&self, state) -> bool` (`card.rs:105`) is an arbitrary
   closure; `detection_surface` only exists as a `vec![]` returned mid-`run()`. Nothing can read
   what a card needs or does without executing it.
2. **Instance-focused vocabulary.** The `Technique` enum mixes tactics (`InitialAccess`, `Exfil`)
   with specific techniques (`Kerberoast`, `BloodHound`), and preconditions key on those specific
   names. "Map the path" can *only* mean `bloodhound`, so Blue learns one signature and Red has no
   sibling to fall back on. The arsenal cannot grow sideways.

## Design

### 1. Two tiers

**Tier 1 — `Category`** (new enum in `state.rs`). The stable backbone. Two families:

- *Attack* = the 14 ATT&CK enterprise tactics: `Reconnaissance`, `ResourceDevelopment`,
  `InitialAccess`, `Execution`, `Persistence`, `PrivilegeEscalation`, `DefenseEvasion`,
  `CredentialAccess`, `Discovery`, `LateralMovement`, `Collection`, `CommandAndControl`,
  `Exfiltration`, `Impact`. Each carries `tactic_id()` (e.g. `TA0006`) and a display name.
- *Defense* = D3FEND top-level tactics: `Harden`, `Detect`, `Isolate`, `Evict`, `Deceive`,
  `Model`. The defensive mirror, so a model trained here learns both sides' real vocabulary.

Most attack categories start with zero or one tool — expected. They are the *slots* the arsenal
grows into (sub-project 2+), not a promise to fill them now.

**Tier 2 — Tool (a card).** Every card gains `category() -> Category`. `Technique` is retained as
the **instance signature** (the specific MITRE technique a tool exposes — it already carries
`attack_id`/`data_source`) and gains `category() -> Category` mapping each to its attack tactic:

| Technique | Category |
|---|---|
| InitialAccess | InitialAccess |
| Recon | Discovery |
| Pivot | LateralMovement |
| Kerberoast | CredentialAccess |
| AsRepRoast | CredentialAccess |
| BloodHound | Discovery |
| CredSpray | CredentialAccess |
| LateralMove (DCSync) | CredentialAccess |
| Exfil | Exfiltration |

### 2. Fact model

Three kinds of fact. **Capability and posture facts are tool-agnostic** — this is what lets many
tools be interchangeable: any CredentialAccess tool that succeeds produces `HasCred`; any
Discovery tool that maps the route produces `PathMapped`. Downstream requirements never name a
tool.

- **Capability (red progress, tool-agnostic)** — keep the existing `facts.rs` capability facts
  (`Foothold`, `ReachesDc`, `HasCred`, `PathMapped`, `DomainAdmin`) and add the ones the closures
  read so preconditions need no residual predicate:

  | New fact | Predicate | Audience |
  |---|---|---|
  | `HasForwardPath` | `!next_hops().is_empty()` | Red |
  | `DomainEnumerated` | any Discovery enumeration performed (`performed_technique(Recon)`) | Red |
  | `LateralPathPlanted` | `vuln(LateralMove)` (scenario) | Red |
  | `CompromisedCredKnown` | `creds.any(cracked && blue_knows(via))` | Blue |
  | `UndetectedActivity` | `performed.any(!blue_knows)` | Blue |
  | `UndetectedAlert` | `alerts.any(!has_detection)` | Blue |

- **Posture (blue state, unchanged)** — `Monitoring`, `AutoResponse`, `PathSevered`,
  `AesEnforced`, `PreauthEnforced`.
- **Two-level detection (replaces the instance-bundle facts `scout_detected`/`roast_detected`/
  `intrusion_detected`):**
  - `SawCategory(Category)` — cheap, category-level: `alerts.any(|a| a.technique.category()==C)`.
    "Credential access is happening." This is what monitoring/baseline yields.
  - `Identified(Technique)` — expensive, instance-level: `has_detection(t)` (a deployed
    technique-based rule fingerprinting the exact tool). This is what unlocks a *precise* counter.

`Fact` becomes an enum with unit variants (capability + posture) and two **data-carrying**
variants `SawCategory(Category)` and `Identified(Technique)`. `key()` renders params
(`"saw:credential_access"`, `"identified:kerberoast"`). `audience()` stays: capability = Red,
posture + detection = Blue. `table_for` expands the parameterized facts over the categories /
techniques in play for the surfaced prompt table.

### 3. Requirement type

```
pub enum Requirement {
    Is(Fact, bool),                 // fact must hold == bool  (want=false ⇒ must be false)
    AnyOf(Vec<(Fact, bool)>),       // at least one member matches (disjunction, see §6)
}
```

`satisfied(state)`: `Is(f,want)` ⇒ `f.holds(state)==want`; `AnyOf(v)` ⇒ any `(f,want)` matches.
A card is legal when **all** its requirements are satisfied. Constructors `Requirement::yes(f)` /
`Requirement::no(f)` build the common `Is` case; `Requirement::any_of([...])` builds `AnyOf`.

**Legality uses ground truth, independent of `audience()`.** The referee already evaluates
preconditions against full state; `audience()` only filters the surfaced prompt table. A Blue
card may require a Red-audience capability fact — the requirement gates the menu via ground
truth exactly as the current closures do, while the fact still never leaks into a surfaced table.

### 4. Trait shape

`Card` gains `category()`, `requires() -> Vec<Requirement>` (default `vec![]`), `produces() ->
Vec<Fact>` (default `vec![]`), `detection_surface() -> Vec<Technique>` (default `vec![]`), and
turns `precondition` into a **provided** method: `self.requires().iter().all(|r|
r.satisfied(state))`. Cards delete their `precondition` overrides and now-redundant inline gating.
`CompositeCard` (`graph.rs`) carries `requires: Vec<Requirement>` + `category` fields instead of
the `precond: fn` pointer. Keep runtime `detection_surface` emission for scoring; add a
debug-assert that declared ⊇ runtime-emitted so they cannot drift.

### 5. Card migration (the equivalence table — semantics DELIBERATELY change where noted)

| Card | Category | `requires()` | `produces()` |
|---|---|---|---|
| initial_access | InitialAccess | no(Foothold), yes(HasForwardPath) | Foothold |
| pivot | LateralMovement | yes(Foothold), no(ReachesDc), yes(HasForwardPath) | (position; ReachesDc when objective-adjacent) |
| recon | Discovery | yes(ReachesDc), no(DomainEnumerated) | DomainEnumerated |
| kerberoast | CredentialAccess | yes(ReachesDc) | HasCred |
| asrep_roast | CredentialAccess | yes(ReachesDc) | HasCred |
| bloodhound | Discovery | yes(HasCred), no(PathMapped) | PathMapped |
| escalate_da | CredentialAccess | yes(LateralPathPlanted), yes(HasCred), yes(PathMapped), no(DomainAdmin), no(PathSevered) | DomainAdmin |
| monitor | Detect | no(Monitoring) | Monitoring |
| active_response | Evict | no(AutoResponse) | AutoResponse |
| remediate_acl | Harden | no(PathSevered), yes(SawCategory(Discovery)) | PathSevered |
| enforce_aes | Harden | no(AesEnforced), yes(Identified(Kerberoast)) | AesEnforced |
| enforce_preauth | Harden | no(PreauthEnforced), yes(Identified(AsRepRoast)) | PreauthEnforced |
| rotate_creds | Evict | yes(CompromisedCredKnown) | (voids cred) |
| hunt | Detect | yes(UndetectedActivity) | alert ⇒ SawCategory(t.category) — NOT Identified |
| deploy_detection | Detect | yes(UndetectedAlert) | Identified(t) (deploys the technique rule) |
| segment | Isolate | yes(SawCategory(InitialAccess)) OR yes(SawCategory(LateralMovement)), no(ReachesDc), no(DomainAdmin), yes(HasForwardPath) | (severs edges) |

Deliberate semantic changes vs. the closures (these are the *point*, not regressions):
- `remediate_acl` now unlocks on **any Discovery** detection (`SawCategory(Discovery)`), not
  specifically a recon/bloodhound alert — so a future second discovery tool also unlocks it.
- `enforce_aes`/`enforce_preauth` now require **instance identification** (`Identified(...)`),
  the deployed-rule fingerprint, not a bare alert — the two-level counter. Concretely this makes
  them a **three-step** play (observe → `deploy_detection(t)` for `Identified` → enforce), a
  deliberate difficulty increase that is exactly where the arms race lives: Red swaps cred tools
  faster than Blue fingerprints them. Expect this to move the win-rate; that is the point.
- `segment`'s OR over two `SawCategory` requirements uses `Requirement::AnyOf` (see §3/§6).
- The old instance-bundle facts `scout_detected`/`roast_detected`/`intrusion_detected` are
  **removed** from `facts.rs`; the referee prompt wiring (`AgentView.facts`, both `view_for`
  arms, the `engagement_facts` map) switches to the `SawCategory`/`Identified` table.

`vuln` checks *inside* `play()`/`run()` (kerberoast's `CrackHash`, asrep's body) stay put — they
are runtime failures, not legality gates. Only `escalate_da` gated on `vuln` pre-legality, via
`LateralPathPlanted`.

### 6. Disjunction

`segment` is the only card needing OR (breach *or* pivot detected). Handled by the
`Requirement::AnyOf` variant defined in §3 — a card's `requires()` is an AND over its
`Requirement`s, and an `AnyOf` requirement is satisfied when any of its members matches. No extra
type; everything stays declarative and data-inspectable (the builder/forest read `AnyOf` too).

## Testing

The old "identical legal menu" characterization test is **removed** — it would enshrine the
instance-focused vocabulary. Replacements:

1. **Fact unit tests** for every new/changed fact (capability transitions; `SawCategory` fires on
   any in-category alert; `Identified` needs a deployed rule not a bare alert; `audience()`
   partition; parameterized `key()` rendering).
2. **Category mapping test** — every `Technique` maps to an attack `Category`; round-trips.
3. **Per-card declaration test** — each card's `category`/`requires`/`produces`/`detection_surface`
   matches §5; the debug-assert (declared ⊇ runtime) holds when each card plays.
4. **Playability invariant** — for each scenario, every registered card is reachable: there exists
   a state sequence in which it becomes legal (no card is dead on arrival), and each populated
   category has ≥1 legal tool along a normal kill chain.
5. **Full existing suite green.**
6. **Balance re-measure (not a gate, a report):** the 10-seed model batch runs to completion;
   record the new win-rate and per-category tool counts as the fresh baseline. It is expected to
   differ from 3/10; the number is the deliverable, not a pass/fail.

## Global constraints

- Sim only. No `LiveEnvironment` changes, no RB3011, no live runs.
- No Ouros wiring of any kind.
- Follow existing `facts.rs` / `cards.rs` patterns; no unrelated refactoring.
- Balance is **re-measured, not preserved** — this is an intentional semantic change.

## Non-goals (each its own later spec)

Populating the arsenal (many tools per category), the knowledge forest, cross-match persistence,
actuation (opening book / menu shaping / builder-driven growth), the builder authoring API,
model training.
