//! Loads move files (RON text) into `ToolDef`s, checks them, and builds the registry.
//! The checker is the safety net for the future author-a-move front-ends, and its
//! structural checks are the first real consumers of the requires/produces data.

use std::collections::HashSet;

use crate::category::Category;
use crate::effects::{Effect, StateFlag};
use crate::facts::Fact;
use crate::graph::resolve_order_keys;
use crate::registry::CardRegistry;
use crate::tool::{DataTool, ToolDef};

/// Parse one move file (RON text) into a `ToolDef`.
pub fn parse_tool(src: &str) -> Result<ToolDef, String> {
    ron::from_str::<ToolDef>(src).map_err(|e| format!("RON parse error: {e}"))
}

/// The facts a move actually establishes: those an effect flips true, plus the ones the referee
/// derives from recording the move's `technique` (recon -> scouted; bloodhound -> path mapped/scouted).
pub fn established_facts(def: &ToolDef) -> Vec<Fact> {
    let mut out = Vec::new();
    let add = |f: Fact, out: &mut Vec<Fact>| {
        if !out.contains(&f) {
            out.push(f);
        }
    };
    for n in &def.nodes {
        match &n.effect {
            Effect::Advance => add(Fact::Foothold, &mut out), // taking the first internal hop yields a foothold
            Effect::GrantCred { .. } => add(Fact::HasCred, &mut out),
            Effect::SetFlag(StateFlag::Monitoring) => add(Fact::Monitoring, &mut out),
            Effect::SetFlag(StateFlag::AutoResponse) => add(Fact::AutoResponse, &mut out),
            Effect::SetFlag(StateFlag::PathSevered) => add(Fact::PathSevered, &mut out),
            Effect::SetFlag(StateFlag::AesEnforced) => add(Fact::AesEnforced, &mut out),
            Effect::SetFlag(StateFlag::PreauthEnforced) => add(Fact::PreauthEnforced, &mut out),
            Effect::SetFlag(StateFlag::DomainAdmin) => add(Fact::DomainAdmin, &mut out),
            // ── arsenal primitives expansion: new objective / posture bits ──
            Effect::SetFlag(StateFlag::Persisted) => add(Fact::Persisted, &mut out),
            Effect::SetFlag(StateFlag::C2Established) => add(Fact::C2Active, &mut out),
            Effect::SetFlag(StateFlag::DataExfiltrated) => add(Fact::DataExfiltrated, &mut out),
            Effect::SetFlag(StateFlag::ImpactDone) => add(Fact::ImpactDone, &mut out),
            Effect::SetFlag(StateFlag::EgressBlocked) => add(Fact::EgressBlocked, &mut out),
            Effect::SetFlag(StateFlag::BackupsReady) => add(Fact::BackupsReady, &mut out),
            Effect::SetFlag(StateFlag::C2Blocked) => add(Fact::C2Blocked, &mut out),
            _ => {}
        }
    }
    // Facts the referee records from `technique()`:
    match def.technique {
        crate::state::Technique::Recon => add(Fact::Scouted, &mut out),
        crate::state::Technique::BloodHound => {
            add(Fact::PathMapped, &mut out);
            add(Fact::Scouted, &mut out);
        }
        _ => {}
    }
    out
}

