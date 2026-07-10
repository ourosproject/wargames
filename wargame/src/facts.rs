//! Engagement facts — the true/false checker over game state.
//!
//! A small, orthogonal alphabet of named truths about the engagement: does red hold a
//! foothold, has it scouted, is the path to DA mapped, does it control the DC; is blue
//! monitoring, has it severed the path, hardened the vectors. Each fact is ONE pure
//! predicate over [`GameState`] — the scattered ad-hoc booleans (`attack_ready`,
//! `has_cracked_cred`, `blue_knows(..)`, `acl_path_fixed`) pulled into one shared vocabulary.
//!
//! Why this exists (the node-builder / forest direction): a card's precondition is really
//! "which facts must hold," and the thin model plays better when it can see *why* a move is
//! legal and what it unlocks — not just pick blindly from a flat menu. Surfacing this table
//! into the prompt is the first concrete step. It is also fog-of-war safe: [`Fact::audience`]
//! keeps red's private progress out of blue's table and blue's posture out of red's, so the
//! checker never leaks the other side's ground truth.

use serde::{Deserialize, Serialize};

use crate::state::{GameState, Side, Technique};

/// A named, boolean truth about the engagement. Orthogonal by construction — each answers a
/// single yes/no question a player would actually ask before choosing a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Fact {
    // ── red's attack progress (red-private: its own ground truth) ──
    /// Red holds a foothold beyond the external edge.
    Foothold,
    /// Red is positioned to attack AD — it holds/reaches the objective zone.
    ReachesDc,
    /// Red has run domain/graph discovery (recon or bloodhound).
    Scouted,
    /// Red has mapped the concrete route to Domain Admin (bloodhound done).
    PathMapped,
    /// Red holds a cracked domain credential.
    HasCred,
    /// Red controls the DC (Domain Admin reached).
    DomainAdmin,

    // ── blue's defensive posture (blue-private) ──
    /// Continuous monitoring is online.
    Monitoring,
    /// Auto-containment is armed.
    AutoResponse,
    /// The DCSync/GenericAll path to DA has been severed.
    PathSevered,
    /// RC4 disabled / AES enforced — Kerberoast tickets are junk.
    AesEnforced,
    /// Kerberos pre-auth enforced — AS-REP roasting yields nothing.
    PreauthEnforced,

    // ── blue's observations (blue-private: what it has actually detected) ──
    /// Blue has detected the scouting (recon/bloodhound) — this is what unlocks `remediate_acl`.
    ScoutDetected,
    /// Blue has detected roasting activity (kerberoast/as-rep).
    RoastDetected,
    /// Blue has detected the intrusion itself (initial access / pivot).
    IntrusionDetected,
}

/// One row of a side-appropriate fact table: the fact, the yes/no question it answers, and
/// whether it currently holds. Serializable so it drops straight into the model prompt / UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactRow {
    pub fact: String,
    pub question: String,
    pub holds: bool,
}

impl Fact {
    /// Every fact, in reading order (attack chain, then posture, then observations).
    pub const ALL: [Fact; 14] = [
        Fact::Foothold,
        Fact::ReachesDc,
        Fact::Scouted,
        Fact::PathMapped,
        Fact::HasCred,
        Fact::DomainAdmin,
        Fact::Monitoring,
        Fact::AutoResponse,
        Fact::PathSevered,
        Fact::AesEnforced,
        Fact::PreauthEnforced,
        Fact::ScoutDetected,
        Fact::RoastDetected,
        Fact::IntrusionDetected,
    ];

