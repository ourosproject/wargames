# Arsenal primitives — genuinely-different tools + compound win conditions

- **Status:** design, pending user review
- **Date:** 2026-07-10
- **Sub-project:** the primitive alphabet the node-chain builder will compose over (built FIRST, before the canvas)
- **Depends on:** arsenal-as-data (merged to `main`) — the effect/fact/technique alphabet this extends
- **Crate:** `purple-wargame`

---

## Heilmeier pitch (read this first)

**What are we building?**
Right now every attack ends the same way — race to Domain Admin — and the sim can only mechanically
do a handful of things (get a foothold, move, steal a credential, flip a defense, cut the network,
detect). That's too narrow to build *genuinely different* tools. This work grows the engine's small
set of building blocks so you can compose real variety — break in by phishing vs exploit, steal a
credential vs drop a virus, persist, exfiltrate data, detonate ransomware — and, crucially, lets a
**win be a combination of goals achieved together** (e.g. *steal creds + deploy a virus + stay
undetected*), not just "reach DA."

**How's it done today, and what's the limit?**
The one way to win is a hardcoded boolean (`red_reached_da`). "Objective" appears in the scoring
comments but only DA exists. So every tool is a variation on the same race, and there's no mechanic
for persistence, exfil, or impact. The builder we want can only ever be as expressive as this
alphabet — so the alphabet has to grow first.

**What's new, and why will it work?**
One structural move unlocks it: make **winning a data-defined combination of conditions**, written in
the *same facts-and-conditions language that already gates every move*. A win condition is just "all
of these must be true." DA becomes one such condition; `creds + virus + undetected` is another. Then
a few new building blocks (an objective dimension, red-side posture like persistence/C2, one missing
blue effect — *evict* — and a richer set of detectable techniques) compose into genuinely different
tools. It works because it reuses machinery that already exists (the requirement/condition checker)
and every new attack primitive ships with its defensive counter, so the game stays a contest.

**Who cares?**
This is the foundation the node-chain builder stands on. Without a rich primitive set, the builder can
only make reskins. With it, a person (and later the model) can compose attacks and defenses that are
actually new — which is the whole co-evolution point of the project.

**What are the risks (and how we handle them)?**
Main risk: reshaping the game breaks the balance we guard. Handled by making the defaults behaviorally
identical to today's game (default win condition = `[reach DA]`), so the 16 built-ins and the 3/10
balance are unchanged; all new primitives are opt-in. Second risk: a multi-path game is harder to
balance — handled by shipping each attack primitive with its counter and re-*measuring* balance (it
was never hand-tuned).

**When is it done? (the exam)**
The engine supports: multiple, compound, data-defined win conditions (red wins on any satisfied
combination); an objective dimension (DA, data-exfiltrated, impact) plus red posture (persistence,
C2); an `Evict` blue effect; and an expanded technique list for break-in / cred-theft / malware /
C2 / exfil / ransomware. A compound win (e.g. `creds + virus + undetected`) can be expressed as data
and actually triggers a red win in a match. With default rules, the 16 built-ins play identically
(goldens green) and balance is still 3/10.

---

## Plain-language overview

Two things change, and the first makes the second click into place.

