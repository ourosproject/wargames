//! Card library. Hybrid model (option C): `kerberoast` is a COMPOSITE built from
//! function-node primitives; the rest are code cards.
//!
//! RED keeps every ability. What changed is BLUE: it now has real active-defense powers.
//! Red's techniques simply stop paying off against a hardened, watching defender —
//! kerberoast against AES yields no crackable ticket, AS-REP against enforced pre-auth
//! yields nothing, and the escalation path can be removed outright.
//!
//! Every card realizes its effect through `env` ([`realize`] / `env.act`): in the simulator
//! that's a deterministic stand-in (unchanged game); on the live range it fires the real
//! command and the card only applies its game bookkeeping when the real action succeeds.

use serde_json::{json, Value};

use crate::card::{realize, Card, Environment, Outcome};
use crate::category::Category;
use crate::facts::{Fact, InstanceProbe, Requirement};
use crate::graph::{CompositeCard, Context, Primitive, PrimitiveResult};
use crate::registry::CardRegistry;
use crate::state::{grade_rule, Alert, Cred, Detection, GameState, Side, Technique};

/// Note text: prefer the environment's real detail, else the card's own flavor.
fn note(env_narr: &str, fallback: &str) -> String {
    if env_narr.trim().is_empty() { fallback.to_string() } else { env_narr.to_string() }
}

// ══ RED: kerberoast composite ════════════════════════════════════════════════════

struct EnumSpns;
impl Primitive for EnumSpns {
    fn id(&self) -> &'static str { "enum_spns" }
    fn describe(&self) -> &'static str { "Enumerate Kerberos SPNs" }
    fn produces(&self) -> Vec<&'static str> { vec!["spn_targets"] }
    fn run(&self, ctx: &mut Context, s: &mut GameState, e: &mut dyn Environment) -> PrimitiveResult {
        let o = e.act(self.id(), &json!({}), s);
        ctx.set("spn_targets", json!(["MSSQLSvc/dc01.range.local"]));
        PrimitiveResult { success: o.success, note: note(&o.narrative, "found svc_mssql"), detection_surface: vec![Technique::Recon] }
    }
}
struct RequestTgs;
impl Primitive for RequestTgs {
    fn id(&self) -> &'static str { "request_tgs" }
    fn describe(&self) -> &'static str { "Request a service ticket" }
    fn requires(&self) -> Vec<&'static str> { vec!["spn_targets"] }
    fn produces(&self) -> Vec<&'static str> { vec!["tgs_hash"] }
    fn run(&self, ctx: &mut Context, s: &mut GameState, e: &mut dyn Environment) -> PrimitiveResult {
        let o = e.act(self.id(), &json!({}), s);
        ctx.set("tgs_hash", json!("$krb5tgs$"));
        PrimitiveResult { success: o.success, note: note(&o.narrative, "got TGS"), detection_surface: vec![Technique::Kerberoast] }
    }
}
struct CrackHash;
impl Primitive for CrackHash {
    fn id(&self) -> &'static str { "crack_hash" }
    fn describe(&self) -> &'static str { "Crack the ticket offline" }
    fn requires(&self) -> Vec<&'static str> { vec!["tgs_hash"] }
    fn produces(&self) -> Vec<&'static str> { vec!["cracked_cred"] }
    fn run(&self, _ctx: &mut Context, state: &mut GameState, e: &mut dyn Environment) -> PrimitiveResult {
        if !state.vuln(Technique::Kerberoast) {
            return PrimitiveResult { success: false, note: "no roastable SPN in this environment".into(), detection_surface: vec![] };
        }
        if state.rc4_disabled {
            return PrimitiveResult { success: false, note: "AES enforced — ticket uncrackable".into(), detection_surface: vec![] };
        }
        let o = e.act(self.id(), &json!({}), state);
        if o.success {
            state.creds.push(Cred { principal: "range.local\\svc_mssql".into(), secret: Some("Summer2024!".into()), cracked: true, via: Technique::Kerberoast });
            PrimitiveResult { success: true, note: note(&o.narrative, "cracked: Summer2024!"), detection_surface: vec![] }
        } else {
            PrimitiveResult { success: false, note: note(&o.narrative, "crack failed"), detection_surface: vec![] }
        }
    }
}
fn kerberoast_card() -> CompositeCard {
    CompositeCard {
        id: "kerberoast",
        side: Side::Red,
        technique: Technique::Kerberoast,
        summary: "Kerberoast: enum SPNs -> request TGS -> crack (fails vs AES)",
        category: Category::CredentialAccess,
        // Can't roast the domain until red has crossed the network to reach it.
        requires: vec![Requirement::have(Fact::ReachesDc)],
        produces: vec![Fact::HasCred], // on a successful crack
        surface: vec![Technique::Recon, Technique::Kerberoast], // enum_spns + request_tgs
        nodes: vec![Box::new(CrackHash), Box::new(EnumSpns), Box::new(RequestTgs)],
    }
}

