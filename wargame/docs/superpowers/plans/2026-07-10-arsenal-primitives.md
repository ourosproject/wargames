# Arsenal primitives expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Grow the engine's alphabet so genuinely-different tools become possible — data-defined **compound win conditions** (a win is a combination of facts held together), an objective dimension (exfil, impact), red posture (persistence, C2), one new blue effect (`Evict`), and a richer technique list — with a few characterful new built-in tools proving they compose.

**Architecture:** Reuse, don't reinvent. A win condition is a named `Vec<Requirement>` — the *same* condition language that already gates every move — and red wins if any condition is fully satisfied. Objectives and posture are new booleans on `GameState`, set by the existing `SetFlag` effect; the only new effect is `Evict` (blue removes red's ground / burns persistence). New techniques are new detection identities. Defaults keep today's DA game exactly, so the 16 built-ins and the 3/10 balance are unchanged.

**Tech Stack:** Rust, serde, `ron`. No new dependencies.

## Global Constraints

- Crate: `purple-wargame` at `~/Developer/production/purple-range/wargame`. Branch **`arsenal-primitives`** cut from `main`. Commit per task. Build pristine (0 warnings).
- **Default rules must reproduce today's game exactly.** The default `RuleSet` win condition is `[reach Domain Admin]`, behaviorally identical to the old `red_wins_on_da && red_reached_da`. The 16 built-ins' **golden fixtures must stay green** (`tests/arsenal_equivalence.rs::data_moves_match_frozen_goldens`) and their `play()` is unchanged.
- New `GameState` booleans default `false`; new facts/effects/techniques are used ONLY by new tools. Existing moves reference none of them.
- `Fact::ALL` grows (intended — we're expanding the game). The fog-of-war partition invariant (`red.len() + blue.len() == Fact::ALL.len()`) must still hold.
- Reuse the `Requirement` alphabet for win conditions; reuse `SetFlag` for objectives/posture. The ONLY new effect is `Evict`. Do not add per-objective effects.
- Balance is *measured*, not asserted rigidly: default-rules deterministic-heuristic should stay ~3/10 on {1,3,7}; a real shift is investigated and documented, not forced.
- Every new attack primitive ships with its blue counter (mostly via the existing guard mechanic).
- Content voice: match the game's existing flavor ("roasting service tickets like marshmallows", "you touch it, you lose it. instantly."). The new tools should have personality.

---

## File structure

- **Modify** `src/rules.rs` — `WinCondition` type; `RuleSet.red_win_conditions` replaces `red_wins_on_da`.
- **Modify** `src/referee.rs` — win check iterates conditions; thread an optional win-reason into the report.
- **Modify** `src/state.rs` — new `GameState` booleans + helpers; expand the `Technique` enum + its metadata.
- **Modify** `src/facts.rs` — new objective/posture/stealth `Fact`s + `Fact::ALL`.
- **Modify** `src/effects.rs` — new `StateFlag` variants; new `Effect::Evict`.
- **Modify** `src/main.rs` — print the win reason (default "Domain Admin").
- **Create** `tools/*.ron` — new built-in tools (red families + blue counters); **modify** `src/arsenal.rs` `TOOL_FILES`.
- **Modify** `tests/taxonomy.rs` — the arsenal count.
- **Create** `tests/compound_win.rs` — end-to-end compound-win proof.

---

## Task 1: Compound win conditions (data-defined, reusing the condition alphabet)

**Files:** Modify `src/rules.rs`, `src/referee.rs`, `src/main.rs`.

**Interfaces:**
- Produces: `struct WinCondition { name: String, all_of: Vec<Requirement> }` (in rules.rs); `RuleSet.red_win_conditions: Vec<WinCondition>` (replaces `red_wins_on_da`); referee sets `winner = Some(Red)` + a win-reason when any condition holds.

- [ ] **Step 1: Write the failing test.** Create `src/rules.rs` tests (add a `#[cfg(test)] mod tests`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Fact, Requirement};

    #[test]
    fn default_ruleset_wins_on_domain_admin_only() {
        let rs = RuleSet::default();
        assert_eq!(rs.red_win_conditions.len(), 1);
        assert_eq!(rs.red_win_conditions[0].name, "domain_admin");
        assert_eq!(rs.red_win_conditions[0].all_of, vec![Requirement::have(Fact::DomainAdmin)]);
    }

    #[test]
    fn win_condition_is_a_conjunction() {
        let wc = WinCondition {
            name: "silent_heist".into(),
            all_of: vec![Requirement::have(Fact::HasCred), Requirement::have(Fact::DataExfiltrated)],
        };
        assert_eq!(wc.all_of.len(), 2);
    }
}
```

(These reference `Fact::DomainAdmin`/`HasCred`/`DataExfiltrated` — `DataExfiltrated` is added in Task 3. To keep Task 1 self-contained, use only `DomainAdmin` and `HasCred` in the Task-1 test, both of which exist today. Adjust the second test to `[have(HasCred)]` if `DataExfiltrated` isn't in yet — the point is the shape.)

- [ ] **Step 2: Run — expect FAIL** (`WinCondition`/`red_win_conditions` undefined).

- [ ] **Step 3: Implement in `src/rules.rs`.** Add the import and type, and change the field:

```rust
use crate::facts::Requirement;

