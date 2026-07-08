# Purple Range

**An AI-driven purple-team wargame for your homelab.** Stand up a segmented range,
drive real red-team operations from a live dashboard, and — optionally — turn loose two
context-isolated AI agents (a red cell and a blue cell) that attack and defend the range
against each other while you watch.

> ⚠️ **Authorized use only.** Run this against your **own** lab. See [SECURITY.md](SECURITY.md).

---

## What it is

Most purple-team ranges are enterprise products. Purple Range is a small, self-hostable
kit for the homelab / Proxmox crowd, with two things you won't find elsewhere:

- **A live "digital twin" dashboard** — your segmented network drawn as an interactive map:
  VLAN zones, live host status, the firewall policy as `allow` / `pivot` / `drop` edges,
  attacks that **animate along the real network path** (including forced pivots), a MITRE
  kill-chain bar, a live governance-verdict readout, and a real-time SIEM telemetry feed.
- **An AI-agent attack plane** — prompt-injection, cross-tool exfiltration, and
  classifier-evasion operations that test a **governed AI agent you point it at** (bring
  your own — ouros, a LangGraph agent, anything with a headless/batch CLI).

Plus the classic plane (recon, credential attacks, web enumeration) and a defense plane
(host posture, agent audit trail, SIEM alerts) — all launched from the same dashboard and
streamed live.

## Components

| Path | What |
|---|---|
| `command-center/` | The zero-dependency Node dashboard + backend (the star). |
| `range.config.example.json` | The one file you edit — your hosts, zones, firewall policy, and (pluggable) AI victim. |
| `ops` (in `server.js`) | The operations catalog: recon · access · AI-injection · governance/defense. |
| `wargame/` | The red-cell / blue-cell agents + rules of engagement _(Claude Code powered — coming)_. |
| `mini-range/` | A `docker compose` single-box range for people without Proxmox _(coming)_. |

## Quickstart (bring-your-own hosts)

```bash
git clone <your-fork> purple-range && cd purple-range

cp range.config.example.json range.config.json   # edit: your hosts, IPs, zones, AI agent
cp .env.example .env                              # fill in any SSH/sudo passwords

node command-center/server.js                     # → http://localhost:4899
```

You need SSH access to your range hosts (key or password, configured per host) and,
optionally, a Wazuh SIEM for the live telemetry feed. Everything lab-specific lives in
`range.config.json` and `.env` — **both are gitignored; no secrets ever enter the repo.**

Don't have a Proxmox lab? A `docker compose` mini-range is coming so you can try the whole
thing on one box (with reduced network-segmentation fidelity).

## The wargame

Point a red cell and a blue cell at the range in **separate sessions** (they must not share
context — they interact only through the battlefield: the hosts and the SIEM). Red runs the
kill chain and adapts to what gets blocked; blue watches the telemetry and hardens what it
sees. Scoring is objective — read from the SIEM and the agent audit log. See
`wargame/WARGAME.md` for the rules of engagement.

## Safety

This is offensive tooling for defensive practice on your own lab. Read
[SECURITY.md](SECURITY.md) before you run anything. Non-destructive by default; secrets stay
in your local `.env`; keep the range isolated.

## License

MIT — see [LICENSE](LICENSE).