/// Per-move checks: runnable steps, no dangling reads, and every produced fact is really established.
pub fn validate(def: &ToolDef) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();

    let reqs: Vec<Vec<String>> = def.nodes.iter().map(|n| n.requires.clone()).collect();
    let prods: Vec<Vec<String>> = def.nodes.iter().map(|n| n.produces_keys.clone()).collect();

    // (1) runnable steps — the dependency graph resolves from an empty blackboard
    if let Err(e) = resolve_order_keys(&reqs, &prods, &HashSet::new()) {
        errs.push(format!("[{}] unrunnable steps: {e}", def.id));
    }

    // (2) no dangling reads — every required key is produced by some step in this move
    let produced: HashSet<&String> = def.nodes.iter().flat_map(|n| n.produces_keys.iter()).collect();
    for n in &def.nodes {
        for r in &n.requires {
            if !produced.contains(r) {
                errs.push(format!("[{}] step '{}' reads '{}' which no step in the move produces", def.id, n.id, r));
            }
        }
    }

    // (3) leaves-behind — every declared produced fact is established by an effect or the technique
    let established = established_facts(def);
    for f in &def.produces {
        if !established.contains(f) {
            errs.push(format!("[{}] produces '{}' but no effect or technique establishes it", def.id, f.key()));
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Whole-set checks: unique ids, and at least one move in each enforced category.
pub fn validate_set(defs: &[ToolDef]) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();

    let mut seen: HashSet<&str> = HashSet::new();
    for d in defs {
        if !seen.insert(d.id.as_str()) {
            errs.push(format!("duplicate move id '{}'", d.id));
        }
    }
    for cat in Category::ENFORCED {
        if !defs.iter().any(|d| d.category == cat) {
            errs.push(format!("no move in required category '{}'", cat.key()));
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Parse, validate, and register a set of move-file sources into a fresh registry.
pub fn registry_from_sources(sources: &[&str]) -> Result<CardRegistry, Vec<String>> {
    let mut defs = Vec::new();
    let mut errs = Vec::new();
    for src in sources {
        match parse_tool(src) {
            Ok(def) => defs.push(def),
            Err(e) => errs.push(e),
        }
    }
    for def in &defs {
        if let Err(mut e) = validate(def) {
            errs.append(&mut e);
        }
    }
    if let Err(mut e) = validate_set(&defs) {
        errs.append(&mut e);
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    let mut reg = CardRegistry::new();
    for def in defs {
        reg.register(Box::new(DataTool::new(def)));
    }
    Ok(reg)
}

/// The move files, baked into the binary so it stays a single static executable.
pub const TOOL_FILES: &[&str] = &[
    include_str!("../tools/initial_access.ron"),
    include_str!("../tools/pivot.ron"),
    include_str!("../tools/recon.ron"),
    include_str!("../tools/kerberoast.ron"),
    include_str!("../tools/asrep_roast.ron"),
    include_str!("../tools/bloodhound.ron"),
    include_str!("../tools/escalate_da.ron"),
    include_str!("../tools/monitor.ron"),
    include_str!("../tools/active_response.ron"),
    include_str!("../tools/remediate_acl.ron"),
    include_str!("../tools/enforce_aes.ron"),
    include_str!("../tools/enforce_preauth.ron"),
    include_str!("../tools/rotate_creds.ron"),
    include_str!("../tools/hunt.ron"),
    include_str!("../tools/deploy_detection.ron"),
    include_str!("../tools/segment.ron"),
    // ── arsenal primitives expansion: new red families + their blue counters ──
    include_str!("../tools/phish.ron"),
    include_str!("../tools/deploy_implant.ron"),
    include_str!("../tools/establish_c2.ron"),
    include_str!("../tools/exfil_data.ron"),
    include_str!("../tools/ransomware.ron"),
    include_str!("../tools/evict.ron"),
    include_str!("../tools/block_egress.ron"),
    include_str!("../tools/backups.ron"),
    include_str!("../tools/block_c2.ron"),
];

/// Build the game's card library from the embedded move files. Panics with the full list of
/// problems if any file is malformed — a broken arsenal must fail loudly at startup, not
/// silently drop a move.
pub fn default_registry() -> CardRegistry {
    match registry_from_sources(TOOL_FILES) {
        Ok(reg) => reg,
        Err(errs) => panic!("arsenal failed validation:\n  - {}", errs.join("\n  - ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: &str = r#"ToolDef(
        id: "monitor", side: Blue, technique: Recon, category: Detection,
        summary: "watch", gate: [Category(fact: Monitoring, want: false)],
        produces: [Monitoring],
        nodes: [ Node(id: "monitor", effect: SetFlag(Monitoring), ok_narrative: "on") ],
    )"#;

    #[test]
    fn parses_a_valid_move_file() {
        let def = parse_tool(MONITOR).expect("should parse");
        assert_eq!(def.id, "monitor");
        assert_eq!(def.nodes.len(), 1);
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn rejects_a_dangling_blackboard_read() {
        let src = r#"ToolDef(id: "x", side: Red, technique: Recon, category: Discovery,
            summary: "s", nodes: [ Node(id: "a", requires: ["missing"], effect: Attempt, ok_narrative: "n") ])"#;
        let def = parse_tool(src).unwrap();
        let errs = validate(&def).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("dependency") || e.contains("missing")), "got {errs:?}");
    }

    #[test]
    fn rejects_a_move_claiming_a_fact_it_never_establishes() {
        // Claims PathSevered but only performs a technique — nothing sets it.
        let src = r#"ToolDef(id: "x", side: Red, technique: Recon, category: Discovery,
            summary: "s", produces: [PathSevered],
            nodes: [ Node(id: "x", effect: Attempt, ok_narrative: "n") ])"#;
        let def = parse_tool(src).unwrap();
        let errs = validate(&def).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("produces") && e.contains("path_severed")), "got {errs:?}");
    }

    #[test]
    fn set_rejects_duplicate_ids() {
        let a = parse_tool(MONITOR).unwrap();
        let b = parse_tool(MONITOR).unwrap();
        let errs = validate_set(&[a, b]).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("duplicate")), "got {errs:?}");
    }

    #[test]
    fn set_rejects_missing_category_coverage() {
        let def = parse_tool(MONITOR).unwrap(); // only Detection
        let errs = validate_set(&[def]).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("category")), "got {errs:?}");
    }

    #[test]
    fn full_arsenal_loads_with_new_families() {
        let reg = default_registry();
        assert_eq!(reg.len(), 25);
        for id in ["phish", "deploy_implant", "establish_c2", "exfil_data", "ransomware",
                   "evict", "block_egress", "backups", "block_c2"] {
            assert!(reg.get(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn exfil_fails_when_egress_blocked_succeeds_otherwise() {
        use crate::env::SimEnvironment;
        use crate::state::{Cred, GameState, Host, Technique};
        let reg = default_registry();
        let tool = reg.get("exfil_data").unwrap();
        let base = GameState::new(vec![Host {
            id: "e".into(), zone: "internet".into(), label: "e".into(),
            foothold: false, reachable_by_red: true,
        }]);
        let mut s = base.clone();
        s.creds.push(Cred { principal: "svc".into(), secret: None, cracked: true, via: Technique::Kerberoast });
        // egress open → exfil succeeds and sets the objective
        let mut ok = s.clone();
        let o = tool.play(&mut ok, &serde_json::json!({}), &mut SimEnvironment::new());
        assert!(o.success && ok.data_exfiltrated, "open egress: exfil succeeds and objective set");
        // egress blocked → guard fails, no objective
        let mut blocked = s.clone();
        blocked.egress_blocked = true;
        let o2 = tool.play(&mut blocked, &serde_json::json!({}), &mut SimEnvironment::new());
        assert!(!o2.success && !blocked.data_exfiltrated, "blocked egress: exfil fails, objective untouched");
    }

    #[test]
    fn evict_burns_persistence_then_kicks_red_out() {
        use crate::env::SimEnvironment;
        use crate::state::{GameState, Host};
        let reg = default_registry();
        let tool = reg.get("evict").unwrap();
        let mut s = GameState::new(vec![Host {
            id: "e".into(), zone: "internet".into(), label: "e".into(),
            foothold: false, reachable_by_red: true,
        }]);
        s.add_zone("vlan20");
        s.red_persisted = true;
        // gate needs a blue observation that red is inside — an initial-access alert
        s.alerts.push(crate::state::Alert {
            round: 1, technique: crate::state::Technique::InitialAccess,
            source: "edr".into(), rule_id: "r".into(), level: 8,
        });
        // first evict burns the implant, red keeps its ground
        let o1 = tool.play(&mut s, &serde_json::json!({}), &mut SimEnvironment::new());
        assert!(o1.success && !s.red_persisted && s.holds("vlan20"), "first evict burns persistence, ground held");
        // second evict removes red's deepest zone
        let o2 = tool.play(&mut s, &serde_json::json!({}), &mut SimEnvironment::new());
        assert!(o2.success && !s.holds("vlan20"), "second evict kicks red back to the perimeter");
    }
}