    /// Stable slug — the key the prompt / registry / builder refer to a fact by.
    pub fn key(&self) -> &'static str {
        match self {
            Fact::Foothold => "foothold",
            Fact::ReachesDc => "reaches_dc",
            Fact::Scouted => "scouted",
            Fact::PathMapped => "path_mapped",
            Fact::HasCred => "has_cred",
            Fact::DomainAdmin => "domain_admin",
            Fact::Monitoring => "monitoring",
            Fact::AutoResponse => "auto_response",
            Fact::PathSevered => "path_severed",
            Fact::AesEnforced => "aes_enforced",
            Fact::PreauthEnforced => "preauth_enforced",
            Fact::ScoutDetected => "scout_detected",
            Fact::RoastDetected => "roast_detected",
            Fact::IntrusionDetected => "intrusion_detected",
        }
    }

    /// The yes/no question this fact answers — the human phrasing the model reads.
    pub fn question(&self) -> &'static str {
        match self {
            Fact::Foothold => "Does red hold an internal foothold?",
            Fact::ReachesDc => "Can red reach the domain controller from where it stands?",
            Fact::Scouted => "Has red scouted the domain (recon/bloodhound)?",
            Fact::PathMapped => "Has red mapped the concrete path to Domain Admin?",
            Fact::HasCred => "Does red hold a cracked credential?",
            Fact::DomainAdmin => "Does red control the DC (Domain Admin)?",
            Fact::Monitoring => "Is continuous monitoring online?",
            Fact::AutoResponse => "Is auto-containment armed?",
            Fact::PathSevered => "Has the path to Domain Admin been severed?",
            Fact::AesEnforced => "Is AES enforced (Kerberoast neutralized)?",
            Fact::PreauthEnforced => "Is Kerberos pre-auth enforced (AS-REP neutralized)?",
            Fact::ScoutDetected => "Have you detected red's scouting yet?",
            Fact::RoastDetected => "Have you detected roasting activity yet?",
            Fact::IntrusionDetected => "Have you detected the intrusion (breach/pivot) yet?",
        }
    }

    /// Which side may see this fact. Red's attack-progress is red-private ground truth; blue's
    /// posture and observations are blue-private. This is the fog-of-war boundary — a side's
    /// table is exactly the facts it legitimately knows, so nothing leaks across.
    pub fn audience(&self) -> Side {
        match self {
            Fact::Foothold
            | Fact::ReachesDc
            | Fact::Scouted
            | Fact::PathMapped
            | Fact::HasCred
            | Fact::DomainAdmin => Side::Red,
            _ => Side::Blue,
        }
    }

    /// Evaluate the fact against ground truth. Pure — reads state, never mutates.
    pub fn holds(&self, s: &GameState) -> bool {
        match self {
            Fact::Foothold => s.has_internal(),
            Fact::ReachesDc => s.attack_ready(),
            Fact::Scouted => {
                s.performed_technique(Technique::Recon) || s.performed_technique(Technique::BloodHound)
            }
            Fact::PathMapped => s.performed_technique(Technique::BloodHound),
            Fact::HasCred => s.has_cracked_cred(),
            Fact::DomainAdmin => s.red_reached_da,
            Fact::Monitoring => s.monitoring,
            Fact::AutoResponse => s.auto_response,
            Fact::PathSevered => s.acl_path_fixed,
            Fact::AesEnforced => s.rc4_disabled,
            Fact::PreauthEnforced => s.preauth_required,
            Fact::ScoutDetected => {
                s.blue_knows(Technique::Recon) || s.blue_knows(Technique::BloodHound)
            }
            Fact::RoastDetected => {
                s.blue_knows(Technique::Kerberoast) || s.blue_knows(Technique::AsRepRoast)
            }
            Fact::IntrusionDetected => {
                s.blue_knows(Technique::InitialAccess) || s.blue_knows(Technique::Pivot)
            }
        }
    }

    fn row(&self, s: &GameState) -> FactRow {
        FactRow { fact: self.key().to_string(), question: self.question().to_string(), holds: self.holds(s) }
    }
}

/// The fact table `side` legitimately knows this turn — its own private facts, evaluated
/// against ground truth. Fog-of-war safe: red never sees blue's posture and vice versa.
pub fn table_for(side: Side, s: &GameState) -> Vec<FactRow> {
    Fact::ALL.iter().filter(|f| f.audience() == side).map(|f| f.row(s)).collect()
}

