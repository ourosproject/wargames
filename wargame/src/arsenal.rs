//! Loads move files (RON text) into `ToolDef`s, checks them, and builds the registry.
//! The checker is the safety net for the future author-a-move front-ends, and its
//! structural checks are the first real consumers of the requires/produces data.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

/// The home folder that holds a player's authored moves: `$HOME/.purple-range/tools`.
pub fn authored_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".purple-range").join("tools")
}

/// The built-in arsenal PLUS every valid authored `.ron` in `dir`. A malformed or invalid
/// authored file is logged to stderr and skipped — never fatal (unlike the built-ins, which
/// `default_registry` panics on). A missing dir yields just the built-ins.
pub fn registry_with_authored(dir: &Path) -> CardRegistry {
    let mut reg = default_registry(); // built-ins (panics if a built-in is bad — intended)
    let existing: std::collections::HashSet<String> = reg.all_specs().iter().map(|s| s.id.clone()).collect();
    let mut seen = existing.clone();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return reg, // no authored dir yet
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "ron").unwrap_or(false))
        .collect();
    paths.sort(); // deterministic load order

    for path in paths {
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => { eprintln!("[arsenal] skip {}: {e}", path.display()); continue; }
        };
        let def = match parse_tool(&src) {
            Ok(d) => d,
            Err(e) => { eprintln!("[arsenal] skip {}: {e}", path.display()); continue; }
        };
        if let Err(errs) = validate(&def) {
            eprintln!("[arsenal] skip {}: {}", path.display(), errs.join("; "));
            continue;
        }
        if seen.contains(&def.id) {
            eprintln!("[arsenal] skip {}: id '{}' collides with an existing move", path.display(), def.id);
            continue;
        }
        seen.insert(def.id.clone());
        reg.register(Box::new(DataTool::new(def)));
    }
    reg
}

/// Serialize a move to RON text for writing to an authored file. Emits struct names
/// (e.g. `ToolDef(...)`) so authored files read like the hand-written built-ins, and
/// re-parse cleanly via `parse_tool`.
pub fn to_ron(def: &ToolDef) -> Result<String, String> {
    let cfg = ron::ser::PrettyConfig::default().struct_names(true);
    ron::ser::to_string_pretty(def, cfg).map_err(|e| format!("RON serialize error: {e}"))
}