// ══ RED: external → internal traversal (deep-#4) ══════════════════════════════════
//
// Red no longer starts on the inside. It begins on the `internet` edge and must break in,
// then pivot through whatever segmentation the scenario planted before any AD attack is legal.

/// Break in from the outside — phish or exploit an edge service to land the first internal foothold.
struct InitialAccess;
impl Card for InitialAccess {
    fn id(&self) -> &'static str { "initial_access" }
    fn side(&self) -> Side { Side::Red }
    fn technique(&self) -> Technique { Technique::InitialAccess }
    fn describe(&self) -> &'static str { "Breach the perimeter (edge exploit / phish) for a first foothold" }
    fn category(&self) -> Category { Category::InitialAccess }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::lack(Fact::Foothold), Requirement::probe(InstanceProbe::HasForwardPath)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::Foothold] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::InitialAccess] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let hop = s.next_hops().into_iter().next();
        let o = realize(e, self.id(), p, s, "breached the perimeter — landed an internal foothold", vec![Technique::InitialAccess]);
        if o.success {
            if let Some(z) = hop {
                s.add_zone(&z);
            }
        }
        o
    }
}

/// Move deeper — pivot from a held zone into an adjacent one, crossing internal segmentation.
struct Pivot;
impl Card for Pivot {
    fn id(&self) -> &'static str { "pivot" }
    fn side(&self) -> Side { Side::Red }
    fn technique(&self) -> Technique { Technique::Pivot }
    fn describe(&self) -> &'static str { "Pivot across internal segmentation toward the domain" }
    fn category(&self) -> Category { Category::LateralMovement }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::have(Fact::Foothold), Requirement::lack(Fact::ReachesDc),
             Requirement::probe(InstanceProbe::HasForwardPath)]
    }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::Pivot] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let hop = s.next_hops().into_iter().next();
        let dest = hop.clone().unwrap_or_default();
        let o = realize(e, self.id(), p, s, &format!("pivoted into {} — one segment closer to the DC", dest), vec![Technique::Pivot]);
        if o.success {
            if let Some(z) = hop {
                s.add_zone(&z);
            }
        }
        o
    }
}

// ══ RED: code cards ══════════════════════════════════════════════════════════════

struct Recon;
impl Card for Recon {
    fn id(&self) -> &'static str { "recon" }
    fn side(&self) -> Side { Side::Red }
    fn technique(&self) -> Technique { Technique::Recon }
    fn describe(&self) -> &'static str { "Enumerate the AD estate" }
    fn category(&self) -> Category { Category::Discovery }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::have(Fact::ReachesDc), Requirement::not_yet(Technique::Recon)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::Scouted] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::Recon] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        realize(e, self.id(), p, s, "recon — mapped the domain", vec![Technique::Recon])
    }
}

struct AsRepRoast;
impl Card for AsRepRoast {
    fn id(&self) -> &'static str { "asrep_roast" }
    fn side(&self) -> Side { Side::Red }
    fn technique(&self) -> Technique { Technique::AsRepRoast }
    fn describe(&self) -> &'static str { "AS-REP roast a no-preauth user (fails if pre-auth enforced)" }
    fn category(&self) -> Category { Category::CredentialAccess }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::have(Fact::ReachesDc)] }
    fn produces(&self) -> Vec<Fact> { vec![Fact::HasCred] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::AsRepRoast] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        if !s.vuln(Technique::AsRepRoast) {
            return Outcome { success: false, narrative: "no AS-REP-roastable user in this environment".into(), detection_surface: vec![] };
        }
        if s.preauth_required {
            return Outcome { success: false, narrative: "AS-REP blocked — pre-auth enforced".into(), detection_surface: vec![Technique::AsRepRoast] };
        }
        let o = realize(e, self.id(), p, s, "AS-REP roast — cracked jbecker", vec![Technique::AsRepRoast]);
        if o.success {
            s.creds.push(Cred { principal: "range.local\\jbecker".into(), secret: Some("Baseball2023".into()), cracked: true, via: Technique::AsRepRoast });
        }
        o
    }
}