/// A way for red to win: red wins the moment EVERY requirement here holds at once.
/// Reuses the same condition alphabet that gates moves — victory and legality speak one language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinCondition {
    pub name: String,
    pub all_of: Vec<Requirement>,
}
```

In `RuleSet`, replace the `pub red_wins_on_da: bool,` line with:

```rust
    pub red_win_conditions: Vec<WinCondition>,
```

In `impl Default for RuleSet`, replace `red_wins_on_da: true,` with:

```rust
            red_win_conditions: vec![WinCondition {
                name: "domain_admin".into(),
                all_of: vec![Requirement::have(Fact::DomainAdmin)],
            }],
```

(add `use crate::facts::Fact;` alongside `Requirement`.)

- [ ] **Step 4: Update the referee win check.** In `src/referee.rs`, replace the block at ~line 357:

```rust
            if self.rules.red_wins_on_da && state.red_reached_da {
                finished = true;
                winner = Some(Side::Red);
            }
```

with a scan over the conditions (report which one fired):

```rust
            for wc in &self.rules.red_win_conditions {
                if wc.all_of.iter().all(|r| r.satisfied(state)) {
                    finished = true;
                    winner = Some(Side::Red);
                    state.win_reason = wc.name.clone();
                    break;
                }
            }
```

Add a `pub win_reason: String` field to `GameState` (default `String::new()`) so the reason survives into the report/feed. (If you prefer not to touch `GameState` here, thread the reason via `PhaseReport`/`RoundReport` instead — but a state field is simplest and the feed already reads state.)

- [ ] **Step 5: Update `main.rs` win messages.** Where `run_cli` prints `"RED WINS — Domain Admin in {} rounds."`, use the reason (fall back to "Domain Admin" if empty):

```rust
                Some(Side::Red) => {
                    let why = if state.win_reason.is_empty() { "Domain Admin".to_string() } else { state.win_reason.replace('_', " ") };
                    println!("RED WINS — {} in {} rounds.", why, rep.round);
                    "red"
                }
```

- [ ] **Step 6: Fix fallout.** `grep -rn red_wins_on_da src/` — update every reference (there should be only `rules.rs` + `referee.rs`). Build.

Run: `cargo build 2>&1 | tail -20` — fix until clean.

- [ ] **Step 7: Run tests — expect PASS.**

Run: `cargo test --lib rules::`
Then full `cargo test` (the 16 goldens must stay green — the default condition reproduces the DA win event).

- [ ] **Step 8: Commit.**

```bash
git add src/rules.rs src/referee.rs src/main.rs src/state.rs
git commit -m "feat(wargame): data-defined compound win conditions (WinCondition; DA is the default)"
```

---

## Task 2: New GameState fields + Technique alphabet expansion

**Files:** Modify `src/state.rs`.

**Interfaces:**
- Produces: `GameState` bools `data_exfiltrated, impact_done, red_persisted, c2_established, egress_blocked, backups_ready, c2_blocked` (all default `false`); new `Technique` variants + full metadata; `Technique::from_key` round-trips.

- [ ] **Step 1: Write the failing tests.** Add to `src/state.rs` tests (create the module if absent):

```rust
#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn new_state_has_no_new_objectives_or_posture() {
        let s = GameState::new(vec![]);
        assert!(!s.data_exfiltrated && !s.impact_done && !s.red_persisted && !s.c2_established);
        assert!(!s.egress_blocked && !s.backups_ready && !s.c2_blocked);
    }

    #[test]
    fn new_techniques_round_trip_and_categorize() {
        use crate::category::Category;
        for t in [Technique::Phishing, Technique::ExploitPublicApp, Technique::ValidAccounts,
                  Technique::LsassDump, Technique::Malware, Technique::C2,
                  Technique::Persistence, Technique::Ransomware] {
            assert_eq!(Technique::from_key(t.as_key()), Some(t), "round-trip {}", t.as_key());
            let _ = t.category(); // must be exhaustive (compile-time)
            assert!(!t.attack_id().is_empty());
        }
        assert_eq!(Technique::Phishing.category(), Category::InitialAccess);
        assert_eq!(Technique::Ransomware.category(), Category::Impact);
        assert_eq!(Technique::C2.category(), Category::CommandAndControl);
        assert_eq!(Technique::Persistence.category(), Category::Persistence);
    }
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Add the `GameState` booleans.** In `struct GameState`, after the blue-posture block, add:

```rust
    // ── red objectives (goals reached) ──
    pub data_exfiltrated: bool,
    pub impact_done: bool,
    // ── red posture (mirrors blue's) ──
    pub red_persisted: bool,
    pub c2_established: bool,
    // ── blue counters to the new red primitives ──
    pub egress_blocked: bool,
    pub backups_ready: bool,
    pub c2_blocked: bool,
    // ── the reason red won (feed/report flavor) ──
    pub win_reason: String,
```

In `GameState::new`, initialise all to `false` / `String::new()`.

- [ ] **Step 4: Expand the `Technique` enum.** Add the 8 variants after `Exfil`:

```rust
    Phishing,
    ExploitPublicApp,
    ValidAccounts,
    LsassDump,
    Malware,
    C2,
    Persistence,
    Ransomware,
```

Add their arms to EVERY `Technique` method (each is a `match`): `as_key`, `from_key`, `value`, `attack_id`, `attack_name`, `data_source`, and `Technique::category()`. Use these (ATT&CK ids are real; flavor the data_source):

| variant | as_key | value | attack_id | attack_name | data_source | category |
|---|---|---|---|---|---|---|
| Phishing | `phishing` | 6 | T1566 | Phishing | `mail gateway · attachment/link detonation` | InitialAccess |
| ExploitPublicApp | `exploit` | 6 | T1190 | Exploit Public-Facing App | `WAF/edge · anomalous request → shell` | InitialAccess |
| ValidAccounts | `valid_accounts` | 6 | T1078 | Valid Accounts | `4624 type-10 from a new geo/asset` | InitialAccess |
| LsassDump | `lsass_dump` | 9 | T1003.001 | LSASS Memory Dump | `Sysmon 10 · handle to lsass.exe` | CredentialAccess |
| Malware | `malware` | 7 | T1204 | Malware Execution | `EDR · unsigned binary / script child proc` | Execution |
| C2 | `c2` | 7 | T1071 | Command & Control | `netflow · beaconing to a rare destination` | CommandAndControl |
| Persistence | `persistence` | 8 | T1547 | Persistence (autostart) | `autoruns · new run-key / service / task` | Persistence |
| Ransomware | `ransomware` | 9 | T1486 | Data Encrypted for Impact | `mass file-rename entropy spike · shadow-copy delete` | Impact |

(`Technique::category()` lives in `state.rs`; add the arms there. `Category::Execution`/`CommandAndControl`/`Persistence`/`Impact` already exist.)

- [ ] **Step 5: Run tests — expect PASS** (`cargo test --lib state`). Then `cargo test` (goldens unaffected — new state defaults false, existing moves untouched).

- [ ] **Step 6: Commit.**

```bash
git add src/state.rs
git commit -m "feat(wargame): objective/posture state + expanded technique alphabet"
```

---

## Task 3: New objective / posture / stealth facts

**Files:** Modify `src/facts.rs`.

**Interfaces:**
- Produces: `Fact` variants `DataExfiltrated, ImpactDone, Persisted, C2Active, Undetected` (red-audience) and `EgressBlocked, BackupsReady, C2Blocked` (blue-audience); all in `Fact::ALL` with `key`/`question`/`audience`/`holds`.

- [ ] **Step 1: Write the failing tests.** Add to `src/facts.rs` tests:

```rust
    #[test]
    fn new_objective_and_posture_facts_hold() {
        let mut s = base();
        assert!(!Fact::DataExfiltrated.holds(&s));
        s.data_exfiltrated = true;
        assert!(Fact::DataExfiltrated.holds(&s));
        s.red_persisted = true;
        assert!(Fact::Persisted.holds(&s));
        s.egress_blocked = true;
        assert!(Fact::EgressBlocked.holds(&s));
    }

    #[test]
    fn undetected_is_true_until_blue_holds_an_alert() {
        let mut s = base();
        s.performed.push(Technique::Malware);
        assert!(Fact::Undetected.holds(&s), "red acted, blue has no alert → undetected");
        s.alerts.push(crate::state::Alert { round: 1, technique: Technique::Malware, source: "edr".into(), rule_id: "r".into(), level: 8 });
        assert!(!Fact::Undetected.holds(&s), "blue now holds an alert → detected");
    }

    #[test]
    fn all_and_fog_partition_hold_after_growth() {
        let s = base();
        let red = table_for(Side::Red, &s);
        let blue = table_for(Side::Blue, &s);
        assert_eq!(red.len() + blue.len(), Fact::ALL.len());
    }
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.** Add the variants to `enum Fact` (red-audience group after `DomainAdmin`; blue group after `PreauthEnforced`):

```rust
    // red objectives / posture (red-private progress)
    DataExfiltrated,
    ImpactDone,
    Persisted,
    C2Active,
    /// Red has performed something and Blue holds NO alert yet — the "under the radar" term.
    Undetected,
    // blue counters (blue-private posture)
    EgressBlocked,
    BackupsReady,
    C2Blocked,
```

Add each to `Fact::ALL`, `key`, `question`, `audience` (the five red facts → `Side::Red` in the `audience` match's Red arm; the three blue → default/Blue), and `holds`:

```rust
            Fact::DataExfiltrated => s.data_exfiltrated,
            Fact::ImpactDone => s.impact_done,
            Fact::Persisted => s.red_persisted,
            Fact::C2Active => s.c2_established,
            Fact::Undetected => !s.performed.is_empty() && s.performed.iter().all(|t| !s.blue_knows(*t)),
            Fact::EgressBlocked => s.egress_blocked,
            Fact::BackupsReady => s.backups_ready,
            Fact::C2Blocked => s.c2_blocked,
```

keys: `data_exfiltrated`, `impact_done`, `persisted`, `c2_active`, `undetected`, `egress_blocked`, `backups_ready`, `c2_blocked`. Questions: plain phrasings (e.g. `Undetected` → "Are you still under the radar (no alerts on your activity)?"). Add the five red facts to the `audience` Red arm; the three blue facts fall to the `_ => Side::Blue` default.

- [ ] **Step 4: Run tests — expect PASS** (`cargo test --lib facts`). Then `cargo test` (goldens unaffected — new facts are additive; existing moves don't reference them).

- [ ] **Step 5: Commit.**

```bash
git add src/facts.rs
git commit -m "feat(wargame): objective/posture/stealth facts (incl. Undetected)"
```

---

## Task 4: StateFlag expansion + the Evict effect

**Files:** Modify `src/effects.rs`.

**Interfaces:**
- Produces: `StateFlag` variants `Persisted, C2Established, DataExfiltrated, ImpactDone, EgressBlocked, BackupsReady, C2Blocked` (+ `key`/`from_key`/`set`); `Effect::Evict` + its `apply`.

- [ ] **Step 1: Write the failing tests.** Add to `src/effects.rs` tests:

```rust
    #[test]
    fn set_flag_sets_new_objective_and_posture_bits() {
        let mut s = base();
        Effect::SetFlag(StateFlag::DataExfiltrated).apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(s.data_exfiltrated);
        Effect::SetFlag(StateFlag::Persisted).apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(s.red_persisted);
        Effect::SetFlag(StateFlag::BackupsReady).apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(s.backups_ready);
    }

    #[test]
    fn evict_burns_persistence_then_removes_ground() {
        let mut s = base();
        s.add_zone("vlan20"); s.add_zone("vlan30");
        s.red_persisted = true;
        // first evict burns the persistence, red keeps its ground
        Effect::Evict.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(!s.red_persisted, "first evict burns persistence");
        assert!(s.holds("vlan30"), "…but red still holds ground");
        // second evict removes the deepest zone
        Effect::Evict.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(!s.holds("vlan30"), "second evict kicks red back");
    }
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.** Add the `StateFlag` variants + their `key`/`from_key`/`set` arms (mirror keys to the fact keys: `persisted`, `c2_active`, `data_exfiltrated`, `impact_done`, `egress_blocked`, `backups_ready`, `c2_blocked`):

```rust
// in StateFlag::set
            StateFlag::Persisted => s.red_persisted = true,
            StateFlag::C2Established => s.c2_established = true,
            StateFlag::DataExfiltrated => s.data_exfiltrated = true,
            StateFlag::ImpactDone => s.impact_done = true,
            StateFlag::EgressBlocked => s.egress_blocked = true,
            StateFlag::BackupsReady => s.backups_ready = true,
            StateFlag::C2Blocked => s.c2_blocked = true,
```

