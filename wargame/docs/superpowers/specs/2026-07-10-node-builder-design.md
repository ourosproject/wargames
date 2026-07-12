# Move builder (guided form, phase 1 of the node-based builder) — design

- **Status:** design, pending user review
- **Date:** 2026-07-10
- **Sub-project:** 3 of the node-based card-builder direction (the human authoring front-end)
- **Depends on:** arsenal-as-data (merged to `main`) — the move data format + validator this builds on
- **Crate:** `purple-wargame` (`~/Developer/production/purple-range/wargame`)

---

## Heilmeier pitch (read this first)

**What are we building?**
A screen in the dashboard where you can create a new attack or defense move by filling out a
guided form — pick when it's legal, what it does, and what it leaves behind — and the game
validates it, saves it, and lets you play it in the next match. No code, no rebuild.

**How's it done today, and what's the limit?**
The last sub-project turned all 16 moves into data files, but those files are still authored by
hand (by me, in a text editor) and baked into the program at build time. There's no way for a
person sitting at the game to invent a move. The arsenal can't grow without a developer.

**What's new, and why will it work?**
The engine already has the two hard parts: a data format for a move and a validator that checks
it. This adds the missing front-end — a form whose dropdowns are filled from the engine's own
vocabulary, so it can't offer you anything the engine doesn't support, and every save runs
through the real validator. Your moves are saved as ordinary move files in a folder the game
reads at startup, so "play it now" and "still there next session" are the same act.

**Who cares?**
This is the first half of the whole point of the project: a place where new attacks and defenses
get invented and tried. Once a person can author a move and immediately test it, the
cat-and-mouse can actually evolve — and it's the groundwork for the later step where the AI
proposes moves through the same validator.

**What are the risks (and how we handle them)?**
The main risk is a bad authored move breaking the game — handled by validating before saving and
by skipping (not crashing on) a malformed file at startup. A second risk is authored moves
drifting the balance the built-ins guarantee — handled by keeping the two completely separate:
the built-ins keep their frozen guards and 3/10 balance; your moves are experimental and
unguarded on purpose.

**What does it cost / how long?**
One implementation plan. A handful of small HTTP endpoints, one HTML/JS page, and a startup
load-path change. No new dependencies.

**When is it done? (the exam)**
From the dashboard you can: open the Build page, compose a Blue or Red move, hit Validate and see
plain-language errors or a green check, Save it, start a match, and see your move on the menu and
play it — and after quitting and relaunching, it's still there. A deliberately malformed move file
is skipped at startup with a warning instead of crashing. The 16 built-ins are untouched (golden +
taxonomy guards still green, balance still 3/10).

---

## Plain-language overview

Today a "move" is a data file that a developer writes. This adds a **Build** page so *you* can
write one through a form.

Three things make it work, and they're all things the engine already has or nearly has:

1. **A vocabulary the form reads.** A small endpoint reports what the engine supports right now —
   the list of effects, the facts and probes you can gate on, the categories, techniques, and
   sides. The form's dropdowns come from this, so the builder can never offer a choice the engine
   can't honor. Add an effect to the engine later and it appears in the form automatically.
2. **The real validator on save.** When you save, the move goes through the exact same
   `arsenal::validate` checks the built-ins pass. A malformed move is rejected with a plain reason,
   never saved.
3. **A folder the game reads.** Your moves are saved as normal `.ron` move files in
   `~/.purple-range/tools/`. The game loads that folder (on top of the built-ins) when it sets up a
   match, so a saved move is playable in the next match and is still there next session.