struct BloodHoundCollect;
impl Card for BloodHoundCollect {
    fn id(&self) -> &'static str { "bloodhound" }
    fn side(&self) -> Side { Side::Red }
    fn technique(&self) -> Technique { Technique::BloodHound }
    fn describe(&self) -> &'static str { "Collect the AD graph, find the path to DA" }
    fn category(&self) -> Category { Category::Discovery }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::have(Fact::HasCred), Requirement::not_yet(Technique::BloodHound)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::PathMapped, Fact::Scouted] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::BloodHound] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        realize(e, self.id(), p, s, "BloodHound — svc_mssql holds DCSync => krbtgt => DA", vec![Technique::BloodHound])
    }
}

struct EscalateDa;
impl Card for EscalateDa {
    fn id(&self) -> &'static str { "escalate_da" }
    fn side(&self) -> Side { Side::Red }
    fn technique(&self) -> Technique { Technique::LateralMove }
    fn describe(&self) -> &'static str { "Abuse the ACL path to Domain Admin (gone if remediated)" }
    fn category(&self) -> Category { Category::PrivilegeEscalation }
    fn requires(&self) -> Vec<Requirement> {
        vec![
            Requirement::probe(InstanceProbe::LateralPathPlanted),
            Requirement::have(Fact::HasCred),
            Requirement::have(Fact::PathMapped),
            Requirement::lack(Fact::DomainAdmin),
            Requirement::lack(Fact::PathSevered),
        ]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::DomainAdmin] }
    fn detection_surface(&self) -> Vec<Technique> { vec![Technique::LateralMove] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let o = realize(e, self.id(), p, s, "DCSync via svc_mssql -> dumped krbtgt -> DOMAIN ADMIN", vec![Technique::LateralMove]);
        if o.success {
            s.red_reached_da = true;
        }
        o
    }
}

// ══ BLUE: active defense (the pitbull) ════════════════════════════════════════════

/// Turn on the always-watching posture. Once online, red's actions are seen even without
/// a specific rule — this is what ends red's free stealth.
struct ContinuousMonitoring;
impl Card for ContinuousMonitoring {
    fn id(&self) -> &'static str { "monitor" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Recon }
    fn describe(&self) -> &'static str { "Bring continuous monitoring online (Velociraptor + Sysmon live)" }
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::lack(Fact::Monitoring)] }
    fn produces(&self) -> Vec<Fact> { vec![Fact::Monitoring] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let o = realize(e, self.id(), p, s, "continuous monitoring ONLINE — the range is now watched", vec![]);
        if o.success {
            s.monitoring = true;
        }
        o
    }
}

/// Arm auto-containment: any detected credential theft is rotated the same round.
struct ActiveResponse;
impl Card for ActiveResponse {
    fn id(&self) -> &'static str { "active_response" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Recon }
    fn describe(&self) -> &'static str { "Arm active response — detections auto-contain (the bite)" }
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::lack(Fact::AutoResponse)] }
    fn produces(&self) -> Vec<Fact> { vec![Fact::AutoResponse] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let o = realize(e, self.id(), p, s, "active response ARMED — a detected theft is contained instantly", vec![]);
        if o.success {
            s.auto_response = true;
        }
        o
    }
}

/// Remediate the escalation path outright. Requires knowing it exists (monitoring/alert).
struct FixAclPath;
impl Card for FixAclPath {
    fn id(&self) -> &'static str { "remediate_acl" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::LateralMove }
    fn describe(&self) -> &'static str { "Remove the GenericAll->DA path / tier admins" }
    fn category(&self) -> Category { Category::Harden }
    fn requires(&self) -> Vec<Requirement> {
        // Category-gated: any discovery Blue has *seen* unlocks the cut.
        vec![Requirement::lack(Fact::PathSevered), Requirement::saw_category(Category::Discovery)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::PathSevered] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let o = realize(e, self.id(), p, s, "revoked svc_mssql DCSync on the domain — path to DA severed", vec![]);
        if o.success {
            s.acl_path_fixed = true;
        }
        o
    }
}