Add `Effect::Evict` to the enum and its `apply` arm (the persist↔evict balance: persistence absorbs one eviction; otherwise red loses its deepest zone):

```rust
            Effect::Evict => {
                if !env_success {
                    return EffectResult { success: false, narrative: None };
                }
                if s.red_persisted {
                    s.red_persisted = false;
                    EffectResult { success: true, narrative: Some("evicted red — but an implant dug back in (persistence burned)".into()) }
                } else {
                    // remove red's deepest (last-added, non-internet) zone
                    let deepest = s.red_zones.iter().rposition(|z| z != "internet");
                    if let Some(i) = deepest {
                        let z = s.red_zones.remove(i);
                        EffectResult { success: true, narrative: Some(format!("evicted red from {} — back on the wrong side of the wire", z)) }
                    } else {
                        EffectResult { success: false, narrative: Some("nothing to evict — red holds no ground".into()) }
                    }
                }
            }
```

(`s.red_zones` and `s.holds` are `pub`/`pub fn` already. `add_zone` exists.)

- [ ] **Step 4: Run tests — expect PASS** (`cargo test --lib effects`). Then `cargo test`.

- [ ] **Step 5: Commit.**

```bash
git add src/effects.rs
git commit -m "feat(wargame): SetFlag over objective/posture bits + the Evict effect (persist↔evict)"
```

---

## Task 5: New built-in tools (the new families + their counters)

**Files:** Create `tools/*.ron` (below); modify `src/arsenal.rs` (`TOOL_FILES`); modify `tests/taxonomy.rs` (count).

Author these with character. Single-node moves; node id = move id. Red tools that pursue an objective **guard on their blue counter** (fail loud when the defense is up), exactly like kerberoast vs AES.

- [ ] **Step 1: Author the red family tools.**

`tools/phish.ron` — a break-in *variant* (distinct technique, same "get a foothold" mechanic):
```ron
ToolDef(
    id: "phish", side: Red, technique: Phishing, category: InitialAccess,
    summary: "Phish a user for the first foothold (a quieter way in than an exploit)",
    gate: [Category(fact: Foothold, want: false), Instance(probe: HasForwardPath, want: true)],
    produces: [Foothold],
    nodes: [ Node(id: "phish", effect: Advance, ok_surface: [Phishing],
        ok_narrative: "spear-phish landed — someone clicked, and we're inside") ],
)
```

`tools/deploy_implant.ron` — persistence (the thing that outlives cleanup):
```ron
ToolDef(
    id: "deploy_implant", side: Red, technique: Persistence, category: Persistence,
    summary: "Drop a persistent implant (survives a single eviction)",
    gate: [Category(fact: Foothold, want: true), Category(fact: Persisted, want: false)],
    produces: [Persisted],
    nodes: [ Node(id: "deploy_implant", effect: SetFlag(Persisted), ok_surface: [Persistence, Malware],
        ok_narrative: "implant installed in a run-key — we'll be back even if they sweep us") ],
)
```

`tools/establish_c2.ron` — command & control:
```ron
ToolDef(
    id: "establish_c2", side: Red, technique: C2, category: CommandAndControl,
    summary: "Beacon out to command-and-control (fails if egress to C2 is blocked)",
    gate: [Category(fact: Foothold, want: true), Category(fact: C2Active, want: false)],
    produces: [C2Active],
    nodes: [ Node(id: "establish_c2",
        guards: [ Guard(req: Category(fact: C2Blocked, want: false), else_narrative: "C2 channel blocked at the egress — beacon going nowhere", else_surface: [C2]) ],
        effect: SetFlag(C2Established), ok_surface: [C2],
        ok_narrative: "C2 online — low-and-slow beacon to a domain nobody's watching") ],
)
```

`tools/exfil_data.ron` — the data-heist objective (needs a cred; dies to egress-block):
```ron
ToolDef(
    id: "exfil_data", side: Red, technique: Exfil, category: Exfiltration,
    summary: "Stage and exfiltrate the crown-jewel data (fails if egress is blocked)",
    gate: [Category(fact: HasCred, want: true), Category(fact: DataExfiltrated, want: false)],
    produces: [DataExfiltrated],
    nodes: [ Node(id: "exfil_data",
        guards: [ Guard(req: Category(fact: EgressBlocked, want: false), else_narrative: "egress locked down — the data isn't leaving the building", else_surface: [Exfil]) ],
        effect: SetFlag(DataExfiltrated), ok_surface: [Exfil],
        ok_narrative: "data staged and shipped out over the C2 — the crown jewels are gone") ],
)
```

