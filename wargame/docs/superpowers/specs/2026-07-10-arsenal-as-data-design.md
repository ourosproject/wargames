# Moves as data (arsenal-as-data) — design

- **Status:** design, pending user review
- **Date:** 2026-07-10
- **Sub-project:** 2 of the node-based card-builder direction
- **Depends on:** sub-project 1 (facts-as-data), merged to `main`
- **Crate:** `purple-wargame` (`~/Developer/production/purple-range/wargame`)

---

## Heilmeier pitch (read this first)

**What are we building?**
Right now every attack and defense move in the wargame is written as Rust code. Adding a
new move means a programmer edits code and rebuilds. We want a move to be just a small text
file that says three things: what has to be true before you can play it, what it does, and
what it leaves behind. The game reads those files and plays them. No programming to add a move.

**How's it done today, and what's the limit?**
The 16 moves live in code (`cards.rs`). Only a programmer can add one. That means the game
can't grow on its own — a person at a simple builder can't invent a move, and neither can the
AI. The game is frozen at whatever a programmer last typed.

**What's new, and why will it work?**
The earlier sub-project already turned the "what must be true to play this" half into data.
This one does the other half — "what the move actually does" — as a small fixed set of effects
(move forward, steal a credential, switch a defense on, cut the network, and a few more). A
move becomes a little file that snaps those pieces together. It works because we don't change
any behavior: before we delete the old code, we prove each converted move plays exactly the
same, and we re-check that the win rate hasn't moved.

**Who cares?**
This is the piece that lets the game grow itself. Once moves are data, both a person and the
AI can propose new attacks and defenses and the engine just checks and runs them. That's the
actual goal of the whole project — a range that keeps inventing new back-and-forth and produces
clean data to train a security model on.

**What are the risks (and how we handle them)?**
Main risk: accidentally changing how the game plays during the conversion, which would show up
as the win rate moving. We guard against it with a test that plays each converted move side by
side with the old code and checks they behave identically. Smaller risk: a couple of moves
(cutting the network, threat-hunting) are awkward to express as data; if so we keep them
faithful with a slightly chunkier single effect rather than forcing them apart.

**What does it cost / how long?**
One implementation plan, built and tested move-by-move (the same style as sub-project 1). No
new dependencies beyond a text-format reader for the move files.

**When is it done? (the exam)**
All 16 moves are text files, the old Rust move code is deleted, every move proves identical to
before, and the win rate is still 3 out of 10 on seeds 1, 3, and 7.

---

## Plain-language overview

A **move** (the code calls it a "card") is one thing a side can do on its turn: an attack like
"kerberoast" or a defense like "turn on monitoring". Today a move is a Rust struct. After this
work a move is a small text file.

Every move has two halves:

- **The "may I?" half — preconditions.** What must be true for the move to even appear on the
  menu. Sub-project 1 already made this data (the `requires` list on each move). Nothing to
  redo here; we only *read* it.
- **The "what happens" half — effects.** What the move changes when you play it. This is still
  Rust today, and it's what this sub-project turns into data.

So the job is: define a small vocabulary for the "what happens" half, describe each move as a
file, and build the engine that reads a move file and plays it exactly like the old code did.

Two supporting terms used below, defined once:

- **Blackboard** — a small scratchpad a move uses to pass a value from one step to a later
  step (for example, kerberoast's first step finds a target and writes it down; a later step
  reads it). Already exists in `graph.rs` as `Context`.
- **Step order from dependencies** — if step B reads something step A wrote, A must run first.
  The engine already figures this out (`resolve_order` in `graph.rs`). We keep both as-is.

---

## The move file

A move file lists: its identity, its preconditions (the "may I?" list, unchanged from
sub-project 1), the facts it leaves behind, and one or more **steps**. Each step has an
optional runtime check, one effect, and the flavor text / detection exposure that go with it.

```rust
// A runtime check inside a step — NOT a precondition. The move is still on the menu;
// this just decides whether the step succeeds or fizzles, with its own message.
struct Guard {
  req: Requirement,             // reuses the sub-project-1 alphabet
  else_narrative: String,       // shown if this check fails
  else_surface: Vec<Technique>, // what the failed attempt still exposed to the defender
}

struct Node {                   // one step of a move
  id: String,
  requires: Vec<String>,        // blackboard values this step reads
  produces_keys: Vec<String>,   // blackboard values this step writes
  guards: Vec<Guard>,           // runtime checks, in order; first failure stops the move
  effect: Effect,               // the one thing this step does (see vocabulary below)
  ok_surface: Vec<Technique>,   // what a successful step exposed to the defender
  ok_narrative: String,         // flavor text on success
}

struct ToolDef {                // the whole move
  id, side, technique, category, summary,
  gate: Vec<Requirement>,       // the "may I?" preconditions → Card::requires()
  produces: Vec<Fact>,          // facts left behind → Card::produces()
  params_schema: Option<Value>, // only the "write a detection rule" move needs this
  nodes: Vec<Node>,             // the steps
}
```

