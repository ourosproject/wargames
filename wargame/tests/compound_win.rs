//! Proves a data-defined COMPOUND win: red wins by satisfying a combination of conditions at
//! once (creds + exfiltrated + undetected), not by reaching Domain Admin. The win condition
//! reuses the same `Requirement` alphabet that gates every move.

use purple_wargame::arsenal;
use purple_wargame::card::Move;
use purple_wargame::env::SimEnvironment;
use purple_wargame::facts::{Fact, Requirement};
use purple_wargame::referee::Referee;
use purple_wargame::rules::{RuleSet, WinCondition};
use purple_wargame::state::{Alert, Cred, GameState, Host, Side, Technique};

fn heist_rules() -> RuleSet {
    RuleSet {
        red_win_conditions: vec![WinCondition {
            name: "silent_heist".into(),
            all_of: vec![
                Requirement::have(Fact::HasCred),
                Requirement::have(Fact::DataExfiltrated),
                Requirement::have(Fact::Undetected),
            ],
        }],
        ..RuleSet::default()
    }
}

fn edge_host() -> Host {
    Host { id: "e".into(), zone: "internet".into(), label: "e".into(), foothold: false, reachable_by_red: true }
}

#[test]
fn red_wins_a_silent_heist_only_when_all_three_hold() {
    let reg = arsenal::default_registry();
    let referee = Referee { rules: heist_rules(), registry: reg };
    let mut env = SimEnvironment::new();

    // red holds a cred and is about to exfiltrate — quietly (no alerts yet).
    let mut s = GameState::new(vec![edge_host()]);
    s.add_zone("vlan30");
    s.creds.push(Cred { principal: "svc".into(), secret: None, cracked: true, via: Technique::Kerberoast });

    // not yet exfiltrated → not all conditions hold
    assert!(
        referee.rules.red_win_conditions[0].all_of.iter().any(|r| !r.satisfied(&s)),
        "before exfil, the compound condition is not satisfied"
    );

    // play exfil_data → sets DataExfiltrated; still no alerts → Undetected holds → WIN
    let mv = Move { side: Side::Red, card: "exfil_data".into(), params: serde_json::json!({}) };
    referee.begin_round(&mut s);
    let report = referee.red_phase(&mut s, &mv, &mut env);
    assert!(
        report.finished && report.winner == Some(Side::Red),
        "all three conditions now hold → silent heist wins"
    );
    assert_eq!(s.win_reason, "silent_heist");

    // control: a detected exfil makes Undetected false → NOT a win, even with cred + exfil.
    let mut s2 = GameState::new(vec![edge_host()]);
    s2.add_zone("vlan30");
    s2.creds.push(Cred { principal: "svc".into(), secret: None, cracked: true, via: Technique::Kerberoast });
    s2.data_exfiltrated = true;
    s2.performed.push(Technique::Exfil);
    s2.alerts.push(Alert { round: 1, technique: Technique::Exfil, source: "dlp".into(), rule_id: "r".into(), level: 8 });
    let won = referee.rules.red_win_conditions[0].all_of.iter().all(|r| r.satisfied(&s2));
    assert!(!won, "a detected exfil must NOT win the silent heist (Undetected is false)");
}
