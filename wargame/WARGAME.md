# Purple Range — Wargame Engine (design spec)

An autonomous, round-based **red-vs-blue purple-team wargame** that runs on your range.
A Red agent attacks; a Blue agent defends *and fights back*; a neutral Referee runs the
rounds and scores them from ground truth. **Model-pluggable** (bring your own local model),
**dashboard-driven**, and **human-playable** (take a side yourself).

The point is a **game of evolution** — both sides genuinely level up round over round —
not an operator tipping the scales.

---

## Prime directive: evolution, not favoritism
Three things exist; only two may change:
- **The battlefield** — topology, VLAN segmentation, firewall *base* posture, which hosts
  exist — is **FROZEN**. Never altered to favor a side. Any change is neutral lab hygiene
  (e.g. DC NTP), done in the open, never mid-round.
- **Blue's capability** (detection, response, deception, hardening) — **evolves.**
- **Red's capability** (tradecraft, tooling, evasion) — **evolves.**

Every improvement is re-tested by the other side. Detection gaps are *findings*, never
papered over.

## Rules of Engagement (referee-enforced)
1. **Detect before respond.** Blue may act only on what its own telemetry surfaced — no
   god-mode knowledge of red's actual moves.
2. **Act on your own side.** Blue acts on its systems + responses to detected threats; it
   never rewrites the base battlefield or reaches preemptively into red's box. Red operates
   from its foothold via legitimate paths (e.g. the pivot), never by the referee handing it access.
3. **No overfit detections.** Blue rules must be *technique-based* ("any RC4 TGS in an AES
   domain"), not keyed to this run's artifacts ("srcip == X", "user == svc_mssql"). Referee flags overfit.
4. **Every move is counterable.** Whatever one side does, the other gets a turn to adapt.
5. **Non-destructive, authorized-lab-only.** All action stays inside the sealed range on the
   owner's own machines. No real-world hack-back; "counterattack" exists only as in-lab
   deception/redirect.

---

## Architecture: THICK ENGINE, THIN MODEL
The models are small (7B-class, CPU on the bench or local on the Mac). So the **engine does
the heavy lifting deterministically; the model makes only small, constrained choices.**

- **Referee (code, deterministic):** owns the round loop, scoring, ROE enforcement, and safe
  execution. Impartial — this replaces the biased human operator. **Not an LLM.**
- **Action library (code):** every red technique and blue response is a coded, parameterized
  **card** — preconditions, effect, and how it's scored. The card *does* the work (runs the
  roast, deploys the rule, drops the block, plants the honeytoken). **Execution is always
  code, never the model.**
- **Agents (thin LLM):** each turn the agent gets a compact state summary + the **menu of
  currently-legal cards**, and returns **one card + its parameters** — nothing more. Output is
  **grammar/JSON-schema-constrained** (GBNF for llama.cpp/Ollama, guided decoding for vLLM) so
  even a small model can't emit garbage. The engine validates the choice against ROE +
  preconditions before running it.
- **Graceful degradation:** a built-in **heuristic policy** can play either side with **no
  model at all**; a small model improves card selection; a bigger model improves strategy.
  The brain is pluggable and optional. **The engine is the authority; the model is only an
  advisor** — it can never do anything unsafe or out-of-bounds because it can only pick from
  the legal menu.
- **Model layer:** any **OpenAI-compatible endpoint** — local Ollama / vLLM / ouros. Set URL +
  model in config or the dashboard. (ouros can be the governed runtime for the agents.)
- **Dashboard (command-center, extended):** configure endpoint/model, start/pause/step rounds,
  watch the live red-vs-blue timeline + scoreboard, and **take a side** (a human picks cards
  from the same menu the model would).
- **The range = the arena** (frozen battlefield).

## The round loop
```
Referee: start Round N, snapshot state
  -> Red turn:  [state + legal red cards] -> model/human/heuristic picks a card -> engine executes -> observe
  -> Referee:   score red's effect + pull blue-relevant telemetry (Wazuh/EDR)
  -> Blue turn: [telemetry + legal blue cards] -> picks a card -> engine executes (detect/contain/block/harden/deceive)
  -> Referee:   score the round, log the delta, update both sides' memory
Round N+1 ...
```

## Card libraries
**Red (technique cards):** recon (nmap/enum) · pivot (SOCKS via owned host) · kerberoast ·
asrep-roast · bloodhound-collect · cred-spray · cred-crack · lateral-move · exfil ·
evasion modifiers (RC4->AES, low-and-slow, source-rotate). Each card declares preconditions
(needs foothold/cred/pivot), effect (loot/access), and the detection surface it touches.

**Blue (response cards):**
- **Detect** — deploy a technique-based detection (Sigma -> Wazuh rule) into `defense/detections/`.
- **Contain** — kill process, lock/disable a compromised account, kill sessions, isolate a host.
- **Block** — firewall-drop a *detected* attacker source (Wazuh active-response), host-firewall block.
- **Harden** — rotate a roasted account + remove SPN, clear DONT_REQ_PREAUTH, force AES / disable RC4, tighten GPO.
- **Deceive** — plant honeytokens / honey-accounts / fake SPNs; stand up a honeypot / tarpit /
  sinkhole; redirect red to a decoy. Red's recon becomes blue's tripwire.

## Scoring (referee, objective, from ground truth)
Per round, from Wazuh + attack results + host/AD state:
- **Red points:** objectives achieved (creds cracked, DA path reached, exfil proven) + stealth
  bonus (actions with no alert) + dwell time before detection.
- **Blue points:** detection coverage (techniques alerted) + speed (time-to-detect) +
  containment (attacker action blocked/evicted) + deception hits (red touched a honeytoken) +
  low false-positive rate.
- **The scoreboard is the fitness function** both agents optimize against — that's the evolution.

## Playable
Any turn's decision can be made by the model, the built-in heuristic, or **a human** (pick a
card from the dashboard menu). Mix freely: human-red vs model-blue, model vs model, etc.

---

## Build phases
0. **Prototype (Claude-driven):** persistent Red + Blue agents + a scoring referee; 2-3 rounds
   against the range to prove the loop + ROE. (Claude, not your model yet.)
1. **Engine (Rust):** referee + round manager + card library + scorer + constrained
   model client (OpenAI-compatible) + heuristic fallback. Everything above, one binary.
2. **Dashboard:** wire the engine into the command-center — config, live timeline/scoreboard,
   take-a-side controls.

## Tech notes
- Rust (matches the command-center + single-binary distribution). Model calls over
  OpenAI-compatible HTTP; **constrained decoding (GBNF / JSON schema)** for reliable
  small-model output. Keep prompts short and state summaries compact (bench CPU is slow, 7B
  context is small). **One atomic decision per model call.** The engine, not the model, owns
  correctness, safety, and the ROE.

## Status
The manual reps run this session (pivot -> kerberoast/asrep -> Wazuh score -> propose a
technique-based rule) ARE the working spec — the engine automates exactly that loop. Blue's
detect-then-respond and the pivot tooling already exist as `defense/detections/` and
`~/Developer/development/proxmox-bench/redteam-ops/`.
