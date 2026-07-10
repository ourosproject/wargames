# Facts-as-Data: a two-tier categorical alphabet for card preconditions and effects

**Date:** 2026-07-09 (rev 2 — categorical vocabulary)
**Status:** design, pending review
**Sub-project 1 of:** the co-evolution training ground (see North Star)
**Supersedes:** rev 1 (instance-named facts + "identical legal menu" characterization
test). Rev 1 was rejected — see *Why rev 1 was wrong* below.

## North Star (why this exists)

The Purple Range wargame is being built toward one end: a reproducible **environment
that emits clean trajectories for training a cybersecurity-dedicated model**, running on
the local homelab range over ethernet. Balance between Red and Blue is not hand-tuned with
constants; it **emerges from a co-evolutionary arms race** in which both sides *grow* —
Red finds a fast line, Blue learns to catch it, Red composes a new line that routes around
the catch, landing at cat-and-mouse symbiosis. The medium of that growth is a **node-based
card builder**: new attacks/defenses composed from primitives, authored **both** by a
human (builder UI) and by a model (proposing a composition the engine validates). The same
primitives eventually fire for real on the lab net (`LiveEnvironment` already dispatches
real commands).

For any of that — model-proposed cards, forest reasoning about which capability counters
which, chokepoint detection, a clean action space for a learner — a card's preconditions
and effects must be **introspectable data over a vocabulary that does not cap the arsenal**.
That is this sub-project, and only this sub-project.

**Explicitly future, not in this spec:** the knowledge forest, cross-match persistence, the
actuation channels (opening book / menu shaping / arsenal growth), the builder's authoring
API, and any training harness. This spec builds the substrate they stand on.

## Why rev 1 was wrong (the vocabulary trap)

Rev 1 named facts after specific tools: `KerberoastSeen`, `AsRepSeen`, `ReconDone`,
`PathMapped == BloodHoundDone`. That bakes the *instance* into the *primitive*. Two failures
follow:

1. **It caps the arsenal at the named tools.** Adding a new Discovery tool means adding a
   new fact and threading it through every precondition — growth by adding parts, not by
   composing what exists. The whole north star is arsenal growth; a vocabulary that fights
   growth is disqualifying.
2. **It manufactures the BloodHound chokepoint.** If `escalate_da` literally requires
   `BloodHoundDone`, then BloodHound is the *only* way to unlock privilege escalation, by
   definition — the chokepoint is an artifact of the vocabulary, not of the domain. Red can
   never route around it because the requirement names the tool, not the capability.

Rev 1's centerpiece test — "the legal menu is *identical* to today's for every state" — is
therefore the **wrong safety net**: it would enshrine exactly the tool-bound closures we
need to dissolve, and it forbids the arsenal from ever growing (any new sibling tool changes
some menu). We replace it (see Testing).

## The alphabet — two tiers

The system needs a **small fixed alphabet** whose *composition* yields an unbounded arsenal.
Two tiers, orthogonal by construction:

### Tier 1 — Category (the kill-chain topology)

A `Category` is an ATT&CK *tactic* — a stage of the engagement. This is the fixed backbone;
the *topology* every tool and fact keys on. Ordered, and this order **is** the kill chain:

```
InitialAccess → Discovery → CredentialAccess → PrivilegeEscalation → LateralMovement → Exfiltration
```

plus one cross-cutting Blue lane, `Detection` (monitoring/hunting infrastructure that is not
tied to a single attack stage). `DefenseEvasion` and `Exfiltration` are reserved slots in
the topology with **no tool yet** — they are named growth points, not enforced categories.

**Invariant: every enforced category holds ≥1 tool.** The enforced set for the current
arsenal is `{InitialAccess, Discovery, CredentialAccess, PrivilegeEscalation,
LateralMovement, Detection}`. "Load the VM with as many tools as possible, sorted by
category" is the direct consequence: siblings within a category are how Red routes around a
detected tool.

### Tier 2 — Tool (a card)

