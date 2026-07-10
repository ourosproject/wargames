//! One-time proof the facts-as-data migration preserves each existing card's legality.
//! The `legacy` fn mirrors the ORIGINAL closures (card.rs @ baseline commit) verbatim.
//! NOTE: not a standing invariant — adding sibling tools later may change menus by design.

use purple_wargame::arsenal::default_registry;
use purple_wargame::state::{Alert, Cred, GameState, Technique};

const TECHS: [Technique; 9] = [
    Technique::InitialAccess, Technique::Recon, Technique::Pivot, Technique::Kerberoast,
    Technique::AsRepRoast, Technique::BloodHound, Technique::CredSpray, Technique::LateralMove,
    Technique::Exfil,
];

// Cheap splitmix64 so the sweep is deterministic without external crates.
fn mix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Build a pseudo-random reachable-ish state: vary zones, creds, performed, alerts,
/// detections, and blue posture flags.
fn gen_state(seed: u64) -> GameState {
    let mut r = seed;
    let mut s = GameState::new(vec![]);
    // topology: maybe internal, maybe attack-ready, sometimes a forward hop
    if mix(&mut r) & 1 == 1 { s.add_zone("vlan10"); }
    if mix(&mut r) & 1 == 1 { s.add_zone("vlan30"); } // objective_zone by default → attack_ready
    // extra forward edge so next_hops() can be non-empty even when internal
    if mix(&mut r) & 1 == 1 { s.edges.push(("vlan10".into(), "vlan20".into())); }
    // performed techniques
    for t in TECHS { if mix(&mut r) & 1 == 1 { s.performed.push(t); } }
    // creds (cracked, with a via technique)
    if mix(&mut r) & 1 == 1 {
        let via = if mix(&mut r) & 1 == 1 { Technique::Kerberoast } else { Technique::AsRepRoast };
        s.creds.push(Cred { principal: "p".into(), secret: None, cracked: true, via });
    }
    // alerts (what blue has seen)
    for t in TECHS { if mix(&mut r) & 1 == 1 {
        s.alerts.push(Alert { round: 1, technique: t, source: "gen".into(), rule_id: "r".into(), level: 5 });
    }}
    // detections (rules written)
    for t in TECHS { if mix(&mut r) & 1 == 1 {
        s.detections.push(purple_wargame::state::Detection {
            id: "d".into(), technique: t, deployed_round: 1, technique_based: true, fidelity: "robust".into(),
        });
    }}
    // scenario misconfigs vary
    s.misconfigs = TECHS.iter().copied().filter(|_| mix(&mut r) & 1 == 1).collect();
    // blue posture
    s.monitoring = mix(&mut r) & 1 == 1;
    s.auto_response = mix(&mut r) & 1 == 1;
    s.rc4_disabled = mix(&mut r) & 1 == 1;
    s.preauth_required = mix(&mut r) & 1 == 1;
    s.acl_path_fixed = mix(&mut r) & 1 == 1;
    s.red_reached_da = mix(&mut r) & 1 == 1;
    s
}

/// The ORIGINAL preconditions, verbatim from the baseline `card.rs`/`cards.rs`.
fn legacy(id: &str, s: &GameState) -> bool {
    match id {
        "initial_access" => !s.has_internal() && !s.next_hops().is_empty(),
        "pivot" => s.has_internal() && !s.attack_ready() && !s.next_hops().is_empty(),
        "recon" => s.attack_ready() && !s.performed_technique(Technique::Recon),
        "kerberoast" => s.attack_ready(),
        "asrep_roast" => s.attack_ready(),
        "bloodhound" => s.has_cracked_cred() && !s.performed_technique(Technique::BloodHound),
        "escalate_da" => s.vuln(Technique::LateralMove) && s.has_cracked_cred()
            && s.performed_technique(Technique::BloodHound) && !s.red_reached_da && !s.acl_path_fixed,
        "monitor" => !s.monitoring,
        "active_response" => !s.auto_response,
        "remediate_acl" => !s.acl_path_fixed
            && s.alerts.iter().any(|a| matches!(a.technique, Technique::Recon | Technique::BloodHound)),
        // v2: enforce_aes/enforce_preauth require a DEPLOYED rule (Identified/has_detection), not a bare alert.
        "enforce_aes" => !s.rc4_disabled && s.has_detection(Technique::Kerberoast),
        "enforce_preauth" => !s.preauth_required && s.has_detection(Technique::AsRepRoast),
        "rotate_creds" => s.creds.iter().any(|c| c.cracked && s.blue_knows(c.via)),
        "hunt" => s.performed.iter().any(|t| !s.blue_knows(*t)),
        "deploy_detection" => s.alerts.iter().any(|a| !s.has_detection(a.technique)),
        "segment" => (s.blue_knows(Technique::Pivot) || s.blue_knows(Technique::InitialAccess))
            && !s.attack_ready() && !s.red_reached_da && !s.next_hops().is_empty(),
        other => panic!("unknown card id in oracle: {other}"),
    }
}

#[test]
fn every_card_precondition_matches_legacy_over_the_reachable_space() {
    let reg = default_registry();
    for seed in 0..800u64 {
        let s = gen_state(seed);
        for spec in reg.all_specs() {
            let card = reg.get(&spec.id).unwrap();
            assert_eq!(
                card.precondition(&s),
                legacy(&spec.id, &s),
                "card {} disagreed with legacy oracle at seed {}", spec.id, seed
            );
        }
    }
}
