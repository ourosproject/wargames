# Prompt — Design the Purple Range Wargame UI

> Paste the block below into Claude (design). It produces an interactive prototype we can wire to the real engine.

---

You are designing the interface for **Purple Range**, a round-based **red-vs-blue cybersecurity wargame that runs on a real homelab** (a live Windows Active Directory range). Produce a **single self-contained interactive HTML prototype** — inline CSS/JS, **no external assets or CDNs** (it is served as an embedded page by a Rust/axum binary and rendered as an Artifact). Use mock data, but bind everything to the real contracts at the bottom so this can become the architecture we build on.

**The game, in one line:** red runs an AD attack chain (recon → Kerberoast → AS-REP → BloodHound → DCSync → Domain Admin); **blue is an active "pitbull" defense** that watches, contains, and hardens in real time (enforce AES, enforce pre-auth, revoke DCSync, rotate creds, deploy detections). A **neutral code referee** scores objectively; the battlefield (topology/firewall) is **frozen** so only tradecraft and defenses evolve — *"evolution, not favoritism."*

**What's wrong today:** it's a passive spectator view — one-shot, no user involvement, not visually compelling.

**Design goals**
1. **Real involvement** — three modes: **spectate** (both sides autonomous), **1-player** (pick a side; an AI/heuristic plays the other), **2-player**. On a human turn you pick one card from your *legal* hand + fill its params.
2. **Fog of war (hard rule):** each seat sees ONLY its own view — red never sees blue's detections; blue never sees red's ground truth. The UI must reflect/enforce this per side.
3. **Blue is the star** — the product is an active defender; make its detections, containments, and hardening feel powerful and alive (the "bite").
4. **Sim vs LIVE must feel different** — LIVE fires real actions on real hardware (high stakes). Make that unmistakable, with a **Reset Range** control.
5. **Tactical war-room aesthetic** — dark, RED vs BLUE, cinematic but instantly legible; motion that *communicates* (compromise spreading, a detection firing, a containment biting).

**Key regions**
- **Battlefield** — a node-graph "infection" map: VLAN columns (Attacker → Targets → DC → SIEM), hosts as nodes, edges that ignite as compromise spreads; **dc01 is the crown objective**; dotted telemetry lines to the SIEM; node states: clean / compromised / detected / contained / owned. Nodes are first-class — this seeds a future **node-based attack/defense builder**.
- **Scoreboard + round timeline** (RED vs BLUE totals, per-round deltas, winner banner).
- **Play-by-play feed** — per-round narrative lines with STEALTH / DETECTED / CONTAINED / HONEYPOT badges.
- **Your hand** — the legal cards for your side this turn (from the catalog) with param inputs; playing one animates the battlefield + feed.
- **Setup bar** — mode, side, sim/LIVE toggle, Reset Range, and an **OpenAI-compatible model-endpoint field** (a small local model can drive a side).

**Data contracts (bind to these)**
- `GET /api/catalog` → `{ cards:[{ id, side:"Red"|"Blue", technique, summary, params_schema }] }`
- `GET /api/game?live=0|1` → **SSE**: `start{battlefield,objective,mode}` · `round{round,lines[],red,blue,finished,winner,bf{red_cred,bloodhound,dc_owned,blue_watching,alerts,honeytokens}}` · `end`
- `POST /api/reset` → `{ok,msg}`
- **Human turns (implemented):** `POST /api/match {mode:"spectate"|"1p"|"2p", side, live}` → session view; `GET /api/match/:id?side=red|blue` → view; `GET /api/legal?match=&side=` → your legal hand; `POST /api/move {match,side,card,params}` → applies your move, the AI plays the other seat, returns your updated view; `POST /api/step {match}` → advance one agent phase (spectator). Every view is **fog-of-war**: `{id, round, awaiting, your_turn, finished, winner, red, blue, mode, bf{...}, legal:[…only your side…], feed:[…side-filtered…]}`.

**Deliver:** the interactive prototype **plus a short component + state map** — what components exist, what state each owns, and how each binds to the events/endpoints above — so this doubles as the architecture spec.

---