/// The full palette the builder form renders from — assembled from the engine so it can never
/// drift from what the engine actually supports.
pub fn vocabulary() -> serde_json::Value {
    use serde_json::json;
    use crate::category::Category;
    use crate::effects::StateFlag;
    use crate::facts::Fact;
    use crate::state::{Side, Technique};

    let categories: Vec<serde_json::Value> = [
        Category::InitialAccess, Category::Discovery, Category::CredentialAccess, Category::PrivilegeEscalation,
        Category::LateralMovement, Category::Exfiltration, Category::Detection, Category::DefenseEvasion,
        Category::Reconnaissance, Category::ResourceDevelopment, Category::Execution, Category::Persistence,
        Category::Collection, Category::CommandAndControl, Category::Impact, Category::Harden, Category::Isolate,
        Category::Evict, Category::Deceive, Category::Model,
    ].iter().map(|c| json!({ "key": c.key(), "defensive": c.is_defensive(),
        "enforced": Category::ENFORCED.contains(c) })).collect();

    let techniques: Vec<serde_json::Value> = [
        Technique::InitialAccess, Technique::Recon, Technique::Pivot, Technique::Kerberoast, Technique::AsRepRoast,
        Technique::BloodHound, Technique::CredSpray, Technique::LateralMove, Technique::Exfil,
        // ── arsenal primitives expansion techniques ──
        Technique::Phishing, Technique::ExploitPublicApp, Technique::ValidAccounts, Technique::LsassDump,
        Technique::Malware, Technique::C2, Technique::Persistence, Technique::Ransomware,
    ].iter().map(|t| json!({ "key": t.as_key(), "label": t.attack_name() })).collect();

    let facts: Vec<serde_json::Value> = Fact::ALL.iter().map(|f| json!({
        "key": f.key(), "question": f.question(),
        "side": if f.audience() == Side::Red { "Red" } else { "Blue" },
    })).collect();

    let state_flags: Vec<serde_json::Value> = [
        StateFlag::Monitoring, StateFlag::AutoResponse, StateFlag::PathSevered, StateFlag::AesEnforced,
        StateFlag::PreauthEnforced, StateFlag::DomainAdmin,
        // ── arsenal primitives expansion: objective / posture flags ──
        StateFlag::Persisted, StateFlag::C2Established, StateFlag::DataExfiltrated, StateFlag::ImpactDone,
        StateFlag::EgressBlocked, StateFlag::BackupsReady, StateFlag::C2Blocked,
    ].iter().map(|f| json!({ "key": f.key() })).collect();

    // Probes the form can gate on. `arg` = what the form must collect for this probe.
    let probes = json!([
        { "key": "SawCategory", "arg": "category", "label": "Seen any activity in a tactic (cheap)" },
        { "key": "Identified", "arg": "technique", "label": "Has a deployed detection rule for a technique" },
        { "key": "Vuln", "arg": "technique", "label": "The attack path for a technique is planted this scenario" },
        { "key": "Performed", "arg": "technique", "label": "The attacker has performed a technique" },
        { "key": "Detected", "arg": "technique", "label": "Blue has an alert for a technique" },
        { "key": "HasForwardPath", "arg": null, "label": "The attacker has a forward hop to take" },
        { "key": "LateralPathPlanted", "arg": null, "label": "A DCSync-able ACL path exists this scenario" },
        { "key": "CredCompromiseKnown", "arg": null, "label": "A stolen credential the defender has detected exists" },
        { "key": "UndetectedActivity", "arg": null, "label": "Some performed technique is not yet detected" },
        { "key": "UndetectedAlert", "arg": null, "label": "Some alert has no detection rule yet" },
    ]);

    json!({
        "sides": ["Red", "Blue"],
        "categories": categories,
        "techniques": techniques,
        "facts": facts,
        "state_flags": state_flags,
        "probes": probes,
        "effects": crate::effects::effect_descriptors(),
    })
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

    #[test]
    fn tool_round_trips_through_ron_serialization() {
        // parse a built-in, serialize it back to RON, re-parse — must be identical.
        let original = parse_tool(TOOL_FILES[3]).unwrap(); // kerberoast (3-node composite)
        let ron = to_ron(&original).expect("serialize");
        let reparsed = parse_tool(&ron).unwrap_or_else(|e| panic!("re-parse failed: {e}\n---\n{ron}"));
        assert_eq!(serde_json::to_value(&original).unwrap(), serde_json::to_value(&reparsed).unwrap());
    }

    #[test]
    fn registry_with_authored_includes_valid_and_skips_broken() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("pw_authored_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // a valid authored move
        let good = r#"ToolDef(id: "my_test_move", side: Blue, technique: Recon, category: Detection,
            summary: "test", gate: [Category(fact: Monitoring, want: false)], produces: [Monitoring],
            nodes: [ Node(id: "my_test_move", effect: SetFlag(Monitoring), ok_narrative: "on") ])"#;
        std::fs::File::create(dir.join("my_test_move.ron")).unwrap().write_all(good.as_bytes()).unwrap();
        // a broken authored move (bad RON)
        std::fs::File::create(dir.join("broken.ron")).unwrap().write_all(b"ToolDef( this is not ron").unwrap();

        let reg = registry_with_authored(&dir);
        assert_eq!(reg.len(), 26, "25 built-ins + 1 valid authored (broken one skipped)");
        assert!(reg.get("my_test_move").is_some());
        // built-ins-only registry is unchanged
        assert_eq!(default_registry().len(), 25);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_with_authored_handles_missing_dir() {
        let dir = std::env::temp_dir().join("pw_authored_does_not_exist_zzz");
        let _ = std::fs::remove_dir_all(&dir);
        let reg = registry_with_authored(&dir);
        assert_eq!(reg.len(), 25, "missing authored dir → just the built-ins");
    }

    #[test]
    fn vocabulary_lists_the_full_palette_including_produce_for_the_canvas() {
        let v = vocabulary();
        let effects: Vec<&str> = v["effects"].as_array().unwrap().iter().map(|e| e["key"].as_str().unwrap()).collect();
        assert!(effects.contains(&"SetFlag") && effects.contains(&"GrantCred") && effects.contains(&"HuntGap"));
        assert!(effects.contains(&"Evict"), "the expansion's Evict effect is authorable");
        assert!(effects.contains(&"Produce"), "Produce is the multi-node wire — offered now the canvas exists");
        // facts, probes, categories, techniques, sides all present and non-empty
        assert_eq!(v["facts"].as_array().unwrap().len(), 19);
        assert!(v["probes"].as_array().unwrap().len() >= 8);
        assert!(v["categories"].as_array().unwrap().iter().any(|c| c["key"] == "harden"));
        assert!(v["techniques"].as_array().unwrap().iter().any(|t| t["key"] == "kerberoast"));
        assert_eq!(v["sides"].as_array().unwrap().len(), 2);
        assert!(v["state_flags"].as_array().unwrap().iter().any(|f| f["key"] == "aes_enforced"));
    }
}
