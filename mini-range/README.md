# Docker Mini-Range

The whole thing on one box — no Proxmox, no VMs. A small Docker network with an
attacker container, two SSH targets (each with a planted flag and a weak login),
and the command center wired to drive them.

```bash
cd mini-range
docker compose up --build         # first run builds the images (~a few minutes)
```

Then open **http://localhost:4899**.

Click a target on the map and run the operations — recon, the SSH credential
attack (the demo password `changeme` is in the wordlist, so it lands), and the
host posture scan (which finds the world-readable flag at
`/srv/agent/secret/flag.txt`). Watch it stream in the console.

Tear down:

```bash
docker compose down
```

## What you get vs. the full range

| | Mini-range (Docker) | Full range (your lab) |
|---|---|---|
| Setup | one command | your hosts + `range.config.json` |
| Network segmentation | a flat Docker bridge (no real VLAN/firewall pivots) | real VLANs, real firewall policy |
| Classic plane (recon, creds, posture) | ✅ | ✅ |
| Live SIEM telemetry feed | — (no SIEM in the mini-range yet) | ✅ (point it at your Wazuh) |
| AI-agent plane (injection/exfil) | bring your own agent (`ai_victim` in config) | bring your own agent |

The mini-range is for *trying it in minutes*. For the real segmentation mechanics
(the pivot animations, firewall `drop`/`pivot` edges) and the live telemetry, run
it against your own segmented lab with `range.config.json`.

## Safety

Intentionally-weak containers with a throwaway `changeme` login. Keep it on your
own machine; don't expose port 4899 or the containers to a network you don't
control. See [../SECURITY.md](../SECURITY.md).