`tools/ransomware.ron` — the impact objective (needs a foothold; dies to backups):
```ron
ToolDef(
    id: "ransomware", side: Red, technique: Ransomware, category: Impact,
    summary: "Detonate ransomware across the estate (worthless if backups are ready)",
    gate: [Category(fact: Foothold, want: true), Category(fact: ImpactDone, want: false)],
    produces: [ImpactDone],
    nodes: [ Node(id: "ransomware",
        guards: [ Guard(req: Category(fact: BackupsReady, want: false), else_narrative: "they had clean backups — restore in progress, extortion's a bust", else_surface: [Ransomware]) ],
        effect: SetFlag(ImpactDone), ok_surface: [Ransomware],
        ok_narrative: "everything's encrypted. shadow copies deleted. bitcoin or bust.") ],
)
```

- [ ] **Step 2: Author the blue counter tools.**

`tools/evict.ron` — kick red out / burn the implant (the new effect):
```ron
ToolDef(
    id: "evict", side: Blue, technique: Pivot, category: Evict,
    summary: "Evict red from a host — kicks it back, or burns an implant first",
    gate: [Category(fact: IntrusionDetected, want: true), Category(fact: Foothold, want: true)],
    produces: [],
    nodes: [ Node(id: "evict", effect: Evict, ok_surface: [],
        ok_narrative: "isolated and reimaged the host — red's back on the wrong side of the wire") ],
)
```

`tools/block_egress.ron` — DLP / egress lockdown (kills exfil + C2):
```ron
ToolDef(
    id: "block_egress", side: Blue, technique: Exfil, category: Isolate,
    summary: "Lock down egress / DLP — data can't leave (kills exfil)",
    gate: [Instance(probe: SawCategory(Exfiltration), want: true), Category(fact: EgressBlocked, want: false)],
    produces: [EgressBlocked],
    nodes: [ Node(id: "block_egress", effect: SetFlag(EgressBlocked), ok_surface: [],
        ok_narrative: "egress filtered to a whitelist — nothing large is leaving the network") ],
)
```

`tools/backups.ron` — proactive resilience (defangs ransomware):
```ron
ToolDef(
    id: "backups", side: Blue, technique: Ransomware, category: Harden,
    summary: "Ready tested, offline backups — makes ransomware a non-event",
    gate: [Category(fact: BackupsReady, want: false)],
    produces: [BackupsReady],
    nodes: [ Node(id: "backups", effect: SetFlag(BackupsReady), ok_surface: [],
        ok_narrative: "offline backups verified — if they encrypt us, we just roll back") ],
)
```

`tools/block_c2.ron` — sever the beacon:
```ron
ToolDef(
    id: "block_c2", side: Blue, technique: C2, category: Isolate,
    summary: "Sinkhole/deny the C2 destination — cuts the beacon",
    gate: [Instance(probe: SawCategory(CommandAndControl), want: true), Category(fact: C2Blocked, want: false)],
    produces: [C2Blocked],
    nodes: [ Node(id: "block_c2", effect: SetFlag(C2Blocked), ok_surface: [],
        ok_narrative: "C2 domain sinkholed at the resolver — the beacon's screaming into the void") ],
)
```

- [ ] **Step 3: Embed them + teach the validator the new facts + fix the count.**
  - In `src/arsenal.rs` `TOOL_FILES`, add nine `include_str!("../tools/<id>.ron")` entries (phish, deploy_implant, establish_c2, exfil_data, ransomware, evict, block_egress, backups, block_c2).
  - **Extend `established_facts` in `src/arsenal.rs`** — the produces-lint maps each `SetFlag` to the fact it establishes, and the new tools produce new facts. Add these arms to the `match &n.effect` (alongside the existing `SetFlag(StateFlag::Monitoring) => …`):