/// Enforce AES — kills Kerberoasting's payoff. Requires knowing roasting is happening.
struct EnforceAes;
impl Card for EnforceAes {
    fn id(&self) -> &'static str { "enforce_aes" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Kerberoast }
    fn describe(&self) -> &'static str { "Disable RC4 / enforce AES — Kerberoast tickets uncrackable" }
    fn category(&self) -> Category { Category::Harden }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::lack(Fact::AesEnforced), Requirement::identified(Technique::Kerberoast)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::AesEnforced] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let o = realize(e, self.id(), p, s, "RC4 disabled, AES enforced — roast tickets are now junk", vec![]);
        if o.success {
            s.rc4_disabled = true;
        }
        o
    }
}

/// Enforce pre-auth — kills AS-REP roasting. Requires knowing it's happening.
struct EnforcePreauth;
impl Card for EnforcePreauth {
    fn id(&self) -> &'static str { "enforce_preauth" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::AsRepRoast }
    fn describe(&self) -> &'static str { "Enforce Kerberos pre-auth — AS-REP roasting yields nothing" }
    fn category(&self) -> Category { Category::Harden }
    fn requires(&self) -> Vec<Requirement> {
        vec![Requirement::lack(Fact::PreauthEnforced), Requirement::identified(Technique::AsRepRoast)]
    }
    fn produces(&self) -> Vec<Fact> { vec![Fact::PreauthEnforced] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let o = realize(e, self.id(), p, s, "pre-auth enforced on jbecker — AS-REP dead", vec![]);
        if o.success {
            s.preauth_required = true;
        }
        o
    }
}

/// Rotate a known-compromised credential.
struct HardenCreds;
impl Card for HardenCreds {
    fn id(&self) -> &'static str { "rotate_creds" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Kerberoast }
    fn describe(&self) -> &'static str { "Rotate credentials known to be compromised" }
    fn category(&self) -> Category { Category::Evict }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::probe(InstanceProbe::CredCompromiseKnown)] }
    // produces: invalidates a cred (removal), not a fact flip → empty
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        // Fire the real password reset on the DC (sim: no-op success), then record it.
        let live = realize(e, self.id(), p, s, "", vec![]);
        let mut rotated = vec![];
        for i in 0..s.creds.len() {
            if s.creds[i].cracked {
                let via = s.creds[i].via;
                if s.blue_knows(via) {
                    s.creds[i].cracked = false;
                    rotated.push(s.creds[i].principal.clone());
                }
            }
        }
        let narrative = if live.narrative.trim().is_empty() {
            format!("rotated {} — those tickets are dead", rotated.join(", "))
        } else {
            format!("{} ({})", live.narrative, rotated.join(", "))
        };
        Outcome { success: !rotated.is_empty(), narrative, detection_surface: vec![] }
    }
}

/// Threat hunt — go looking for a stealthy technique that slipped past (a coverage gap),
/// closing the visibility gap so a rule can then be written for it. This is how blue discovers
/// what baseline telemetry missed; it records a (delayed) detection, which shows up as MTTD.
struct Hunt;
impl Card for Hunt {
    fn id(&self) -> &'static str { "hunt" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Recon }
    fn describe(&self) -> &'static str { "Threat-hunt telemetry for an undetected technique (closes a gap)" }
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::probe(InstanceProbe::UndetectedActivity)] }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let live = realize(e, self.id(), p, s, "", vec![]);
        // hunt the highest-value technique red has performed that blue can't yet see
        let target = s.performed.iter().copied().filter(|t| !s.blue_knows(*t)).max_by_key(|t| t.value());
        if let Some(t) = target {
            s.alerts.push(Alert { round: s.round, technique: t, source: "hunt".into(), rule_id: "velociraptor-hunt".into(), level: 8 });
            s.mark_detected(t, "hunt");
            let narrative = note(&live.narrative, &format!("threat hunt surfaced {} — gap closed", t.as_key()));
            Outcome { success: true, narrative, detection_surface: vec![] }
        } else {
            Outcome { success: false, narrative: "threat hunt — nothing new".into(), detection_surface: vec![] }
        }
    }
}

