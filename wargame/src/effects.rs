//! The write-half of the alphabet: the fixed set of state changes a move step can make.
//! Each `Effect` reproduces exactly one old `play()` body, so converting a move to data
//! cannot change behavior.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::graph::Context;
use crate::state::{grade_rule, Alert, Cred, Detection, GameState, Technique};

/// The on/off switches the game tracks. Each maps to one boolean on `GameState` — blue defenses
/// and red posture/objectives alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateFlag {
    Monitoring,
    AutoResponse,
    PathSevered,
    AesEnforced,
    PreauthEnforced,
    DomainAdmin,
    // ── new: red objectives / posture + blue counters ──
    DataExfiltrated,
    ImpactDone,
    Persisted,
    C2Established,
    EgressBlocked,
    BackupsReady,
    C2Blocked,
}

impl StateFlag {
    /// Stable slug for this flag (matches the surfaced fact key it flips).
    pub fn key(&self) -> &'static str {
        match self {
            StateFlag::Monitoring => "monitoring",
            StateFlag::AutoResponse => "auto_response",
            StateFlag::PathSevered => "path_severed",
            StateFlag::AesEnforced => "aes_enforced",
            StateFlag::PreauthEnforced => "preauth_enforced",
            StateFlag::DomainAdmin => "domain_admin",
            // ── arsenal primitives expansion: objective / posture flags (key = the fact each flips) ──
            StateFlag::Persisted => "persisted",
            StateFlag::C2Established => "c2_active",
            StateFlag::DataExfiltrated => "data_exfiltrated",
            StateFlag::ImpactDone => "impact_done",
            StateFlag::EgressBlocked => "egress_blocked",
            StateFlag::BackupsReady => "backups_ready",
            StateFlag::C2Blocked => "c2_blocked",
        }
    }

    /// Inverse of `key()`.
    pub fn from_key(k: &str) -> Option<StateFlag> {
        Some(match k {
            "monitoring" => StateFlag::Monitoring,
            "auto_response" => StateFlag::AutoResponse,
            "path_severed" => StateFlag::PathSevered,
            "aes_enforced" => StateFlag::AesEnforced,
            "preauth_enforced" => StateFlag::PreauthEnforced,
            "domain_admin" => StateFlag::DomainAdmin,
            "persisted" => StateFlag::Persisted,
            "c2_active" => StateFlag::C2Established,
            "data_exfiltrated" => StateFlag::DataExfiltrated,
            "impact_done" => StateFlag::ImpactDone,
            "egress_blocked" => StateFlag::EgressBlocked,
            "backups_ready" => StateFlag::BackupsReady,
            "c2_blocked" => StateFlag::C2Blocked,
            _ => return None,
        })
    }

    fn set(&self, s: &mut GameState) {
        match self {
            StateFlag::Monitoring => s.monitoring = true,
            StateFlag::AutoResponse => s.auto_response = true,
            StateFlag::PathSevered => s.acl_path_fixed = true,
            StateFlag::AesEnforced => s.rc4_disabled = true,
            StateFlag::PreauthEnforced => s.preauth_required = true,
            StateFlag::DomainAdmin => s.red_reached_da = true,
            StateFlag::DataExfiltrated => s.data_exfiltrated = true,
            StateFlag::ImpactDone => s.impact_done = true,
            StateFlag::Persisted => s.red_persisted = true,
            StateFlag::C2Established => s.c2_established = true,
            StateFlag::EgressBlocked => s.egress_blocked = true,
            StateFlag::BackupsReady => s.backups_ready = true,
            StateFlag::C2Blocked => s.c2_blocked = true,
        }
    }
}

/// The one thing a move step does. Every variant reproduces exactly one old `play()` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// Perform the technique but change no state (recon, bloodhound). The fact left behind
    /// (scouted / path mapped) comes from the referee recording `technique()`, not from here.
    Attempt,
    /// Take the next forward hop. `ok_narrative` may contain `{dest}` (replaced with the zone).
    Advance,
    /// Push a cracked credential.
    GrantCred { principal: String, secret: Option<String>, via: Technique },
    /// Flip one on/off switch true.
    SetFlag(StateFlag),
    /// Cancel any cracked credentials whose acquiring technique the defender has detected.
    RevokeKnownCreds,
    /// Surface the highest-value performed-but-undetected technique (records an alert + detection).
    HuntGap,
    /// Write a graded detection rule for `params.technique`.
    DeployDetection,
    /// Cut every edge out of a zone red holds into one it doesn't.
    SeverForwardEdges,
    /// Blue kicks red out: burns a persistent implant first, else removes red's deepest zone.
    Evict,
    /// Write a value to the blackboard (kerberoast's early steps pass values along).
    Produce { key: String, value: Value },
}

