# Security & Authorized Use

Purple Range is a **defensive training tool**: you attack your **own** lab so you can
watch your detections fire and harden what you find. It belongs to the same category as
Atomic Red Team, Caldera, DVWA, and GOAD — offensive techniques packaged for authorized,
self-contained practice.

## Rules of use

- **Only run this against systems you own or are explicitly authorized to test.**
  The operations launch real recon, credential, and injection techniques. Pointing them
  at anything outside your own lab is unauthorized access — don't.
- **Keep it isolated.** Run it on a segmented, air-gapped or firewalled lab network.
  Do not expose the command center, the SIEM, or the target agents to the public internet.
- **Non-destructive by default.** The bundled operations are read/probe/inject only — no
  wiping, no DoS, no mass-scanning. Keep it that way if you extend the catalog.
- **Your secrets stay yours.** Passwords are read from environment variables (`.env`,
  gitignored) and never stored in the repo. `range.config.json` (your IPs/hosts) is also
  gitignored. Review your `git status` before every push.

## Reporting a vulnerability

Found a bug in Purple Range itself (not in a target you're testing)? Open a private
security advisory on the repository rather than a public issue.

## The AI-agent plane

The injection / exfil / classifier-evasion operations test a **governed AI agent that you
configure** (`ai_victim` in the config). They are for probing the robustness of *your own*
agent deployment — the same purpose as garak, PyRIT, or promptfoo. Do not use them against
third-party AI services.