```rust
            Effect::SetFlag(StateFlag::Persisted) => add(Fact::Persisted, &mut out),
            Effect::SetFlag(StateFlag::C2Established) => add(Fact::C2Active, &mut out),
            Effect::SetFlag(StateFlag::DataExfiltrated) => add(Fact::DataExfiltrated, &mut out),
            Effect::SetFlag(StateFlag::ImpactDone) => add(Fact::ImpactDone, &mut out),
            Effect::SetFlag(StateFlag::EgressBlocked) => add(Fact::EgressBlocked, &mut out),
            Effect::SetFlag(StateFlag::BackupsReady) => add(Fact::BackupsReady, &mut out),
            Effect::SetFlag(StateFlag::C2Blocked) => add(Fact::C2Blocked, &mut out),
```

  Without this, `validate` rejects the new tools ("produces X but nothing establishes it") and `default_registry` panics at load.
  - In `tests/taxonomy.rs`, change the two places that assert `16`/`reg.len() == 16` to `25` (16 + 9). Leave the golden-count assertion (180, over the original 16) untouched — the new tools are NOT in the goldens.

- [ ] **Step 4: Write the failing tests.** In `src/arsenal.rs` tests, assert the new arsenal loads and validates, and add per-tool play checks proving the guards. Example:

```rust
    #[test]
    fn full_arsenal_loads_with_new_families() {
        let reg = default_registry();
        assert_eq!(reg.len(), 25);
        for id in ["phish","deploy_implant","establish_c2","exfil_data","ransomware","evict","block_egress","backups","block_c2"] {
            assert!(reg.get(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn exfil_fails_when_egress_blocked_succeeds_otherwise() {
        use crate::env::SimEnvironment;
        use crate::state::{Cred, GameState, Host, Technique};
        let reg = default_registry();
        let tool = reg.get("exfil_data").unwrap();
        let mut s = GameState::new(vec![Host { id: "e".into(), zone: "internet".into(), label: "e".into(), foothold: false, reachable_by_red: true }]);
        s.creds.push(Cred { principal: "svc".into(), secret: None, cracked: true, via: Technique::Kerberoast });
        // egress open → exfil succeeds and sets the objective
        let mut ok = s.clone();
        let o = tool.play(&mut ok, &serde_json::json!({}), &mut SimEnvironment::new());
        assert!(o.success && ok.data_exfiltrated);
        // egress blocked → guard fails, no objective
        let mut blocked = s.clone(); blocked.egress_blocked = true;
        let o2 = tool.play(&mut blocked, &serde_json::json!({}), &mut SimEnvironment::new());
        assert!(!o2.success && !blocked.data_exfiltrated);
    }
```

Run: `cargo test --lib arsenal` → expect FAIL first (count/tools), then implement Steps 1-3, then PASS. Then full `cargo test` (goldens for the original 16 stay green; taxonomy now expects 25).

- [ ] **Step 5: Commit.**

```bash
git add tools/ src/arsenal.rs tests/taxonomy.rs
git commit -m "feat(wargame): new built-in tools — phishing, implant, C2, exfil, ransomware + blue counters"
```

---

## Task 6: Prove a compound win end-to-end

**Files:** Create `tests/compound_win.rs`.

- [ ] **Step 1: Write the test.** A scripted match with a custom ruleset whose win condition is the compound "silent heist" — red must hold a cred, have exfiltrated, AND be undetected:

```rust
//! Proves a data-defined COMPOUND win: red wins by satisfying a combination of conditions at once,
//! not by reaching Domain Admin.

use purple_wargame::card::Environment;
use purple_wargame::env::SimEnvironment;
use purple_wargame::facts::{Fact, Requirement};
use purple_wargame::referee::Referee;
use purple_wargame::rules::{RuleSet, WinCondition};
use purple_wargame::state::{Cred, GameState, Host, Technique};
use purple_wargame::arsenal;

fn heist_rules() -> RuleSet {
    RuleSet {
        red_win_conditions: vec![WinCondition {
            name: "silent_heist".into(),
            all_of: vec![
                Requirement::have(Fact::HasCred),
                Requirement::have(Fact::DataExfiltrated),
                Requirement::have(Fact::Undetected),
            ],
        }],
        ..RuleSet::default()
    }
}

#[test]
fn red_wins_a_silent_heist_only_when_all_three_hold() {
    let reg = arsenal::default_registry();
    let referee = Referee { rules: heist_rules(), registry: reg };
    let mut env = SimEnvironment::new();

    // red reaches the DC, holds a cred, and exfiltrates — quietly (no alerts).
    let mut s = GameState::new(vec![Host { id: "e".into(), zone: "internet".into(), label: "e".into(), foothold: false, reachable_by_red: true }]);
    s.add_zone("vlan30"); // reaches DC
    s.creds.push(Cred { principal: "svc".into(), secret: None, cracked: true, via: Technique::Kerberoast });
    s.performed.push(Technique::Exfil);

    // not yet exfiltrated → not a win
    assert!(referee.rules.red_win_conditions[0].all_of.iter().any(|r| !r.satisfied(&s)));

    // play exfil_data → sets DataExfiltrated; still no alerts → Undetected holds → WIN
    let mv = purple_wargame::card::Move { side: purple_wargame::state::Side::Red, card: "exfil_data".into(), params: serde_json::json!({}) };
    referee.begin_round(&mut s);
    let report = referee.red_phase(&mut s, &mv, &mut env);
    assert!(report.finished && report.winner == Some(purple_wargame::state::Side::Red), "silent heist should win");
    assert_eq!(s.win_reason, "silent_heist");

    // control: if blue had an alert on the exfil, Undetected is false → NOT a win
    let mut s2 = GameState::new(vec![Host { id: "e".into(), zone: "internet".into(), label: "e".into(), foothold: false, reachable_by_red: true }]);
    s2.add_zone("vlan30");
    s2.creds.push(Cred { principal: "svc".into(), secret: None, cracked: true, via: Technique::Kerberoast });
    s2.data_exfiltrated = true;
    s2.performed.push(Technique::Exfil);
    s2.alerts.push(purple_wargame::state::Alert { round: 1, technique: Technique::Exfil, source: "dlp".into(), rule_id: "r".into(), level: 8 });
    let won = referee.rules.red_win_conditions[0].all_of.iter().all(|r| r.satisfied(&s2));
    assert!(!won, "detected exfil must NOT win the silent heist");
}
```

