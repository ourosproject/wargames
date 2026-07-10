//! Migration proof: each data move plays byte-for-byte identically to the legacy card it
//! replaced. The legacy cards are gone; their behavior is preserved as frozen golden fixtures
//! (`tests/fixtures/arsenal_goldens.json`), and this suite proves the data moves still match.

use purple_wargame::card::Card;
use purple_wargame::env::SimEnvironment;
use purple_wargame::state::{Alert, Cred, Detection, GameState, Host, Technique};
use serde_json::{json, Value};

/// A diverse set of states that, between them, exercise every effect and guard branch.
fn matrix() -> Vec<(String, GameState, Value)> {
    let mut out: Vec<(String, GameState, Value)> = Vec::new();

    let host = || Host { id: "edge".into(), zone: "internet".into(), label: "edge".into(), foothold: false, reachable_by_red: true };

    // s0: fresh
    out.push(("fresh".into(), GameState::new(vec![host()]), json!({})));

    // s1: foothold with a forward hop
    let mut s1 = GameState::new(vec![host()]);
    s1.add_zone("vlan20");
    s1.edges.push(("vlan20".into(), "vlan30".into()));
    out.push(("foothold+hop".into(), s1, json!({})));

    // s2: reaches DC (objective zone held), default misconfigs
    let mut s2 = GameState::new(vec![host()]);
    s2.add_zone("vlan30");
    out.push(("reaches_dc".into(), s2, json!({})));

    // s3: reaches DC but AES enforced + preauth enforced
    let mut s3 = GameState::new(vec![host()]);
    s3.add_zone("vlan30");
    s3.rc4_disabled = true;
    s3.preauth_required = true;
    out.push(("reaches_dc+hardened".into(), s3, json!({})));

    // s4: reaches DC but no roastable misconfigs planted
    let mut s4 = GameState::new(vec![host()]);
    s4.add_zone("vlan30");
    s4.misconfigs.clear();
    out.push(("reaches_dc+no_vuln".into(), s4, json!({})));

    // s5: holds a cracked cred (kerberoast) that blue has detected; path mapped
    let mut s5 = GameState::new(vec![host()]);
    s5.add_zone("vlan30");
    s5.creds.push(Cred { principal: "range.local\\svc_mssql".into(), secret: Some("Summer2024!".into()), cracked: true, via: Technique::Kerberoast });
    s5.performed.push(Technique::BloodHound);
    s5.alerts.push(Alert { round: 1, technique: Technique::Kerberoast, source: "baseline".into(), rule_id: "r".into(), level: 8 });
    out.push(("cred+detected+mapped".into(), s5, json!({})));

    // s6: red performed undetected recon + an intrusion alert exists
    let mut s6 = GameState::new(vec![host()]);
    s6.add_zone("vlan20");
    s6.edges.push(("vlan20".into(), "vlan30".into()));
    s6.performed.push(Technique::Recon);
    s6.performed.push(Technique::Kerberoast);
    s6.alerts.push(Alert { round: 1, technique: Technique::Pivot, source: "baseline".into(), rule_id: "r".into(), level: 8 });
    out.push(("undetected_activity".into(), s6, json!({})));

    // s7: scouting detected + a DCSync path planted (for remediate/escalate branches)
    let mut s7 = GameState::new(vec![host()]);
    s7.add_zone("vlan30");
    s7.alerts.push(Alert { round: 1, technique: Technique::Recon, source: "hunt".into(), rule_id: "r".into(), level: 8 });
    s7.creds.push(Cred { principal: "range.local\\svc_mssql".into(), secret: Some("x".into()), cracked: true, via: Technique::Kerberoast });
    s7.performed.push(Technique::BloodHound);
    out.push(("scout_detected+path".into(), s7, json!({})));

    // s8: reaches DC + DEPLOYED rules for kerberoast & asrep (identified:* true → unlocks
    // enforce_aes / enforce_preauth in v2's two-level detection model)
    let mut s8 = GameState::new(vec![host()]);
    s8.add_zone("vlan30");
    s8.detections.push(Detection { id: "rk".into(), technique: Technique::Kerberoast, deployed_round: 1, technique_based: true, fidelity: "robust".into() });
    s8.detections.push(Detection { id: "ra".into(), technique: Technique::AsRepRoast, deployed_round: 1, technique_based: true, fidelity: "robust".into() });
    out.push(("identified_rules".into(), s8, json!({})));

    // s9: foothold + an initial-access alert + a forward hop (saw:initial_access true, not at DC →
    // segment's AnyOf branch is legal)
    let mut s9 = GameState::new(vec![host()]);
    s9.add_zone("vlan20");
    s9.edges.push(("vlan20".into(), "vlan30".into()));
    s9.alerts.push(Alert { round: 1, technique: Technique::InitialAccess, source: "baseline".into(), rule_id: "r".into(), level: 8 });
    out.push(("saw_initial_access+hop".into(), s9, json!({})));

    out
}

/// Play a card on a clone of `state` through a fresh sim env; return (outcome, resulting state)
/// as JSON for exact comparison.
fn run(card: &dyn Card, state: &GameState, params: &Value) -> (Value, Value) {
    let mut s = state.clone();
    let mut env = SimEnvironment::new();
    let o = card.play(&mut s, params, &mut env);
    (serde_json::to_value(&o).unwrap(), serde_json::to_value(&s).unwrap())
}

// Standing behavioral guard after the legacy code is gone: every data move still matches the
// frozen golden captured from the original hand-written cards.
#[test]
fn data_moves_match_frozen_goldens() {
    let golden: Value = serde_json::from_str(include_str!("fixtures/arsenal_goldens.json")).unwrap();
    let golden = golden.as_object().unwrap();
    let reg = purple_wargame::arsenal::default_registry();
    let param_cases = |id: &str| -> Vec<Value> {
        if id == "deploy_detection" { vec![json!({"technique":"kerberoast"}), json!({"technique":"not_a_technique"}), json!({})] } else { vec![json!({})] }
    };
    for (key, expected) in golden {
        let parts: Vec<&str> = key.split('|').collect();
        let (id, label, pi) = (parts[0], parts[1], parts[2].parse::<usize>().unwrap());
        let card = reg.get(id).unwrap_or_else(|| panic!("no move '{id}' in data registry"));
        let (_, state, _) = matrix().into_iter().find(|(l, _, _)| l == label).unwrap();
        let p = param_cases(id).into_iter().nth(pi).unwrap();
        let (o, s) = run(card, &state, &p);
        assert_eq!(&json!({ "outcome": o, "state": s }), expected, "move '{id}' drifted from golden in state '{label}' (params #{pi})");
    }
}
