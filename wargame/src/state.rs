//! Core game state — the shared vocabulary of the wargame.
//!
//! Backend-agnostic: both the simulator and the live-range backend read and write this
//! same state. The referee owns it; cards mutate it (through an `Environment`) on their turn.

use serde::{Deserialize, Serialize};

/// Which player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Red,
    Blue,
}

/// An ATT&CK-flavored technique — the shared unit that red *performs* and blue *detects*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Technique {
    InitialAccess,
    Recon,
    Pivot,
    Kerberoast,
    AsRepRoast,
    BloodHound,
    CredSpray,
    LateralMove,
    Exfil,
}

impl Technique {
    pub fn as_key(&self) -> &'static str {
        match self {
            Technique::InitialAccess => "initial_access",
            Technique::Recon => "recon",
            Technique::Pivot => "pivot",
            Technique::Kerberoast => "kerberoast",
            Technique::AsRepRoast => "asrep",
            Technique::BloodHound => "bloodhound",
            Technique::CredSpray => "credspray",
            Technique::LateralMove => "lateral",
            Technique::Exfil => "exfil",
        }
    }
    pub fn from_key(s: &str) -> Option<Technique> {
        Some(match s {
            "initial_access" => Technique::InitialAccess,
            "recon" => Technique::Recon,
            "pivot" => Technique::Pivot,
            "kerberoast" => Technique::Kerberoast,
            "asrep" => Technique::AsRepRoast,
            "bloodhound" => Technique::BloodHound,
            "credspray" => Technique::CredSpray,
            "lateral" => Technique::LateralMove,
            "exfil" => Technique::Exfil,
            _ => return None,
        })
    }
    pub fn value(&self) -> i32 {
        match self {
            Technique::Kerberoast | Technique::AsRepRoast => 9,
            Technique::LateralMove => 8,
            Technique::BloodHound => 7,
            Technique::InitialAccess => 6,
            Technique::Pivot => 5,
            Technique::Recon => 3,
            _ => 4,
        }
    }
    /// MITRE ATT&CK technique id — makes coverage comparable to CALDERA / Atomic Red Team.
    pub fn attack_id(&self) -> &'static str {
        match self {
            Technique::InitialAccess => "T1190",
            Technique::Recon => "T1087.002",
            Technique::Pivot => "T1210",
            Technique::Kerberoast => "T1558.003",
            Technique::AsRepRoast => "T1558.004",
            Technique::BloodHound => "T1069.002",
            Technique::CredSpray => "T1110.003",
            Technique::LateralMove => "T1003.006",
            Technique::Exfil => "T1041",
        }
    }
    pub fn attack_name(&self) -> &'static str {
        match self {
            Technique::InitialAccess => "Initial Access (edge exploit/phish)",
            Technique::Recon => "Account/Domain Discovery",
            Technique::Pivot => "Lateral Movement / Pivot",
            Technique::Kerberoast => "Kerberoasting",
            Technique::AsRepRoast => "AS-REP Roasting",
            Technique::BloodHound => "Permission Groups Discovery (BloodHound)",
            Technique::CredSpray => "Password Spraying",
            Technique::LateralMove => "DCSync (OS Credential Dumping)",
            Technique::Exfil => "Exfiltration Over C2",
        }
    }
    /// The ATT&CK tactic (kill-chain stage) this technique belongs to.
    pub fn category(&self) -> crate::category::Category {
        use crate::category::Category;
        match self {
            Technique::InitialAccess => Category::InitialAccess,
            Technique::Recon | Technique::BloodHound => Category::Discovery,
            Technique::Pivot => Category::LateralMovement,
            Technique::Kerberoast | Technique::AsRepRoast | Technique::CredSpray
                | Technique::LateralMove => Category::CredentialAccess,
            Technique::Exfil => Category::Exfiltration,
        }
    }
    /// The telemetry a detection for this technique should key on — so a GAP names the rule to write.
    pub fn data_source(&self) -> &'static str {
        match self {
            Technique::InitialAccess => "edge/WAF logs · new external→internal session",
            Technique::Recon => "LDAP query logging · Security 4662",
            Technique::Pivot => "east-west netflow · 4624 type-3 across segments",
            Technique::Kerberoast => "Security 4769 · TGS-REQ etype 0x17 (RC4)",
            Technique::AsRepRoast => "Security 4768 · AS-REQ preauth-not-required / 0x17",
            Technique::BloodHound => "LDAP · mass 4662 directory reads (SharpHound)",
            Technique::CredSpray => "Security 4625/4771 · many accounts, one source",
            Technique::LateralMove => "Security 4662 · DS-Replication-Get-Changes GUIDs",
            Technique::Exfil => "netflow · DLP · large egress",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub zone: String,
    pub label: String,
    pub foothold: bool,
    pub reachable_by_red: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cred {
    pub principal: String,
    pub secret: Option<String>,
    pub cracked: bool,
    pub via: Technique,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub id: String,
    pub technique: Technique,
    pub deployed_round: u32,
    pub technique_based: bool,
    /// Rule quality (the no-overfit ROE): "robust" generalizes across environments; "overfit"
    /// only matches this run's artifacts (won't catch the technique elsewhere); "noisy" fires on
    /// benign traffic (false positives). It catches THIS match either way — the grade is the finding.
    pub fidelity: String,
}

/// Grade a freshly-written rule against a varied scenario + a benign baseline (deterministic per
/// seed). Techniques with a crisp invariant (Kerberoast=4769/RC4, DCSync=replication GUIDs) are
/// easy to write robustly; LDAP-volume techniques (recon/BloodHound) overfit or false-positive more.
pub fn grade_rule(seed: u64, t: Technique, round: u32) -> &'static str {
    let mut z = seed
        ^ (t.value() as u64).wrapping_mul(0x9E3779B97F4A7C15)
        ^ (round as u64).wrapping_mul(0x100000001B3);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    let r = (z ^ (z >> 31)) % 100;
    match t {
        // crisp invariant → usually robust
        Technique::Kerberoast | Technique::AsRepRoast | Technique::LateralMove => {
            if r < 75 { "robust" } else if r < 90 { "overfit" } else { "noisy" }
        }
        // LDAP-heavy / high-volume → easy to overfit or drown in benign traffic
        _ => {
            if r < 45 { "robust" } else if r < 78 { "overfit" } else { "noisy" }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub round: u32,
    pub technique: Technique,
    pub source: String,
    pub rule_id: String,
    pub level: u8,
}

/// One red technique firing = one detection opportunity. `detected_round`/`source` are set the
/// moment blue actually catches it (a deployed rule, baseline telemetry, or a real Wazuh alert) —
/// never by fiat. This is the raw material of the coverage report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackEvent {
    pub technique: Technique,
    pub round: u32,
    pub detected_round: Option<u32>,
    pub source: Option<String>,
}

/// One row of the detection-coverage report: per technique, how many times red fired it, how
/// often blue caught it, whether it's an outright gap, by what, and how fast (rounds).
#[derive(Debug, Clone, Default, Serialize)]
pub struct CoverageRow {
    pub technique: String,
    pub attack_id: String,
    pub attack_name: String,
    pub data_source: String,
    pub fired: u32,
    pub detected: u32,
    pub gap: bool,
    pub source: String,
    pub latency: Option<i64>,
    /// Quality of the WRITTEN rule that caught it (empty for baseline/hunt/wazuh): robust|overfit|noisy.
    pub fidelity: String,
}

/// The report a defender actually acts on: coverage %, mean time-to-detect, the gap list, and —
/// the honest number — ROBUST coverage (only detections that would generalize), with the overfit
/// and noisy rules called out for refinement.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Coverage {
    pub rows: Vec<CoverageRow>,
    pub techniques_fired: u32,
    pub techniques_detected: u32,
    pub coverage_pct: u32,
    pub robust_pct: u32,
    pub gaps: Vec<String>,
    pub overfit: Vec<String>,
    pub noisy: Vec<String>,
    pub mttd_rounds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundScore {
    pub round: u32,
    pub red_delta: i32,
    pub blue_delta: i32,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scoreboard {
    pub red: i32,
    pub blue: i32,
    pub rounds: Vec<RoundScore>,
}

/// The whole game state. Serializable so a round can be snapshotted, replayed, or shown
/// on the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub round: u32,
    pub hosts: Vec<Host>,
    pub creds: Vec<Cred>,
    pub detections: Vec<Detection>,
    pub alerts: Vec<Alert>,
    pub scoreboard: Scoreboard,
    pub red_reached_da: bool,
    /// The name of the win condition red satisfied (feed/report flavor).
    pub win_reason: String,
    pub performed: Vec<Technique>,
    pub honeytokens: u32,
    /// Every red technique firing + whether/when blue detected it — drives the coverage report.
    pub attacks: Vec<AttackEvent>,

    // ── blue's active-defense posture (the "pitbull") ──
    /// Continuous monitoring online: red's actions are seen even without a specific rule.
    pub monitoring: bool,
    /// Auto-containment armed: a detected theft is rotated the same round (the bite).
    pub auto_response: bool,
    /// RC4 Kerberos disabled / AES enforced — Kerberoast tickets are no longer crackable.
    pub rc4_disabled: bool,
    /// Kerberos pre-auth enforced — AS-REP roasting yields nothing.
    pub preauth_required: bool,
    /// The Helpdesk->GenericAll->DA path removed / admins tiered — the escalation is gone.
    pub acl_path_fixed: bool,

    // ── the scenario: this match's environment (see `scenario` module) ──
    pub scenario: String,
    pub seed: u64,
    /// Attack paths actually planted this match — a red technique only succeeds if its vuln exists.
    pub misconfigs: Vec<Technique>,
    /// Techniques default telemetry catches with no bespoke rule (the shop's EDR/SOC maturity).
    pub baseline: Vec<Technique>,

    // ── topology (deep-#4): red's position + the segmentation graph the preconditions consult ──
    /// Zones red holds a foothold in — starts at the external "internet" edge. AD attacks are
    /// gated on red having traversed to a zone that can reach the objective, so segmentation matters.
    pub red_zones: Vec<String>,
    /// The DC's zone. Attacks against AD require reachability to it.
    pub objective_zone: String,
    /// Directed reachability edges (firewall/segmentation) — `(from, to)`.
    pub edges: Vec<(String, String)>,
    /// The full entry→objective zone chain at match start — the immutable layout for the UI.
    /// Held separately from `edges` so blue cutting an edge doesn't erase the front's shape.
    pub zone_path: Vec<String>,
}

impl GameState {
    pub fn new(hosts: Vec<Host>) -> Self {
        Self {
            round: 0,
            hosts,
            creds: vec![],
            detections: vec![],
            alerts: vec![],
            scoreboard: Scoreboard::default(),
            red_reached_da: false,
            win_reason: String::new(),
            performed: vec![],
            honeytokens: 0,
            attacks: vec![],
            monitoring: false,
            auto_response: false,
            rc4_disabled: false,
            preauth_required: false,
            acl_path_fixed: false,
            // default scenario = the classic weak-svc lab (overridden per match via `apply_scenario`)
            scenario: "flat-net · weak service acct".into(),
            seed: 0,
            misconfigs: vec![Technique::Kerberoast, Technique::AsRepRoast, Technique::LateralMove],
            baseline: vec![Technique::LateralMove],
            red_zones: vec!["internet".into()],
            objective_zone: "vlan30".into(),
            edges: vec![("internet".into(), "vlan30".into())],
            zone_path: vec!["internet".into(), "vlan30".into()],
        }
    }

    // ── topology helpers (deep-#4) ──
    pub fn holds(&self, z: &str) -> bool {
        self.red_zones.iter().any(|x| x == z)
    }
    fn edge(&self, from: &str, to: &str) -> bool {
        self.edges.iter().any(|(f, t)| f == from && t == to)
    }
    /// Red has a foothold beyond the external edge.
    pub fn has_internal(&self) -> bool {
        self.red_zones.iter().any(|z| z != "internet")
    }
    /// Red is positioned to attack AD: it holds an internal zone that is, or can reach, the objective.
    pub fn attack_ready(&self) -> bool {
        self.red_zones.iter().any(|z| z != "internet" && (z == &self.objective_zone || self.edge(z, &self.objective_zone)))
    }
    /// Zones red can reach from where it stands but doesn't yet hold (the next hops to pivot into).
    pub fn next_hops(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (f, t) in &self.edges {
            if self.holds(f) && !self.holds(t) && !out.contains(t) {
                out.push(t.clone());
            }
        }
        out
    }
    pub fn add_zone(&mut self, z: &str) {
        if !self.holds(z) {
            self.red_zones.push(z.to_string());
        }
    }

    /// Is the attack path for technique `t` planted in this scenario? (red only succeeds if so)
    pub fn vuln(&self, t: Technique) -> bool {
        self.misconfigs.contains(&t)
    }
    /// Does default telemetry catch `t` without a bespoke rule? (this scenario's EDR maturity)
    pub fn baseline_covers(&self, t: Technique) -> bool {
        self.baseline.contains(&t)
    }

    pub fn has_detection(&self, t: Technique) -> bool {
        self.detections.iter().any(|d| d.technique == t && d.technique_based)
    }
    pub fn has_cracked_cred(&self) -> bool {
        self.creds.iter().any(|c| c.cracked)
    }
    pub fn performed_technique(&self, t: Technique) -> bool {
        self.performed.contains(&t)
    }
    pub fn has_foothold(&self) -> bool {
        self.hosts.iter().any(|h| h.foothold)
    }
    /// Blue "knows about" technique `t` once it has an alert for it. Continuous monitoring
    /// is what *generates* those alerts (see the referee) — so turning the pitbull on is
    /// what lets blue then remediate the specific vectors it observes.
    pub fn blue_knows(&self, t: Technique) -> bool {
        self.alerts.iter().any(|a| a.technique == t)
    }

    /// Record a red technique firing (a detection opportunity), initially undetected.
    pub fn record_attack(&mut self, t: Technique) {
        self.attacks.push(AttackEvent { technique: t, round: self.round, detected_round: None, source: None });
    }
    /// Mark the most recent still-undetected firing of `t` as caught this round, by `source`.
    /// Returns true if it flipped something (i.e. this was a real, first detection → counts for MTTD).
    pub fn mark_detected(&mut self, t: Technique, source: &str) -> bool {
        if let Some(a) = self.attacks.iter_mut().rev().find(|a| a.technique == t && a.detected_round.is_none()) {
            a.detected_round = Some(self.round);
            a.source = Some(source.to_string());
            return true;
        }
        false
    }
    /// Aggregate the attack log into the coverage report.
    pub fn coverage(&self) -> Coverage {
        use std::collections::BTreeMap;
        // key -> (fired, detected, latencies, first source)
        let mut m: BTreeMap<&'static str, (u32, u32, Vec<i64>, String)> = BTreeMap::new();
        for a in &self.attacks {
            let e = m.entry(a.technique.as_key()).or_default();
            e.0 += 1;
            if let Some(dr) = a.detected_round {
                e.1 += 1;
                e.2.push(dr as i64 - a.round as i64);
                if e.3.is_empty() {
                    if let Some(s) = &a.source { e.3 = s.clone(); }
                }
            }
        }
        // rule fidelity per technique (only WRITTEN rules have a grade)
        let mut fid: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for d in &self.detections {
            fid.insert(d.technique.as_key(), if d.fidelity.is_empty() { "robust" } else { d.fidelity.as_str() });
        }
        let mut rows = Vec::new();
        let (mut fired_t, mut det_t, mut robust_t) = (0u32, 0u32, 0u32);
        let mut gaps = Vec::new();
        let mut overfit = Vec::new();
        let mut noisy = Vec::new();
        let mut all_lat: Vec<i64> = Vec::new();
        for (k, (fired, detected, lats, src)) in m {
            fired_t += 1;
            let gap = detected == 0;
            // If blue WROTE a rule for this technique, its grade governs (overfit/noisy don't
            // generalize). Otherwise it was caught by baseline/hunt/wazuh — a real observation.
            let rule_fid = fid.get(k).copied();
            let fidelity = rule_fid.unwrap_or("");
            let robust = !gap && rule_fid.map(|f| f == "robust").unwrap_or(true);
            if gap {
                gaps.push(k.to_string());
            } else {
                det_t += 1;
                if robust { robust_t += 1; }
                match rule_fid {
                    Some("overfit") => overfit.push(k.to_string()),
                    Some("noisy") => noisy.push(k.to_string()),
                    _ => {}
                }
            }
            all_lat.extend(&lats);
            let t = Technique::from_key(k);
            rows.push(CoverageRow {
                technique: k.to_string(),
                attack_id: t.map(|t| t.attack_id()).unwrap_or("").to_string(),
                attack_name: t.map(|t| t.attack_name()).unwrap_or("").to_string(),
                data_source: t.map(|t| t.data_source()).unwrap_or("").to_string(),
                fired,
                detected,
                gap,
                source: if gap { "—".into() } else if rule_fid.is_some() { "rule".into() } else if src.is_empty() { "detected".into() } else { src },
                latency: lats.iter().min().copied(),
                fidelity: fidelity.to_string(),
            });
        }
        let coverage_pct = if fired_t > 0 { det_t * 100 / fired_t } else { 0 };
        let robust_pct = if fired_t > 0 { robust_t * 100 / fired_t } else { 0 };
        let mttd_rounds = if all_lat.is_empty() { None } else { Some(all_lat.iter().sum::<i64>() as f64 / all_lat.len() as f64) };
        Coverage { rows, techniques_fired: fired_t, techniques_detected: det_t, coverage_pct, robust_pct, gaps, overfit, noisy, mttd_rounds }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_technique_maps_to_an_attack_category() {
        use Technique::*;
        use crate::category::Category;
        assert_eq!(Pivot.category(), Category::LateralMovement);
        assert_eq!(LateralMove.category(), Category::CredentialAccess);
        assert_eq!(Recon.category(), Category::Discovery);
        assert_eq!(BloodHound.category(), Category::Discovery);
        assert_eq!(Kerberoast.category(), Category::CredentialAccess);
        assert_eq!(InitialAccess.category(), Category::InitialAccess);
        for t in [InitialAccess, Recon, Pivot, Kerberoast, AsRepRoast, BloodHound, CredSpray, LateralMove, Exfil] {
            assert!(!t.category().is_defensive(), "attack technique must map to an attack category");
        }
    }
}