- [ ] **Step 2: Run — iterate until PASS.** (`cargo test --test compound_win`). If the exact API (`begin_round`/`red_phase` signatures, `Move` path) differs, adjust to the real signatures — the assertions are the point: all-three → win with `win_reason == "silent_heist"`; detected → no win.

- [ ] **Step 3: Commit.**

```bash
git add tests/compound_win.rs
git commit -m "test(wargame): compound win proven end-to-end (silent heist: creds + exfil + undetected)"
```

---

## Task 7: Compatibility + balance re-measure

**Files:** Create `tests/balance_note.md` (append a section).

- [ ] **Step 1: Confirm the built-ins are untouched.** Run the full suite; the frozen goldens for the original 16 must be green and `default_registry` must contain them.

Run: `cargo test` — all green, 0 warnings.

- [ ] **Step 2: Measure balance with DEFAULT rules.** Build, then run seeds 1..10 with the deterministic heuristic (no model), default ruleset (DA-only win, now with 25 tools in the arsenal):

Run: `WARGAME_SEED=<n> ./target/debug/purple-wargame cli` for n in 1..10; record the winner line.

- [ ] **Step 3: Record + judge.** Append to `tests/balance_note.md` a "primitives expansion" section with the 10-seed table. Expectation: still ~3/10 on {1,3,7} — the heuristic doesn't prefer the new tools (its Red preference list is unchanged), and the default win is DA-only, so the DA race is undisturbed. If it deviates, investigate *why* (is the heuristic picking a new tool via its `legal[0]` fallback and stalling the race?) and document the finding honestly — a real shift from new mechanics is a data point, not a failure, but an unexplained one is a bug.

- [ ] **Step 4: Commit.**

```bash
git add tests/balance_note.md
git commit -m "test(wargame): balance re-measured with the expanded arsenal (default rules)"
```

---

## Self-review notes (for the implementer)

- **The default game is sacred.** The default `RuleSet` win condition is `[reach DA]`; the referee's win event must be identical to the old `red_wins_on_da && red_reached_da`. If a golden fixture for the 16 built-ins goes red, you changed behavior you shouldn't have — the new state defaults `false` and existing moves never touch it.
- **One new effect only.** Objectives and posture are `SetFlag` over new booleans; `Evict` is the only new `Effect`. If you're tempted to add an `Achieve`/`Exfiltrate`/`Persist` effect, stop — it's a `SetFlag`.
- **Guards are the counter mechanism.** Exfil/ransomware/C2 *guard* on their blue counter (`lack(EgressBlocked)` etc.) and fail loud when it's up — the same shape as kerberoast vs AES. Don't invent new machinery for "the defense stopped it."
- **`Undetected`** is computed from existing state (`performed` non-empty AND blue holds no alert for any of it) — no new field.
- **Content voice.** The narratives above are the bar — keep new tools characterful; a bland arsenal is a missed opportunity.
- **Win-reason flavor.** `win_reason` (snake_case condition name) flows to the CLI/feed; a compound win reads "RED WINS — silent heist in N rounds."