Scope for this phase: a **guided form for single-step moves** (the shape the 15 non-composite
built-ins already use). The multi-step **node canvas** and the **AI-proposes-a-move** loop are
later phases — but a saved move is a full move file (the same node/guard/effect structure that
already supports kerberoast's 3-step chain), so those later phases write the exact same format;
the form just fills in a one-step version of it.

---

## Architecture: folder-as-truth

**The authored folder is the single source of truth for your moves.** There is no separate
in-memory list to keep in sync — writing a file to the folder *is* registering the move.

**Loading (a startup + per-match concern).** Two registry builders, kept separate on purpose:

- `arsenal::default_registry()` — **unchanged**: the 16 embedded built-ins only. The golden and
  taxonomy guards keep asserting exactly 16; tests keep using this.
- `arsenal::registry_with_authored(dir)` (new) — built-ins **plus** every valid `.ron` in `dir`.
  A malformed authored file is logged and **skipped**, not fatal (built-ins are still fail-loud
  inside `default_registry`). This is what the running game uses to create matches and to fill the
  catalog.

Because the game builds a match's registry from `registry_with_authored(~/.purple-range/tools/)`
at match-creation time, a move you save shows up in the **next** match you start (a match already
in progress keeps the move list it began with — a predictable rule). Startup re-reads the same
folder, so moves persist across sessions.

**Only the interactive dashboard play/catalog path uses `registry_with_authored`.** The terminal
`cli` path (used for the deterministic balance measurement) stays on `default_registry()` — the
built-ins only — so a balance check can never be polluted by experimental authored moves.

**The move id is reserved once it's a built-in.** A save whose id collides with a built-in id, or
with an existing authored id, is rejected. Built-ins can be viewed in the form as a starting
reference but never overwritten or deleted.

**Endpoints (added to the existing axum dashboard):**

| Method + path | Does |
|---|---|
| `GET /api/vocabulary` | The palette: effects (+ their params), facts, probes, categories, techniques, sides — each with a plain label/description. |
| `GET /api/tools` | List all moves (built-in = read-only, authored = editable/deletable), with their full definition for "load as reference / load to edit". |
| `POST /api/tools` | Validate a submitted move; on success write `~/.purple-range/tools/<id>.ron`; on failure return the plain-language errors. |
| `DELETE /api/tools/:id` | Delete an authored move (authored only; built-ins rejected). |

Trust model: **LAN, no auth** (homelab). The id is strictly sanitized to `[a-z0-9_]+` before it is
ever used as a filename — no path traversal. The folder is created on first save if absent.

---

## The vocabulary endpoint (the palette source)

`GET /api/vocabulary` returns, as JSON, everything the form needs to render — assembled from the
engine so it can't drift:

- **effects** — the 9 `Effect` atoms except `Produce` (hidden until the canvas phase), each with a
  stable key, a plain label, a one-line description (including the "it decides for you" behavior of
  the smart ones: `HuntGap` picks its own target, `SeverForwardEdges` cuts the frontier,
  `RevokeKnownCreds` cancels detected creds, `DeployDetection` writes a rule for a param technique),
  and a small param schema (e.g. `SetFlag` needs a flag; `GrantCred` needs principal/secret/via).
- **facts** — `Fact::ALL` with key, the yes/no question text, and side (audience), so the gate
  builder can group them and warn on cross-side use.
- **probes** — the surfaceable `InstanceProbe` variants with plain descriptions and whether each
  takes a `Technique` or `Category` argument (`SawCategory`, `Identified`, `Vuln`, `Performed`,
  `Detected`, `HasForwardPath`, `LateralPathPlanted`, `CredCompromiseKnown`, `UndetectedActivity`,
  `UndetectedAlert`).
- **categories** — `Category` keys + whether each is defensive (the D3FEND lanes) + which are
  currently empty "reserved" slots (Exfiltration, Deceive, Model, …) that authored moves may use.
- **techniques** — the `Technique` enum keys + human names (this is a fixed list; see Known limits).
- **sides** — Red, Blue.

Most of this metadata already exists (`Fact::key`/`question`/`audience`, `Category::key`/
`is_defensive`, `Technique::as_key`/`attack_name`). New, small: a descriptor list for the effects
and one for the probes (a plain label + arg kind per variant).

---

## The guided form (`/build`)

A new page `public/build.html` served at `/build`, styled to match the existing dark dashboard.
Top-to-bottom, the form builds one move:

- **Name → id.** You type a display name ("My Phish"); the page auto-slugs a safe id (`my_phish`)
  and shows it. Live "id is taken" feedback against the current move list.
- **Side, category, technique** — dropdowns from the vocabulary.
- **Gate (when it's legal)** — add requirement rows. Each row is either a **fact** (have / lack +
  a fact from the dropdown) or a **probe** (e.g. "seen discovery activity", "has a deployed rule
  for Kerberoast"). One level of **"any of these"** grouping is supported (segment uses it). A soft
  warning appears if a Red move gates on a Blue-only fact or vice versa (doesn't block).
- **Effect (what it does)** — pick one of the 9 atoms; the form then shows only that effect's
  parameters (flip-a-switch → which switch; grant-cred → principal + optional secret + via
  technique; deploy-detection → none; etc.).
- **Detection surface** — what Blue can detect when it fires; editable multi-select, prefilled with
  the move's own technique.
- **Narrative** — the line shown in the match feed.
- **Leaves behind (produces)** — **auto-suggested** from the chosen effect + technique (e.g. a
  flip-AES effect suggests `aes_enforced`), editable. This mirrors the validator's own
  "established facts" logic, so a move can't easily claim a fact it never sets.

**Live preview + validate.** The page shows the actual move file it will write, and a **Validate**
button runs the real `arsenal` validator (also run automatically on Save), reporting problems in
plain language ("this move claims to produce 'path severed' but nothing in it sets that").

**Save / manage.** Save validates then writes the file, then offers **"test in a match"** (starts a
new match). The page lists all moves; built-ins are read-only references (click to view/copy into
the form as a starting point); authored moves have **Edit** (loads into the form; Save overwrites)
and **Delete** (with a confirm prompt).

The main dashboard gets a **Build** link.

---

## Validation & safety

- **Structural validation only** — the existing `arsenal::validate` (runnable steps / no dangling
  reads / leaves-behind) plus set-level id-uniqueness against built-ins ∪ authored. No reachability
  or balance analysis (out of scope; authored moves are experimental).
- **Path safety** — id sanitized to `[a-z0-9_]+`, length-capped; rejected otherwise. Never build a
  file path from unsanitized input. `DELETE` only ever removes a file inside the authored dir whose
  name matches a sanitized id.
- **Built-ins immutable** — POST/DELETE targeting a built-in id is rejected.
- **Fail-soft authored load** — a malformed file in the authored dir is skipped with a logged
  warning; the game still launches. Built-ins remain fail-loud.

---

## Deferred (on purpose, later phases)

- The multi-step **node canvas** (drag effect/guard nodes, wire chains). This phase's data format is
  already canvas-ready (a saved move is a full `ToolDef`); the canvas writes the same files.
- The **AI-proposes-a-move** loop (model emits a `ToolDef` → same validator → same folder).
- Authentication / multi-user (LAN-trust for now).
- Editing a built-in (they stay immutable; "start from a built-in" = copy into a new authored move).

---

## Testing

- **Endpoint tests** (Rust, against the handlers): `POST /api/tools` saves a valid move and rejects
  an invalid one with errors; a colliding id is rejected; `GET /api/tools` lists built-ins + a saved
  authored move; `DELETE` removes an authored move and refuses a built-in id; id sanitization
  rejects `../` and non-`[a-z0-9_]` ids (write to a temp dir, not the real home folder).
- **Load-path tests**: `registry_with_authored(tmp)` includes a valid authored move; a malformed
  file in `tmp` is skipped, not fatal, and the good moves still load; `default_registry()` is
  unchanged (still exactly 16).
- **Vocabulary test**: `GET /api/vocabulary` lists all expected effects/facts/probes/categories/
  techniques (and does NOT list `Produce`).
- The built-in guards (golden equivalence, taxonomy, balance 3/10) must stay green — this phase must
  not touch `default_registry`'s built-in set.
- The HTML/JS page is verified by driving it in a browser (open `/build`, author a move, save,
  play) — a manual end-to-end check, since the front-end has no unit-test harness.

---

## Known limits (stated, not bugs)

- **Techniques are a fixed list.** A move must pick an existing technique; a genuinely new technique
  needs a code change (the detection/scoring/coverage machinery keys on the `Technique` enum).
  Free-text technique labels were rejected because a label outside the enum would silently escape
  the whole Blue-detection model.
- **Single-step only this phase.** Multi-step chains wait for the canvas.
- **Authored moves are not balance-guarded** — by design; they're the experimental medium.

---

## Decisions locked (from alignment)

Folder: `~/.purple-range/tools/` (home, repo stays clean). Access: LAN, no auth. Bad authored file:
skip + warn. Effects: all 9 (Produce hidden). Techniques: existing enum only. Id: auto-slug,
`[a-z0-9_]`, collisions rejected. Produces: auto-suggested, editable. Detection surface: editable,
default = the technique. Gate: facts + probes + one-level AnyOf, soft cross-side warning. Built-ins:
read-only references. Delete: confirm. Aesthetic: match the dark dashboard; add Build + test-in-match
links.