/// What applying an effect reports back: did it succeed, and (when the message is computed
/// from state) the narrative to show. `None` narrative means "let the step's default text stand".
pub struct EffectResult {
    pub success: bool,
    pub narrative: Option<String>,
}

impl Effect {
    /// Apply the effect. `env_success` is the environment's result (always true in sim);
    /// `env_narrative` is the environment's message (empty in sim); `ok_narrative` is the
    /// step's default success text. The narrative precedence reproduces the old `realize()`
    /// helper: an environment message wins, else the effect's own / the step's default text.
    pub fn apply(
        &self,
        state: &mut GameState,
        ctx: &mut Context,
        params: &Value,
        env_success: bool,
        env_narrative: &str,
        ok_narrative: &str,
    ) -> EffectResult {
        match self {
            Effect::Attempt => EffectResult { success: env_success, narrative: None },

            Effect::Advance => {
                let dest = state.next_hops().into_iter().next();
                if env_success {
                    if let Some(z) = &dest {
                        state.add_zone(z);
                    }
                }
                let d = dest.unwrap_or_default();
                let fallback = ok_narrative.replace("{dest}", &d);
                let narrative = if env_narrative.trim().is_empty() { fallback } else { env_narrative.to_string() };
                EffectResult { success: env_success, narrative: Some(narrative) }
            }

            Effect::GrantCred { principal, secret, via } => {
                if env_success {
                    state.creds.push(Cred { principal: principal.clone(), secret: secret.clone(), cracked: true, via: *via });
                }
                EffectResult { success: env_success, narrative: None }
            }

            Effect::SetFlag(f) => {
                if env_success {
                    f.set(state);
                }
                EffectResult { success: env_success, narrative: None }
            }

            Effect::RevokeKnownCreds => {
                let mut rotated = vec![];
                for i in 0..state.creds.len() {
                    if state.creds[i].cracked && state.blue_knows(state.creds[i].via) {
                        state.creds[i].cracked = false;
                        rotated.push(state.creds[i].principal.clone());
                    }
                }
                let narrative = if env_narrative.trim().is_empty() {
                    format!("rotated {} — those tickets are dead", rotated.join(", "))
                } else {
                    format!("{} ({})", env_narrative, rotated.join(", "))
                };
                EffectResult { success: !rotated.is_empty(), narrative: Some(narrative) }
            }

            Effect::HuntGap => {
                let target = state.performed.iter().copied().filter(|t| !state.blue_knows(*t)).max_by_key(|t| t.value());
                if let Some(t) = target {
                    state.alerts.push(Alert { round: state.round, technique: t, source: "hunt".into(), rule_id: "velociraptor-hunt".into(), level: 8 });
                    state.mark_detected(t, "hunt");
                    let dynamic = format!("threat hunt surfaced {} — gap closed", t.as_key());
                    let narrative = if env_narrative.trim().is_empty() { dynamic } else { env_narrative.to_string() };
                    EffectResult { success: true, narrative: Some(narrative) }
                } else {
                    EffectResult { success: false, narrative: Some("threat hunt — nothing new".into()) }
                }
            }

            Effect::DeployDetection => {
                let key = params.get("technique").and_then(|v| v.as_str()).unwrap_or("");
                match Technique::from_key(key) {
                    Some(t) => {
                        let fidelity = grade_rule(state.seed, t, state.round).to_string();
                        state.detections.push(Detection { id: format!("rule-{}", t.as_key()), technique: t, deployed_round: state.round, technique_based: true, fidelity });
                        let dynamic = format!("deployed detection for {}", t.as_key());
                        let narrative = if env_narrative.trim().is_empty() { dynamic } else { env_narrative.to_string() };
                        EffectResult { success: true, narrative: Some(narrative) }
                    }
                    None => EffectResult { success: false, narrative: Some("nothing observed to detect".into()) },
                }
            }

            Effect::SeverForwardEdges => {
                let cut = state.next_hops();
                let dynamic = format!("re-segmented — dropped red's path into {}", cut.join(", "));
                let narrative = if env_narrative.trim().is_empty() { dynamic } else { env_narrative.to_string() };
                if env_success {
                    let held = state.red_zones.clone();
                    state.edges.retain(|(f, t)| !(held.iter().any(|z| z == f) && !held.iter().any(|z| z == t)));
                }
                EffectResult { success: env_success, narrative: Some(narrative) }
            }

            Effect::Evict => {
                if !env_success {
                    return EffectResult { success: false, narrative: None };
                }
                if state.red_persisted {
                    // persistence absorbs one eviction — burn the implant, red keeps its ground
                    state.red_persisted = false;
                    EffectResult { success: true, narrative: Some("evicted the host — but an implant dug back in (persistence burned)".into()) }
                } else if let Some(i) = state.red_zones.iter().rposition(|z| z != "internet") {
                    let z = state.red_zones.remove(i);
                    EffectResult { success: true, narrative: Some(format!("evicted red from {z} — back on the wrong side of the wire")) }
                } else {
                    EffectResult { success: false, narrative: Some("nothing to evict — red holds no ground".into()) }
                }
            }

            Effect::Produce { key, value } => {
                ctx.set(key, value.clone());
                EffectResult { success: env_success, narrative: None }
            }
        }
    }
}