A `Tool` (every existing `Card`) **belongs to one Category** and is a pure composition over
the alphabet:

- `category() -> Category` — its kill-chain stage (Red) or the attacker category it counters
  / the `Detection` lane (Blue).
- `requires() -> Vec<Requirement>` — legality, in **category / capability** terms (Tier 1)
  with instance gates only where the domain truly demands one (Tier 2 — see two-level).
- `produces() -> Vec<Fact>` — the capability facts it flips true on success.
- `detection_surface() -> Vec<Technique>` — the **instance-level** ATT&CK exposure (the
  specific tool's signature), lifted from a runtime `vec![]` to declared metadata.

Adding a tool is registering one of these. No new fact, no new category — composition, not a
new primitive. That is the whole point.

## Facts — capabilities, never tools

A `Fact` names a **capability or state achieved**, tool-agnostic, so that *any* tool in the
relevant category can produce it. The current `facts.rs` alphabet is already mostly at this
altitude (`Foothold`, `Scouted`, `HasCred`, `ReachesDc`, `DomainAdmin`, `Monitoring`,
`PathSevered`, …). We keep those and add the capability facts the closures still read
inline. The one rename of intent: `PathMapped` stays the name (it is a *capability* — "the
concrete DA route is known") but its definition is decoupled from BloodHound-the-tool: it is
whatever the domain-graph-mapping tools in Discovery `produce`. Today that is only
BloodHound; tomorrow a sibling can produce it too.

**New capability facts (Tier 1, each a pure predicate over `GameState`, with `audience()`):**

| Fact | Predicate (today) | Audience | Consumed by |
|---|---|---|---|
| `HasForwardPath` | `!next_hops().is_empty()` | Red | initial_access, pivot, segment |
| `LateralPathPlanted` | `vuln(LateralMove)` present | Red | escalate_da |
| `UndetectedActivity` | `performed.any(!blue_knows)` | Blue | hunt |
| `UndetectedAlert` | `alerts.any(!has_detection)` | Blue | deploy_detection |

The **category-level observation** facts already exist and stay as-is — they are Blue's cheap
awareness, deliberately coarse: `ScoutDetected` (Discovery detected), `RoastDetected`
(CredentialAccess roasting detected), `IntrusionDetected` (InitialAccess/LateralMovement
detected). These are Tier-1 by design: monitoring tells you *a category is active*, not
*which tool*.

**Removed from rev 1:** `KerberoastSeen`, `AsRepSeen`, `ReconDone`. These were instance-named
facts; they are demoted into Tier-2 `Requirement::Instance` probes (below), so the Fact
alphabet stays fixed and categorical.

## The two-level decision — kept, and made literal in `Requirement`

**Decision: keep two-level granularity.** Blue's *awareness* is category-level (cheap, from
monitoring); Blue's *precise counter* is instance-level (AES enforcement neutralizes
RC4-Kerberoast specifically, not AS-REP). Detecting the category is **not** enough to deploy
the exact counter — Blue must fingerprint the specific tool. That gap is where the arms race
lives: Red swaps a sibling tool to dodge the fingerprint; Blue broadens coverage to
re-acquire it. Flattening counters to category would let one detection auto-counter an entire
category and collapse the race. (This is the design's balance/equilibrium — retain it.)

The two tiers become the **two constructors of `Requirement`** — instance-specificity lives
in a *parameter*, never a new fact:

```rust
pub enum Requirement {
    /// Tier 1 — category / capability progress. Coarse, tool-agnostic.
    Category { fact: Fact, want: bool },
    /// Tier 2 — a specific tool, by parameter. The only place an instance appears.
    Instance { probe: InstanceProbe, want: bool },
}

pub enum InstanceProbe {
    Performed(Technique),  // Red once-only guards (recon, bloodhound can't repeat)
    Detected(Technique),   // Blue has fingerprinted this specific tool
    CredCompromiseFingerprinted, // a cracked cred whose `via` technique Blue has detected
}
```

Ergonomic constructors: `Requirement::have(Fact)`, `Requirement::lack(Fact)`,
`Requirement::did(Technique)`, `Requirement::not_yet(Technique)`,
`Requirement::fingerprinted(Technique)`. A `Requirement` is satisfied when its probe
evaluates to `want`; a card's `requires()` is satisfied when all are.

**Legality uses ground truth, independent of `audience()`.** The referee evaluates
`requires()` against full state (`registry.legal`); `audience()` only filters the agent's
*prompt table* (`facts::table_for`). So a Blue card may legitimately require a Red-audience
fact (e.g. `segment` requires `HasForwardPath`): it gates the menu via ground truth exactly
as `next_hops()` does today, while the fact never leaks into either side's surfaced table.
Keep the two concerns separate.

## Card migration table (the equivalence oracle)

Category assignment and `requires()` for the current arsenal. `C=` Category-requirement,
`I=` Instance-requirement. This table IS the implementer's spec; the migration-equivalence
test (Testing §1) proves each row preserves today's legality.

| Card | Category | `requires()` | `produces()` | `detection_surface()` |
|---|---|---|---|---|
| initial_access | InitialAccess | C lack(Foothold), C have(HasForwardPath) | Foothold | [InitialAccess] |
| pivot | LateralMovement | C have(Foothold), C lack(ReachesDc), C have(HasForwardPath) | (position; ReachesDc at objective) | [Pivot] |
| recon | Discovery | C have(ReachesDc), I not_yet(Recon) | Scouted | [Recon] |
| bloodhound | Discovery | C have(HasCred), I not_yet(BloodHound) | PathMapped (+Scouted) | [BloodHound] |
| kerberoast | CredentialAccess | C have(ReachesDc) | HasCred (on crack) | [Kerberoast] |
| asrep_roast | CredentialAccess | C have(ReachesDc) | HasCred (on crack) | [AsRepRoast] |
| escalate_da | PrivilegeEscalation | C have(LateralPathPlanted), C have(HasCred), C have(PathMapped), C lack(DomainAdmin), C lack(PathSevered) | DomainAdmin | [LateralMove] |
| monitor | Detection | C lack(Monitoring) | Monitoring | [] |
| active_response | Detection | C lack(AutoResponse) | AutoResponse | [] |
| hunt | Detection | C have(UndetectedActivity) | (raises observations) | [] |
| deploy_detection | Detection | C have(UndetectedAlert) | (detection coverage) | [] |
| remediate_acl | PrivilegeEscalation (counter) | C lack(PathSevered), C have(ScoutDetected) | PathSevered | [] |
| enforce_aes | CredentialAccess (counter) | C lack(AesEnforced), I fingerprinted(Kerberoast) | AesEnforced | [] |
| enforce_preauth | CredentialAccess (counter) | C lack(PreauthEnforced), I fingerprinted(AsRepRoast) | PreauthEnforced | [] |
| rotate_creds | CredentialAccess (counter) | I have(CredCompromiseFingerprinted) | (invalidates cred) | [] |
| segment | LateralMovement (counter) | C have(IntrusionDetected), C lack(ReachesDc), C lack(DomainAdmin), C have(HasForwardPath) | (removes forward edges) | [] |

Notes:
- **`remediate_acl` keys on the *category* observation `ScoutDetected`** — the coarse
  Discovery-detected gate. This is deliberately Tier-1: any discovery tripping monitoring
  unlocks the ACL cut. It is the decisive Blue path-cut and its detection-gating behavior is
  unchanged.
- **`enforce_aes` / `enforce_preauth` key on *instance* fingerprints** — the two-level point
  made concrete. Category awareness (`RoastDetected`) is not enough; Blue must have
  fingerprinted the specific roast to deploy the matching counter.
- The `vuln(...)` checks *inside* `play()`/`run()` (kerberoast's CrackHash, asrep's body)
  stay exactly where they are — runtime failures, not legality gates. Only `escalate_da`
  gated on a `vuln` pre-legality; that becomes the `LateralPathPlanted` capability fact.
- `produces()` is not yet *consumed* by anything this sub-project ships (it feeds the future
  forest/builder), so it is verified by unit assertion, not by behavior.

## Trait / type changes

- `Card` gains `category() -> Category` (required — no default; forces every tool to declare
  its stage), `requires() -> Vec<Requirement>` (default `vec![]`), `produces() -> Vec<Fact>`
  (default `vec![]`), and `detection_surface() -> Vec<Technique>` (default `vec![]`).
- `precondition` becomes a **provided** method: `self.requires().iter().all(|r|
  r.satisfied(state))`. Cards delete their `precondition` override and now-redundant inline
  logic.
- `CardSpec` gains `category` and (for the builder/dashboard) the declared `requires` /
  `produces` / `detection_surface`, so the menu is introspectable without executing a card.
- The flat `Card::technique()` that today returns a placeholder for Blue cards (`monitor →
  Recon`, `deploy_detection → Kerberoast`) is **replaced** by `category()` for taxonomy;
  `technique()`/`detection_surface()` retain the *instance* signature for scoring only.
- `CompositeCard` (`graph.rs`) carries `requires: Vec<Requirement>` as a field instead of the
  `precond: fn` pointer — the data-defined node builder's substrate.
- Keep the runtime `detection_surface` emission in `play()`/`run()` as the scoring authority;
  add a debug-assert that declared ⊇ runtime-emitted so the two cannot drift.

## Testing (replaces the rev-1 characterization test)

The rev-1 "identical global menu forever" net is gone (it enshrines the tool-bound vocab and
forbids arsenal growth). Replace with:

1. **Migration-equivalence (one-time, existing cards only).** For each registered scenario,
   generate N≥500 seeded `GameState`s spanning the reachable space (zones, creds, performed
   techniques, alerts, detections, posture flags). For every *current* card, assert
   `fact_precondition(state) == legacy_precondition(state)`, with the legacy closures kept in
   the test module as the oracle. This proves the *refactor* preserved each existing tool's
   legality. It is **not** a standing invariant — adding a sibling tool later may legitimately
   change a menu; that is the feature.
2. **Structural invariants (standing).** Every card declares a `category()`; every enforced
   category holds ≥1 tool; the category order is the kill-chain topology; each Blue counter
   names the attacker category it counters (or `Detection`).
3. **Fact unit tests** for each new capability fact (true/false at the right transitions),
   plus `InstanceProbe` unit tests, mirroring the existing `facts.rs` tests, including the
   fog-of-war `audience()` partition.
4. **`produces()` / `detection_surface()` unit assertions** per card against the table above.
5. **Full existing suite** stays green.
6. **Balance re-measured, not asserted-identical.** Run the 10-seed model-Blue batch
   (qwen2.5:7b, localhost). The refactor adds no new tools, so the expectation is **3/10
   (seeds 1,3,7)** and BLUE-wins-iff-it-plays-a-path-cut. A deviation is **investigated and
   explained** (which re-categorization moved which menu), not silently accepted and not an
   automatic fail. The detection-gate ceiling is expected to be unchanged.

## Global constraints

- **Sim only.** No `LiveEnvironment` changes, no RB3011, no live runs.
- **No Ouros wiring** of any kind (Ouros holes are being fixed in a separate session).
- Additive/representation change for the *existing* arsenal: the migration-equivalence test
  and the re-measured 3/10 balance are the gates. New categories/tools are out of scope here.
- Follow existing `facts.rs` / `cards.rs` patterns; no unrelated refactoring.
- Git baseline: `wargame/` is currently untracked on `main`. SDD needs commits — decide
  branch+baseline vs nested repo vs no-git **before** dispatching implementation.

## Non-goals

Knowledge forest, cross-match persistence, actuation (opening book / menu shaping / arsenal
growth), the builder authoring API, model training, and any *new* tools/categories beyond
re-categorizing the current arsenal. Each is its own later spec.