/// A ground-truth legality gate that is NEVER surfaced to a model. This is Tier-2: the place
/// instance-specific gates (`Detected(Kerberoast)` — the precise counter fingerprint) live,
/// alongside topology/aggregate gates that must not leak into an agent's fact table
/// (`UndetectedActivity` would tell Blue hidden work exists). Keeping these out of `Fact::ALL`
/// is why the surfaced fact table is unchanged by this refactor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InstanceProbe {
    /// Red has already performed this exact technique (once-only guards).
    Performed(Technique),
    /// Blue has fingerprinted this exact technique (the precise-counter gate).
    Detected(Technique),
    /// Red has a forward hop to pivot/breach into.
    HasForwardPath,
    /// A DCSync-able ACL path exists in this scenario (the escalation misconfig).
    LateralPathPlanted,
    /// There is a cracked credential whose acquiring technique Blue has detected.
    CredCompromiseKnown,
    /// Some technique Red performed is not yet visible to Blue (a coverage gap to hunt).
    UndetectedActivity,
    /// Some alert Blue holds has no technique-based detection rule yet.
    UndetectedAlert,
}

impl InstanceProbe {
    pub fn holds(&self, s: &GameState) -> bool {
        match self {
            InstanceProbe::Performed(t) => s.performed_technique(*t),
            InstanceProbe::Detected(t) => s.blue_knows(*t),
            InstanceProbe::HasForwardPath => !s.next_hops().is_empty(),
            InstanceProbe::LateralPathPlanted => s.vuln(Technique::LateralMove),
            InstanceProbe::CredCompromiseKnown => s.creds.iter().any(|c| c.cracked && s.blue_knows(c.via)),
            InstanceProbe::UndetectedActivity => s.performed.iter().any(|t| !s.blue_knows(*t)),
            InstanceProbe::UndetectedAlert => s.alerts.iter().any(|a| !s.has_detection(a.technique)),
        }
    }
}

/// A card's legality expressed as data. Tier-1 `Category` requirements gate on a surfaced
/// [`Fact`]; Tier-2 `Instance` requirements gate on a ground-truth [`InstanceProbe`]. A card
/// is legal when all its requirements are satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Requirement {
    Category { fact: Fact, want: bool },
    Instance { probe: InstanceProbe, want: bool },
}