/// Palette metadata for the builder: each authorable effect (Produce is excluded — it is only
/// meaningful inside a multi-step chain, which the guided form does not build). `params` names
/// the fields the form must collect; `kind` tells the form which input to render.
pub fn effect_descriptors() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({ "key": "Attempt", "label": "Perform the technique (no state change)",
                "desc": "Just performs the technique; the referee records it (this is how recon/bloodhound leave 'scouted' behind).", "params": [] }),
        json!({ "key": "Advance", "label": "Move one zone closer",
                "desc": "Takes the next forward hop toward the objective. Use {dest} in the narrative for the zone name.", "params": [] }),
        json!({ "key": "SetFlag", "label": "Flip a defense/progress switch on",
                "desc": "Turns one on/off switch true (e.g. AES enforced, path severed, monitoring).",
                "params": [ { "name": "flag", "kind": "state_flag" } ] }),
        json!({ "key": "GrantCred", "label": "Steal a credential",
                "desc": "Adds a cracked credential.",
                "params": [ { "name": "principal", "kind": "text" }, { "name": "secret", "kind": "text", "optional": true }, { "name": "via", "kind": "technique" } ] }),
        json!({ "key": "RevokeKnownCreds", "label": "Cancel detected stolen credentials",
                "desc": "Cancels any cracked credential whose technique the defender has already detected. It decides which — no parameters.", "params": [] }),
        json!({ "key": "HuntGap", "label": "Threat-hunt for the top undetected technique",
                "desc": "Finds the highest-value technique the attacker performed but you haven't detected, and surfaces it. It picks the target itself — no parameters.", "params": [] }),
        json!({ "key": "DeployDetection", "label": "Write a detection rule",
                "desc": "Writes a graded detection rule for a technique (the move takes a 'technique' parameter at play time).", "params": [] }),
        json!({ "key": "SeverForwardEdges", "label": "Cut the network in front of the attacker",
                "desc": "Drops the attacker's forward network edges. It computes which — no parameters.", "params": [] }),
        json!({ "key": "Evict", "label": "Evict the attacker from a host",
                "desc": "Kicks the attacker off its deepest foothold — but a persistent implant absorbs the first eviction (burns the implant instead). It decides what to remove — no parameters.", "params": [] }),
        json!({ "key": "Produce", "label": "Produce a value for a later step",
                "desc": "Writes a value onto the move's blackboard under a key, so a later node can require it. This is how a multi-step move chains (enum → request → crack). Canvas moves only.",
                "params": [ { "name": "key", "kind": "text" }, { "name": "value", "kind": "text" } ] }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Cred, GameState, Host};

    fn base() -> GameState {
        GameState::new(vec![Host {
            id: "edge".into(), zone: "internet".into(), label: "edge".into(),
            foothold: false, reachable_by_red: true,
        }])
    }
    fn ctx() -> Context { Context::new() }

    #[test]
    fn set_flag_flips_the_right_boolean() {
        let mut s = base();
        let r = Effect::SetFlag(StateFlag::PathSevered).apply(&mut s, &mut ctx(), &Value::Null, true, "", "sev");
        assert!(r.success);
        assert!(s.acl_path_fixed);
    }

    #[test]
    fn set_flag_does_nothing_when_env_fails() {
        let mut s = base();
        let r = Effect::SetFlag(StateFlag::Monitoring).apply(&mut s, &mut ctx(), &Value::Null, false, "", "on");
        assert!(!r.success);
        assert!(!s.monitoring);
    }

    #[test]
    fn advance_takes_the_next_hop_and_templates_dest() {
        let mut s = base();
        // GameState::new seeds a default internet->vlan30 edge (pre-existing topology default,
        // unrelated to this test); clear it so the pushed edge is the only next hop.
        s.edges.clear();
        s.edges.push(("internet".into(), "vlan20".into()));
        let r = Effect::Advance.apply(&mut s, &mut ctx(), &Value::Null, true, "", "pivoted into {dest} — closer");
        assert!(s.holds("vlan20"));
        assert_eq!(r.narrative.as_deref(), Some("pivoted into vlan20 — closer"));
    }

    #[test]
    fn grant_cred_pushes_a_cracked_credential() {
        let mut s = base();
        let e = Effect::GrantCred { principal: "range.local\\svc".into(), secret: Some("pw".into()), via: Technique::Kerberoast };
        let r = e.apply(&mut s, &mut ctx(), &Value::Null, true, "", "cracked");
        assert!(r.success);
        assert_eq!(s.creds.len(), 1);
        assert!(s.creds[0].cracked);
        assert_eq!(s.creds[0].via, Technique::Kerberoast);
    }

    #[test]
    fn revoke_known_creds_only_cancels_detected_ones() {
        let mut s = base();
        s.creds.push(Cred { principal: "seen".into(), secret: None, cracked: true, via: Technique::Kerberoast });
        s.creds.push(Cred { principal: "unseen".into(), secret: None, cracked: true, via: Technique::AsRepRoast });
        s.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "m".into(), rule_id: "r".into(), level: 8 });
        let r = Effect::RevokeKnownCreds.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(r.success);
        assert!(!s.creds[0].cracked, "detected cred cancelled");
        assert!(s.creds[1].cracked, "undetected cred untouched");
    }

    #[test]
    fn revoke_known_creds_reports_failure_when_none_match() {
        let mut s = base();
        let r = Effect::RevokeKnownCreds.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(!r.success);
    }

    #[test]
    fn hunt_surfaces_the_highest_value_undetected_technique() {
        let mut s = base();
        s.performed.push(Technique::Recon);
        s.performed.push(Technique::Kerberoast); // higher value
        let r = Effect::HuntGap.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(r.success);
        assert!(s.blue_knows(Technique::Kerberoast));
        assert!(!s.blue_knows(Technique::Recon), "only the top target this turn");
    }

    #[test]
    fn deploy_detection_writes_a_graded_rule_from_params() {
        let mut s = base();
        let p = serde_json::json!({ "technique": "kerberoast" });
        let r = Effect::DeployDetection.apply(&mut s, &mut ctx(), &p, true, "", "");
        assert!(r.success);
        assert!(s.has_detection(Technique::Kerberoast));
    }

    #[test]
    fn sever_forward_edges_removes_red_frontier() {
        let mut s = base();
        s.add_zone("vlan20");
        s.edges.push(("vlan20".into(), "vlan30".into()));
        let r = Effect::SeverForwardEdges.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(r.success);
        assert!(s.next_hops().is_empty(), "forward edge cut");
    }

    #[test]
    fn produce_writes_to_the_blackboard() {
        let mut s = base();
        let mut c = ctx();
        let e = Effect::Produce { key: "tgs_hash".into(), value: serde_json::json!("$krb5tgs$") };
        e.apply(&mut s, &mut c, &Value::Null, true, "", "");
        assert_eq!(c.get("tgs_hash"), Some(&serde_json::json!("$krb5tgs$")));
    }

    #[test]
    fn state_flag_from_key_round_trips() {
        for (k, want) in [("monitoring", StateFlag::Monitoring), ("path_severed", StateFlag::PathSevered), ("domain_admin", StateFlag::DomainAdmin)] {
            assert_eq!(StateFlag::from_key(k), Some(want));
        }
        assert_eq!(StateFlag::from_key("nope"), None);
    }

    #[test]
    fn attempt_changes_no_state() {
        let mut s = base();
        let before = serde_json::to_value(&s).unwrap();
        let r = Effect::Attempt.apply(&mut s, &mut ctx(), &Value::Null, true, "", "did it");
        assert!(r.success);
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
    }

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
        s.add_zone("vlan20");
        s.add_zone("vlan30");
        s.red_persisted = true;
        // first evict burns the persistence; red keeps its ground
        Effect::Evict.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(!s.red_persisted, "first evict burns persistence");
        assert!(s.holds("vlan30"), "…but red still holds ground");
        // second evict removes the deepest held zone
        Effect::Evict.apply(&mut s, &mut ctx(), &Value::Null, true, "", "");
        assert!(!s.holds("vlan30"), "second evict kicks red back");
    }
}