Most moves are a single step. Only kerberoast has three (find target → request ticket →
crack it). The engine treats a single-step move as a one-step graph, so there's no special case.

**Two real examples** (the files use RON, a text format that maps directly onto these Rust
types — chosen so the move files read close to the code; swappable for JSON later with no
change to the design):

```ron
// tools/remediate_acl.ron — a one-step defense: remove the path to Domain Admin
ToolDef(
  id: "remediate_acl", side: Blue, technique: LateralMove,
  category: PrivilegeEscalation,
  summary: "Remove the GenericAll->DA path / tier admins",
  gate: [Category(PathSevered, false), Category(ScoutDetected, true)],
  produces: [PathSevered],
  nodes: [ Node(
    id: "revoke_dcsync", guards: [], effect: SetFlag(PathSevered),
    ok_surface: [], ok_narrative: "revoked svc_mssql DCSync — path to DA severed",
  )],
)
```

```ron
// tools/kerberoast.ron — a three-step attack, steps wired by the values they pass along
ToolDef(
  id: "kerberoast", side: Red, technique: Kerberoast, category: CredentialAccess,
  summary: "Kerberoast: enum SPNs -> request TGS -> crack (fails vs AES)",
  gate: [Category(ReachesDc, true)], produces: [HasCred],
  nodes: [
    Node(id: "enum_spns",   produces_keys: ["spn_targets"],
         effect: Produce(key: "spn_targets", value: ["MSSQLSvc/dc01.range.local"]),
         ok_surface: [Recon], ok_narrative: "found svc_mssql"),
    Node(id: "request_tgs", requires: ["spn_targets"], produces_keys: ["tgs_hash"],
         effect: Produce(key: "tgs_hash", value: "$krb5tgs$"),
         ok_surface: [Kerberoast], ok_narrative: "got TGS"),
    Node(id: "crack_hash",  requires: ["tgs_hash"],
         guards: [
           Guard(req: Instance(Vuln(Kerberoast), true), else_narrative: "no roastable SPN in this environment", else_surface: []),
           Guard(req: Category(AesEnforced, false),     else_narrative: "AES enforced — ticket uncrackable",    else_surface: []),
         ],
         effect: GrantCred(principal: "range.local\\svc_mssql", secret: Some("Summer2024!"), via: Kerberoast),
         ok_surface: [], ok_narrative: "cracked: Summer2024!"),
  ],
)
```

---

## The effect vocabulary (the "what happens" half)

This is the new part: a small, fixed set of effects. Every one of the 16 moves is built from
these. Each maps one-to-one onto what the old Rust code did, so behavior is preserved exactly.
New Rust module `effects.rs`:

```rust
enum StateFlag {  // the six on/off switches the game already has
  Monitoring, AutoResponse, PathSevered, AesEnforced, PreauthEnforced, DomainAdmin
}
enum Effect {
  Attempt,            // just perform the technique; change no state (recon, bloodhound —
                      // the "scouted"/"path mapped" fact is left behind by the referee
                      // recording that the technique happened)
  Advance,            // move one zone closer (take the next available hop)
  GrantCred { principal, secret: Option<String>, via: Technique },  // steal a credential
  SetFlag(StateFlag), // flip one defense/progress switch on
  RevokeKnownCreds,   // cancel any stolen credentials the defender has spotted
  HuntGap,            // go looking; surface the most valuable thing that slipped past
  DeployDetection,    // write a detection rule for a named technique
  SeverForwardEdges,  // cut the network in front of the attacker
  Produce { key, value },  // just write a value to the blackboard (kerberoast's early steps)
}
```

Each effect knows how to apply itself and reports whether it succeeded and (when it varies) a
message — the exact behavior lifted from the old `play()` bodies. "Chunky" here means an effect
like `HuntGap` carries its whole small policy ("pick the highest-value undetected technique")
rather than being split into tiny generic pieces. We chose chunky so behavior can't drift
during the conversion; splitting into finer pieces is a possible later refinement, not now.

---

## How a move runs (the engine)

New module `tool.rs`. A `DataTool` wraps one validated move file and implements the **existing**
`Card` interface, so the referee, the menu, the registry, and the fog-of-war views (each side
only sees its own information) all stay exactly as they are.

Running a move:

1. Put the steps in dependency order (reuse `resolve_order`).
2. For each step, in order:
   - Check its guards. If one fails, stop the move here and report that guard's message and
     exposure. (No live command is sent — matches today's short-circuit behavior.)
   - Otherwise send the action to the environment (`env.act`) — in the simulator this is a
     stand-in; on the real range it runs the actual command. The effect decides success,
     using the environment's result for the simple switch-flips and its own logic for the
     search-style effects (hunt / rotate / deploy), exactly as today.
   - Apply the effect and collect the step's exposure.
3. Return the combined result. The referee then does its usual bookkeeping around the
   move (recording which technique was performed, scoring detections, checking for the win) —
   unchanged.

Because the move keeps its `id` and `technique`, the two places the referee keys on the name
(blue scoring and the flavor lines) keep working with no change.

---

## The checker (validator)

The engine checks every move file when it loads. This is the safety net that matters most
later, when the AI writes a move with no old code to compare against. There are two levels:

**Per-move** (`validate(&ToolDef)`, run on each file):

1. **Runnable steps** — the step order resolves: no step waits on a value nothing produces,
   and there are no loops.
2. **No dangling reads** — every value a step reads is written by some step in the move.
3. **Leaves-behind check** — every fact a move claims to `produce` is actually established,
   either by one of its effects or by the referee recording the move's technique (for example,
   "recon" leaves behind "scouted" because the referee records that recon happened).

**Across the whole set** (`validate_set(&[ToolDef])`, run after all files load):

4. **Unique names** — no two moves share an `id`.
5. **Category coverage** — at least one move in each of the six required categories (the
   existing rule, now checked over the data).

For this conversion the identical-behavior test (below) is the hard proof; the checker is the
foundation for the future author-a-move front-ends. Checks 3–5 are the first real *use* of the
`requires`/`produces` data that sub-project 1 left populated but unused.

---

## Converting the 16 moves, and proving nothing changed

Same approach that worked in sub-project 1:

- **`tests/arsenal_equivalence.rs`** keeps a frozen copy of the 16 old `play()` bodies as the
  reference. For each move, across a set of hand-built game states covering its branches (crack
  with/without AES and with/without the vulnerability; AS-REP with/without pre-auth; hunt with
  and without something to find; segment with and without a path ahead; escalate with and
  without the path already cut), it plays the new data move and the old code move on identical
  states and asserts they produce the **same result and the same resulting game state**
  (compared by turning both states to JSON and checking equality — no new machinery needed).
- This test is made green **before any old code is deleted**, so it guards every step of the
  conversion. The frozen reference stays in the test as a permanent regression guard.
- **Balance re-measured** after deletion: the 10-seed run on the local model, expecting blue to
  win 3 of 10 (seeds 1, 3, 7) — the established baseline. A deviation is investigated, not
  auto-failed.

---

## What changes in the code

- **New:** `src/effects.rs` (the effect vocabulary), `src/tool.rs` (the move file types, the
  `DataTool`, and the runner), and a loader that reads the move files, checks them, and
  registers them.
- **Self-contained binary preserved:** the move files (`tools/*.ron`) are baked into the
  program at build time (`include_str!`), so it stays a single static binary — the reason the
  command center was ported to Rust. An optional load-from-folder path is reserved for the
  future author-a-move front-end.
- **Modified:** `facts.rs` gains `InstanceProbe::Vuln(Technique)` (so the kerberoast/AS-REP
  runtime checks can be written as data) and the ability to read the alphabet back from text
  (`Deserialize`). The surfaced fact list (`Fact::ALL`) is untouched, so the model's view is
  byte-identical and balance can't drift from that. `graph.rs` keeps the blackboard and the
  step-ordering; its Rust-only step trait and composite type are removed.
- **Deleted:** the 16 move structs and kerberoast's Rust step pieces in `cards.rs`;
  `default_registry()` becomes the loader. `referee.rs` is unchanged (it keys on the stable
  move names).

---

## Tests

1. `arsenal_equivalence.rs` — each move plays identically to the old code (the conversion proof).
2. Extend `taxonomy.rs` — all move files load and pass the checker; category coverage; every
   move's steps resolve; unique names; the leaves-behind check.
3. Checker unit tests — reject a looping move, a dangling read, a duplicate name, a missing
   category, and a move that claims a fact it never establishes.
4. Balance run — 3 of 10 on seeds 1, 3, 7.

---

## Out of scope (on purpose, for later sub-projects)

- The two author-a-move front-ends: a visual builder for a person, and a propose-and-validate
  loop for the AI. This sub-project builds the shared engine they will both feed.
- Loading moves from an external folder (only the baked-in files load for now).
- Splitting the chunky effects into finer generic pieces.
- Moving the flavor text and the blue scoring class into the move file.

---

## One open choice

**Move-file format: RON vs JSON.** RON reads closer to the Rust types and allows comments,
which is nicer for hand-authoring now. JSON is friendlier for the AI to emit later. The design
doesn't depend on which — the reader is swappable. Default: RON now, revisit if/when the AI
front-end lands. Flag for user: fine to default to RON?