impl Requirement {
    pub fn have(fact: Fact) -> Self { Requirement::Category { fact, want: true } }
    pub fn lack(fact: Fact) -> Self { Requirement::Category { fact, want: false } }
    pub fn did(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Performed(t), want: true } }
    pub fn not_yet(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Performed(t), want: false } }
    pub fn fingerprinted(t: Technique) -> Self { Requirement::Instance { probe: InstanceProbe::Detected(t), want: true } }
    pub fn probe(p: InstanceProbe) -> Self { Requirement::Instance { probe: p, want: true } }
    pub fn no_probe(p: InstanceProbe) -> Self { Requirement::Instance { probe: p, want: false } }

    pub fn satisfied(&self, s: &GameState) -> bool {
        match self {
            Requirement::Category { fact, want } => fact.holds(s) == *want,
            Requirement::Instance { probe, want } => probe.holds(s) == *want,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Cred, GameState, Host};

    fn base() -> GameState {
        GameState::new(vec![Host {
            id: "edge".into(),
            zone: "internet".into(),
            label: "edge".into(),
            foothold: false,
            reachable_by_red: true,
        }])
    }

    #[test]
    fn fresh_state_has_no_progress_and_no_posture() {
        let s = base();
        for f in Fact::ALL {
            assert!(!f.holds(&s), "{} should be false at start", f.key());
        }
    }

    #[test]
    fn foothold_and_reaches_track_red_position() {
        let mut s = base();
        assert!(!Fact::Foothold.holds(&s));
        s.add_zone("vlan30"); // objective zone by default → internal AND reaches DC
        assert!(Fact::Foothold.holds(&s));
        assert!(Fact::ReachesDc.holds(&s));
    }

    #[test]
    fn scouted_needs_recon_or_bloodhound() {
        let mut s = base();
        assert!(!Fact::Scouted.holds(&s));
        s.performed.push(Technique::Recon);
        assert!(Fact::Scouted.holds(&s));
        assert!(!Fact::PathMapped.holds(&s), "recon alone does not map the DA path");
        s.performed.push(Technique::BloodHound);
        assert!(Fact::PathMapped.holds(&s));
    }

    #[test]
    fn domain_admin_tracks_dc_control() {
        let mut s = base();
        assert!(!Fact::DomainAdmin.holds(&s));
        s.red_reached_da = true;
        assert!(Fact::DomainAdmin.holds(&s));
    }

    #[test]
    fn has_cred_tracks_cracked_creds() {
        let mut s = base();
        assert!(!Fact::HasCred.holds(&s));
        s.creds.push(Cred {
            principal: "svc".into(),
            secret: None,
            cracked: true,
            via: Technique::Kerberoast,
        });
        assert!(Fact::HasCred.holds(&s));
    }

    #[test]
    fn scout_detected_needs_a_blue_alert_not_just_red_action() {
        let mut s = base();
        s.performed.push(Technique::Recon);
        assert!(Fact::Scouted.holds(&s), "red DID scout");
        assert!(!Fact::ScoutDetected.holds(&s), "but blue has not seen it yet");
        s.alerts.push(crate::state::Alert {
            round: 1,
            technique: Technique::Recon,
            source: "hunt".into(),
            rule_id: "r".into(),
            level: 8,
        });
        assert!(Fact::ScoutDetected.holds(&s), "now blue has an alert for it");
    }

    #[test]
    fn tables_respect_fog_of_war() {
        let s = base();
        let red = table_for(Side::Red, &s);
        let blue = table_for(Side::Blue, &s);
        // red sees its own progress, never blue posture/observations
        assert!(red.iter().any(|r| r.fact == "foothold"));
        assert!(red.iter().all(|r| r.fact != "monitoring" && r.fact != "scout_detected"));
        // blue sees posture + observations, never red's private progress
        assert!(blue.iter().any(|r| r.fact == "monitoring"));
        assert!(blue.iter().any(|r| r.fact == "scout_detected"));
        assert!(blue.iter().all(|r| r.fact != "foothold" && r.fact != "has_cred"));
        // partition is total
        assert_eq!(red.len() + blue.len(), Fact::ALL.len());
    }

    #[test]
    fn instance_probe_performed_and_detected() {
        let mut s = base();
        assert!(!InstanceProbe::Performed(Technique::Recon).holds(&s));
        s.performed.push(Technique::Recon);
        assert!(InstanceProbe::Performed(Technique::Recon).holds(&s));
        assert!(!InstanceProbe::Detected(Technique::Recon).holds(&s), "performed != detected");
        s.alerts.push(crate::state::Alert { round: 1, technique: Technique::Recon, source: "m".into(), rule_id: "r".into(), level: 8 });
        assert!(InstanceProbe::Detected(Technique::Recon).holds(&s));
    }

    #[test]
    fn requirement_category_and_instance_respect_want() {
        let mut s = base();
        // ReachesDc false at start → have(ReachesDc) unsatisfied, lack(ReachesDc) satisfied
        assert!(!Requirement::have(Fact::ReachesDc).satisfied(&s));
        assert!(Requirement::lack(Fact::ReachesDc).satisfied(&s));
        // instance: not_yet(Recon) satisfied until performed
        assert!(Requirement::not_yet(Technique::Recon).satisfied(&s));
        s.performed.push(Technique::Recon);
        assert!(!Requirement::not_yet(Technique::Recon).satisfied(&s));
        assert!(Requirement::did(Technique::Recon).satisfied(&s));
    }

    #[test]
    fn undetected_probes_track_gaps() {
        let mut s = base();
        assert!(!InstanceProbe::UndetectedActivity.holds(&s));
        s.performed.push(Technique::Kerberoast);
        assert!(InstanceProbe::UndetectedActivity.holds(&s), "performed but unseen");
        s.alerts.push(crate::state::Alert { round: 1, technique: Technique::Kerberoast, source: "m".into(), rule_id: "r".into(), level: 8 });
        assert!(!InstanceProbe::UndetectedActivity.holds(&s), "now seen");
        assert!(InstanceProbe::UndetectedAlert.holds(&s), "alert has no detection rule yet");
    }
}