**1. Winning becomes a combination of conditions — reusing the language moves already speak.**
Every move already declares *when it's legal* as a list of conditions over the game's facts ("has a
foothold," "reached the DC," "AES is enforced"). We make **victory** speak that same language: a win
condition is a named list of facts that must all be true at once. Red wins if **any** win condition is
fully met.
- The current game is just one win condition: **`{reached Domain Admin}`** — so nothing changes by
  default.
- Your idea becomes a second one: **`{holds a credential, virus deployed, undetected}`**.
- A ransomware path: **`{has a foothold, persistence, impact detonated}`**.

The same checker that decides "is this move legal" now also decides "has red won" — one mechanism, two
uses.

**2. A few new building blocks let tools reach those new conditions.**
- **Objectives** as a real thing (not just DA): *data exfiltrated*, *impact detonated*. Reaching one
  is a fact a win condition can require.
- **Red posture**, mirroring blue's: *persistence* and *C2* — switches red can flip, the way blue flips
  its defenses. Persistence is what makes an implant/virus meaningful: it survives blue's cleanup.
- **Evict** — the one genuinely missing effect: blue can cut the network, sever the DA path, rotate
  creds, and harden, but it has no way to *kick red out* or *clear persistence*. `Evict` completes the
  balance (red **breaks in / persists** ↔ blue **evicts**).
- **More techniques** — the detectable identities. Break-in as *phishing* / *exploit* / *stolen creds*;
  cred theft as kerberoast / AS-REP / *LSASS dump*; plus *malware*, *C2*, *persistence*, *exfil*,
  *ransomware*. Two tools that both "get a foothold" now read and play as different attacks because Blue
  sees different things.

Everything else composes from what already exists (`Advance`, `GrantCred`, `SetFlag`, the detection
ops). Net new mechanics: **the compound win checker, an objective dimension, a couple red posture
switches, one blue effect (Evict), and a batch of techniques.** A small alphabet; the richness comes
from combining it.

---

## The structural move: win conditions as data (the read-half alphabet, reused)

Today (`rules.rs`): `red_wins_on_da: bool`, checked in the referee as `red_wins_on_da && red_reached_da`.

Replace that single special case with a general, data-defined set — reusing the existing `Requirement`
type (the same one gates and guards use, including `AnyOf`):

```rust
/// A way for red to win: red wins the moment EVERY requirement here holds at once.
struct WinCondition { name: String, all_of: Vec<Requirement> }

// in RuleSet, replacing `red_wins_on_da: bool`:
red_win_conditions: Vec<WinCondition>,   // red wins if ANY condition is fully satisfied
```

- **Default** (behaviorally identical to today): `[WinCondition { name: "domain_admin", all_of: [Requirement::have(Fact::DomainAdmin)] }]`. `Fact::DomainAdmin` already reads `red_reached_da`, so the win check is the same event — the current game is untouched.
- **Compound example:** `WinCondition { name: "silent_heist", all_of: [have(HasCred), have(DataExfiltrated), have(Undetected)] }`.
- The referee's win check becomes: after red's phase, if any `WinCondition.all_of.iter().all(|r| r.satisfied(state))`, red wins (report the condition's `name`). Blue still wins on the round-cap timeout.

This is the correspondence — *as above, so below*: the fact/condition language that decides move
legality now also decides victory. No new checker; `Requirement::satisfied` already exists.

`WinCondition` is serializable and lives in `RuleSet`, which is already "rules as data, user-editable
via the node-builder" — so **win conditions themselves become authorable** later (the same folder-as-
truth idea), not baked in.

---

## The primitive expansion (the alphabet)

Organized by the state dimension each touches. New parts are marked **NEW**; everything else is reuse.

### Objectives (new dimension — generalizes the DA special case)
- **State:** add `data_exfiltrated: bool`, `impact_done: bool` to `GameState` (alongside `red_reached_da`). These are red's *goals reached*.
- **Facts (surfaced, red-audience):** `DataExfiltrated`, `ImpactDone` (+ keep `DomainAdmin`). A win condition or a downstream gate references them.
- **Effects:** setting an objective is just `SetFlag` on the new booleans — **no new effect needed** (an objective reached is a posture bit). An `Exfil`/`Impact` *tool* is a move whose effect flips the bit, gated on the prerequisites (e.g. exfil needs a cred + staged data).

### Red posture (new — mirrors blue posture)
- **State:** `red_persisted: bool`, `c2_established: bool`.
- **Facts (red-audience):** `Persisted`, `C2`.
- **Effects:** `SetFlag` (reused) — new `StateFlag` variants `Persisted`, `C2Established` (and the objective bits above). **No new effect.**
- **Balance mechanic:** persistence *resists* blue eviction and can *re-establish* a foothold blue cut — the arms-race counterpart to Evict.

### Stealth (new fact — enables "undetected" win terms)
- **Fact / probe:** `Undetected` — red has done ≥1 objective-relevant technique and Blue holds no alert for it (composed from existing `alerts`/`blue_knows`; no new state). This is what a `+ undetected` win term reads.

### Position — the missing blue counter
- **Effect `Evict` (NEW, blue):** remove a red-held zone (and, with enough force, clear `red_persisted`). Completes the position dimension: `Advance` adds red position, `Evict` removes it. Persistence makes eviction only partial unless paired with a rule (the balance knob).

### Technique alphabet (grow the detection identities)
Add to the `Technique` enum (each carries its detection data-source, ATT&CK id, value — the existing metadata pattern):
- **Break-in variants:** `Phishing`, `ExploitPublicApp`, `ValidAccounts`.
- **Cred variant:** `LsassDump` (kerberoast/asrep/credspray/DCSync already exist).
- **New families:** `Malware` (execution), `C2`, `Persistence`, `Ransomware` (impact). `Exfil` already exists.

These are the "variable options" inside a node: a *break-in* node offers phishing/exploit/valid-accounts;
each is a distinct thing Blue can (or can't) detect.

### Categories
Already present (v2 widened to full ATT&CK + D3FEND): `Execution`, `Persistence`, `CommandAndControl`,
`Exfiltration`, `Impact` are ready lanes. No taxonomy change needed.

---

## Balance — every attack primitive ships with its counter

Stability from opposing forces, not from special-cases. Each new red capability has a blue answer,
mostly via the **existing guard mechanic** (a red effect fails when a blue defense is up, exactly like
kerberoast fails vs AES) plus the one new `Evict`:

| Red primitive | Blue counter | Mechanism |
|---|---|---|
| Break in / pivot | **Evict** (NEW) | remove red's zone |
| Persistence | **Evict** + a persistence-detection rule | eviction clears `red_persisted` only if detected |
| Exfil (objective) | **Block egress / DLP** (a blue posture flag) | red's exfil effect *guards* on `!egress_blocked` |
| Impact / ransomware | **Backups** (a blue posture flag) | red's impact effect *guards* on `!backups_ready` |
| C2 | **Block C2** (a blue posture flag) | red's C2-dependent effects guard on `!c2_blocked` |
| Malware / new techniques | detect + respond | the existing detection/hunt/deploy loop, now over more techniques |

So the new blue defenses are mostly new `StateFlag` switches (`EgressBlocked`, `BackupsReady`,
`C2Blocked`) set by `SetFlag` — plus `Evict`. The red effects that pursue objectives carry guards on
those flags. No sprawl of bespoke effects.

---

## Compatibility — the current game is untouched by default

- **Default `RuleSet`** win condition = `[reach DA]` → the referee win event is identical to today.
- New state booleans default `false`; new facts/effects/techniques are **only** used by tools that opt
  into them. The 16 built-in moves reference none of them, so their `play()` is unchanged → the frozen
  **golden fixtures stay green**.
- **Balance re-measured** with default rules: still **3/10** on seeds {1,3,7} (deterministic heuristic).
  A deviation is a real regression to investigate.
- `Fact::ALL` grows (new red-objective/posture facts) — that's intentional (we're expanding the game,
  not doing a pure refactor), and it only *adds* to the surfaced table; it doesn't change existing facts.

---

## Out of scope (deliberately, next phases)

- **The node-chain canvas** itself — this spec builds the *primitives*; the canvas that composes them
  is the next sub-project (and it reuses the builder backend already written on the `move-builder`
  branch, swapping the scrapped form for the canvas).
- **Authoring win conditions from the UI** — `WinCondition` is data and serializable so it *can* be
  authored later; this phase only makes the engine consume it.
- **A full content library** of new tools — this phase ships the primitives plus a *few* built-in tools
  that exercise each new mechanic (proof they compose); a rich arsenal is authored on the canvas later.

---

## Testing

- **Win-condition checker unit tests:** a single-fact condition (DA) wins exactly as before; a compound
  condition (`creds + virus + undetected`) wins only when all three hold and not before; multiple
  conditions → red wins on whichever is satisfied first; empty/unsatisfied → no win.
- **New primitives unit tests:** `Evict` removes a red zone / clears persistence; each new
  `StateFlag`/objective flips its bit; the new red effects guard correctly on their blue counters
  (exfil fails vs egress-block, impact fails vs backups, like kerberoast vs AES).
- **Compatibility:** default `RuleSet` reproduces the DA win event; the golden fixtures for the 16
  built-ins stay green; `default_registry` unchanged.
- **A few new built-in tools** (one per new family) each get the arsenal-equivalence/golden treatment,
  and at least one compound-win path is proven end-to-end in a scripted match.
- **Balance:** deterministic-heuristic 3/10 on {1,3,7} with default rules; then a *documented* measurement
  of a sample multi-path ruleset (not a guard, just a recorded data point).

---

## Decisions locked (from the conversation)

New objectives are **win conditions, not just score**. Winning can require a **combination** of
conditions held together (compound wins), expressed in the existing `Requirement` alphabet. Defaults
keep today's game (DA-only) so built-ins + 3/10 balance are unchanged. Build the **primitives first**
(this spec), then the node-chain canvas composes over them. Each new attack primitive ships with its
defensive counter.