/// Reactive detection rule for an observed technique.
struct DeployDetection;
impl Card for DeployDetection {
    fn id(&self) -> &'static str { "deploy_detection" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Kerberoast }
    fn describe(&self) -> &'static str { "Write a technique-based detection for observed activity" }
    fn category(&self) -> Category { Category::Detection }
    fn requires(&self) -> Vec<Requirement> { vec![Requirement::probe(InstanceProbe::UndetectedAlert)] }
    fn params_schema(&self) -> Value {
        json!({ "type": "object", "properties": { "technique": { "type": "string" } }, "required": ["technique"] })
    }
    fn default_params(&self, s: &GameState) -> Value {
        let t = s.alerts.iter().map(|a| a.technique).filter(|t| !s.has_detection(*t)).max_by_key(|t| t.value());
        match t { Some(t) => json!({ "technique": t.as_key() }), None => json!({}) }
    }
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let key = p.get("technique").and_then(|v| v.as_str()).unwrap_or("");
        match Technique::from_key(key) {
            Some(t) => {
                let live = realize(e, self.id(), p, s, "", vec![]);
                let fidelity = grade_rule(s.seed, t, s.round).to_string();
                s.detections.push(Detection { id: format!("rule-{}", t.as_key()), technique: t, deployed_round: s.round, technique_based: true, fidelity });
                let narrative = note(&live.narrative, &format!("deployed detection for {}", t.as_key()));
                Outcome { success: true, narrative, detection_surface: vec![] }
            }
            None => Outcome { success: false, narrative: "nothing observed to detect".into(), detection_surface: vec![] },
        }
    }
}

/// Re-segment the network (the rotating firewall): once blue has seen red moving east-west,
/// it drops a firewall rule that severs red's frontier from everything deeper — cutting the
/// path to the DC. It only works if red hasn't already reached a DC-adjacent zone: segmentation
/// can't un-ring a bell red has already crossed, so blue must detect the pivot AND act in time.
struct Segment;
impl Card for Segment {
    fn id(&self) -> &'static str { "segment" }
    fn side(&self) -> Side { Side::Blue }
    fn technique(&self) -> Technique { Technique::Pivot }
    fn describe(&self) -> &'static str { "Re-segment — firewall-drop red's frontier before it reaches the DC" }
    fn category(&self) -> Category { Category::Isolate }
    fn requires(&self) -> Vec<Requirement> {
        vec![
            Requirement::any_of(vec![
                Requirement::saw_category(Category::InitialAccess),
                Requirement::saw_category(Category::LateralMovement),
            ]),
            Requirement::lack(Fact::ReachesDc),
            Requirement::lack(Fact::DomainAdmin),
            Requirement::probe(InstanceProbe::HasForwardPath),
        ]
    }
    // produces: removes forward edges (may flip HasForwardPath false) → empty
    fn play(&self, s: &mut GameState, p: &Value, e: &mut dyn Environment) -> Outcome {
        let cut: Vec<String> = s.next_hops();
        let o = realize(e, self.id(), p, s, &format!("re-segmented — dropped red's path into {}", cut.join(", ")), vec![]);
        if o.success {
            // sever every edge out of a zone red holds into one it doesn't (its forward hops)
            let held = s.red_zones.clone();
            s.edges.retain(|(f, t)| !(held.iter().any(|z| z == f) && !held.iter().any(|z| z == t)));
        }
        o
    }
}

pub fn default_registry() -> CardRegistry {
    let mut r = CardRegistry::new();
    // red
    r.register(Box::new(InitialAccess));
    r.register(Box::new(Pivot));
    r.register(Box::new(Recon));
    r.register(Box::new(kerberoast_card()));
    r.register(Box::new(AsRepRoast));
    r.register(Box::new(BloodHoundCollect));
    r.register(Box::new(EscalateDa));
    // blue — active defense
    r.register(Box::new(ContinuousMonitoring));
    r.register(Box::new(Segment));
    r.register(Box::new(ActiveResponse));
    r.register(Box::new(FixAclPath));
    r.register(Box::new(EnforceAes));
    r.register(Box::new(EnforcePreauth));
    r.register(Box::new(HardenCreds));
    r.register(Box::new(Hunt));
    r.register(Box::new(DeployDetection));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GameState, Alert, Technique, Detection};

    #[test]
    fn enforce_aes_requires_a_deployed_rule_not_just_an_alert() {
        let reg = default_registry();
        let mut s = GameState::new(vec![]);
        s.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "x".into(), rule_id: "r".into(), level: 5 });
        assert!(!reg.get("enforce_aes").unwrap().precondition(&s), "alert alone must NOT unlock enforce_aes");
        s.detections.push(Detection { id: "d".into(), technique: Technique::Kerberoast, deployed_round: 1, technique_based: true, fidelity: "robust".into() });
        assert!(reg.get("enforce_aes").unwrap().precondition(&s), "a deployed kerberoast rule unlocks enforce_aes");
    }
}
