//! The builder core: turn a form-shaped `MoveDraft` into a validated one-node `ToolDef`, and
//! save/list/delete authored move files. The front-end is a thin client over this + `arsenal`.

use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

use crate::arsenal::{self, to_ron};
use crate::category::Category;
use crate::effects::{Effect, StateFlag};
use crate::facts::{Fact, InstanceProbe, Requirement};
use crate::state::{Side, Technique};
use crate::tool::{Guard, Node, ToolDef};

#[derive(Debug, Deserialize)]
pub struct MoveDraft {
    pub name: String,
    pub side: String,
    pub category: String,
    pub technique: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub narrative: String,
    #[serde(default)]
    pub gate: Vec<GateRow>,
    /// Single-node authoring (the guided form): the one effect this move performs. Ignored when
    /// `nodes` is non-empty (the canvas path supplies full nodes instead).
    #[serde(default)]
    pub effect: Option<EffectDraft>,
    #[serde(default)]
    pub detection_surface: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
    /// Multi-node authoring (the node canvas): a wired DAG of steps. When present and non-empty,
    /// this supersedes the single-node `effect`/`detection_surface`/`narrative` fields.
    #[serde(default)]
    pub nodes: Vec<NodeDraft>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum GateRow {
    Fact { fact: String, want: bool },
    Probe { probe: String, #[serde(default)] arg: Option<String>, want: bool },
    AnyOf { of: Vec<GateRow> },
}

/// One step of a multi-node move: an effect, optional guards, and the blackboard keys it reads
/// (`requires`) and writes (`produces_keys`) — the wires the canvas draws between nodes.
#[derive(Debug, Deserialize)]
pub struct NodeDraft {
    pub id: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub produces_keys: Vec<String>,
    #[serde(default)]
    pub guards: Vec<GuardDraft>,
    pub effect: EffectDraft,
    #[serde(default)]
    pub detection_surface: Vec<String>,
    #[serde(default)]
    pub narrative: String,
}

/// A per-node guard: a requirement that must hold, plus the message/surface shown when it fails.
#[derive(Debug, Deserialize)]
pub struct GuardDraft {
    pub req: GateRow,
    #[serde(default)]
    pub else_narrative: String,
    #[serde(default)]
    pub else_surface: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct EffectDraft {
    pub kind: String,
    #[serde(default)]
    pub params: Value,
}

/// Sanitize a display name into a safe move id: lowercase, `[a-z0-9]` kept, every other run
/// becomes a single `_`, trimmed of leading/trailing `_`, capped at 64. Never contains `/` or `.`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
    }
    while out.ends_with('_') { out.pop(); }
    out.truncate(64);
    while out.ends_with('_') { out.pop(); }
    out
}

fn technique(k: &str) -> Result<Technique, String> {
    Technique::from_key(k).ok_or_else(|| format!("unknown technique '{k}'"))
}

fn build_probe(name: &str, arg: &Option<String>) -> Result<InstanceProbe, String> {
    let t = || technique(arg.as_deref().unwrap_or(""));
    let cat = || Category::from_key(arg.as_deref().unwrap_or("")).ok_or_else(|| format!("unknown category '{:?}'", arg));
    Ok(match name {
        "SawCategory" => InstanceProbe::SawCategory(cat()?),
        "Identified" => InstanceProbe::Identified(t()?),
        "Vuln" => InstanceProbe::Vuln(t()?),
        "Performed" => InstanceProbe::Performed(t()?),
        "Detected" => InstanceProbe::Detected(t()?),
        "HasForwardPath" => InstanceProbe::HasForwardPath,
        "LateralPathPlanted" => InstanceProbe::LateralPathPlanted,
        "CredCompromiseKnown" => InstanceProbe::CredCompromiseKnown,
        "UndetectedActivity" => InstanceProbe::UndetectedActivity,
        "UndetectedAlert" => InstanceProbe::UndetectedAlert,
        other => return Err(format!("unknown probe '{other}'")),
    })
}

fn build_requirement(row: &GateRow) -> Result<Requirement, String> {
    Ok(match row {
        GateRow::Fact { fact, want } => {
            let f = Fact::from_key(fact).ok_or_else(|| format!("unknown fact '{fact}'"))?;
            Requirement::Category { fact: f, want: *want }
        }
        GateRow::Probe { probe, arg, want } => {
            Requirement::Instance { probe: build_probe(probe, arg)?, want: *want }
        }
        GateRow::AnyOf { of } => {
            Requirement::AnyOf(of.iter().map(build_requirement).collect::<Result<Vec<_>, _>>()?)
        }
    })
}

fn build_effect(e: &EffectDraft) -> Result<Effect, String> {
    let p = &e.params;
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    Ok(match e.kind.as_str() {
        "Attempt" => Effect::Attempt,
        "Advance" => Effect::Advance,
        "SetFlag" => {
            let f = s("flag").ok_or("SetFlag needs a 'flag'")?;
            Effect::SetFlag(StateFlag::from_key(&f).ok_or_else(|| format!("unknown flag '{f}'"))?)
        }
        "GrantCred" => Effect::GrantCred {
            principal: s("principal").ok_or("GrantCred needs a 'principal'")?,
            secret: s("secret"),
            via: technique(&s("via").ok_or("GrantCred needs a 'via' technique")?)?,
        },
        "RevokeKnownCreds" => Effect::RevokeKnownCreds,
        "HuntGap" => Effect::HuntGap,
        "DeployDetection" => Effect::DeployDetection,
        "SeverForwardEdges" => Effect::SeverForwardEdges,
        "Evict" => Effect::Evict,
        // Produce wires a value onto the blackboard for a later node to read — the mechanism
        // multi-step moves use to chain (enum → request → crack). Canvas-only.
        "Produce" => Effect::Produce {
            key: s("key").ok_or("Produce needs a 'key'")?,
            value: p.get("value").cloned().unwrap_or(Value::Null),
        },
        other => return Err(format!("unknown or unsupported effect '{other}'")),
    })
}

/// Map a guard draft into a `Guard` (requirement + failure message/surface).
fn build_guard(g: &GuardDraft) -> Result<Guard, String> {
    Ok(Guard {
        req: build_requirement(&g.req)?,
        else_narrative: g.else_narrative.clone(),
        else_surface: g.else_surface.iter().map(|k| technique(k)).collect::<Result<Vec<_>, _>>()?,
    })
}

/// Map one node draft into a `Node`. The node id is sanitized like a move id (no traversal, no
/// dots/slashes) since it appears in narratives and keys the environment call.
fn build_node(n: &NodeDraft) -> Result<Node, String> {
    let id = slug(&n.id);
    if id.is_empty() {
        return Err(format!("node id '{}' produced an empty slug — use letters or digits", n.id));
    }
    Ok(Node {
        id,
        requires: n.requires.clone(),
        produces_keys: n.produces_keys.clone(),
        guards: n.guards.iter().map(build_guard).collect::<Result<Vec<_>, _>>()?,
        effect: build_effect(&n.effect)?,
        ok_surface: n.detection_surface.iter().map(|k| technique(k)).collect::<Result<Vec<_>, _>>()?,
        ok_narrative: n.narrative.clone(),
    })
}

/// Map a form draft into a validated-shape single-node ToolDef (not yet run through `validate`).
pub fn draft_to_tooldef(draft: &MoveDraft) -> Result<ToolDef, String> {
    let id = slug(&draft.name);
    if id.is_empty() {
        return Err("name produced an empty id — use letters or digits".into());
    }
    let side = match draft.side.as_str() {
        "Red" => Side::Red, "Blue" => Side::Blue,
        other => return Err(format!("unknown side '{other}'")),
    };
    let category = Category::from_key(&draft.category).ok_or_else(|| format!("unknown category '{}'", draft.category))?;
    let tech = technique(&draft.technique)?;
    let gate = draft.gate.iter().map(build_requirement).collect::<Result<Vec<_>, _>>()?;
    let produces = draft.produces.iter()
        .map(|k| Fact::from_key(k).ok_or_else(|| format!("unknown fact '{k}'")))
        .collect::<Result<Vec<_>, _>>()?;

    // Two authoring paths → one node list. The canvas supplies full `nodes`; the guided form
    // supplies a single top-level `effect`. Both produce the same `Vec<Node>`.
    let nodes = if !draft.nodes.is_empty() {
        let nodes = draft.nodes.iter().map(build_node).collect::<Result<Vec<_>, _>>()?;
        let mut seen = std::collections::HashSet::new();
        for n in &nodes {
            if !seen.insert(n.id.clone()) {
                return Err(format!("duplicate node id '{}' — each step needs a distinct id", n.id));
            }
        }
        nodes
    } else {
        let effect = build_effect(draft.effect.as_ref().ok_or("a move needs an effect (or a `nodes` list)")?)?;
        let ok_surface = draft.detection_surface.iter().map(|k| technique(k)).collect::<Result<Vec<_>, _>>()?;
        vec![Node {
            id: id.clone(), requires: vec![], produces_keys: vec![], guards: Vec::<Guard>::new(),
            effect, ok_surface, ok_narrative: draft.narrative.clone(),
        }]
    };

    // DeployDetection reads params.technique at play time, so any move containing it needs the schema.
    let params_schema = if nodes.iter().any(|n| matches!(n.effect, Effect::DeployDetection)) {
        Some(json!({ "type": "object", "properties": { "technique": { "type": "string" } }, "required": ["technique"] }))
    } else { None };

    Ok(ToolDef {
        id, side, technique: tech, category, summary: draft.summary.clone(),
        gate, produces, params_schema, nodes,
    })
}

/// Validate a draft WITHOUT writing anything: map to a ToolDef, run structural validation, and
/// check the id doesn't collide (built-in or existing authored). This backs the non-destructive
/// "Validate" button. Returns the mapped ToolDef or the plain-language errors.
pub fn check(draft: &MoveDraft, reg: &crate::registry::CardRegistry) -> Result<ToolDef, Vec<String>> {
    let def = draft_to_tooldef(draft).map_err(|e| vec![e])?;
    arsenal::validate(&def)?; // runnable / no dangling / leaves-behind
    if reg.get(&def.id).is_some() {
        return Err(vec![format!("a move named '{}' already exists — pick a different name", def.id)]);
    }
    Ok(def)
}

/// Validate (via `check`) then persist the draft as `<id>.ron` in `dir`. Returns the saved
/// ToolDef or the plain-language errors.
pub fn save(dir: &Path, draft: &MoveDraft, reg: &crate::registry::CardRegistry) -> Result<ToolDef, Vec<String>> {
    let def = check(draft, reg)?;
    let ron = to_ron(&def).map_err(|e| vec![e])?;
    std::fs::create_dir_all(dir).map_err(|e| vec![format!("cannot create {}: {e}", dir.display())])?;
    let path = dir.join(format!("{}.ron", def.id));
    std::fs::write(&path, ron).map_err(|e| vec![format!("cannot write {}: {e}", path.display())])?;
    Ok(def)
}

/// The set of built-in ids (an authored move may not use one).
fn builtin_ids() -> std::collections::HashSet<String> {
    arsenal::default_registry().all_specs().iter().map(|s| s.id.clone()).collect()
}

/// List every move: built-ins (read-only) + authored (editable), each with its full definition
/// (as JSON) so the form can load one as a reference or for editing.
pub fn list(dir: &Path) -> Value {
    let builtins = builtin_ids();
    let reg = arsenal::registry_with_authored(dir);
    let moves: Vec<Value> = reg.all_specs().iter().map(|s| {
        json!({ "id": s.id, "side": format!("{:?}", s.side), "category": s.category.key(),
                "technique": s.technique.as_key(), "summary": s.summary,
                "authored": !builtins.contains(&s.id) })
    }).collect();
    json!({ "moves": moves })
}

/// Delete an authored move file. Rejects built-in ids and sanitizes the id before touching disk.
pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    let safe = slug(id);
    if safe != id || safe.is_empty() {
        return Err(format!("invalid id '{id}'"));
    }
    if builtin_ids().contains(&safe) {
        return Err(format!("'{safe}' is a built-in move and cannot be deleted"));
    }
    let path = dir.join(format!("{safe}.ron"));
    if !path.exists() {
        return Err(format!("no authored move '{safe}'"));
    }
    std::fs::remove_file(&path).map_err(|e| format!("cannot delete {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> MoveDraft {
        MoveDraft {
            name: "My Test Move".into(), side: "Blue".into(), category: "harden".into(),
            technique: "kerberoast".into(), summary: "test".into(), narrative: "did it".into(),
            gate: vec![
                GateRow::Fact { fact: "aes_enforced".into(), want: false },
                GateRow::Probe { probe: "Identified".into(), arg: Some("kerberoast".into()), want: true },
            ],
            effect: Some(EffectDraft { kind: "SetFlag".into(), params: json!({ "flag": "aes_enforced" }) }),
            detection_surface: vec![],
            produces: vec!["aes_enforced".into()],
            nodes: vec![],
        }
    }

    #[test]
    fn slug_sanitizes_and_rejects_traversal() {
        assert_eq!(slug("My Phish!!"), "my_phish");
        assert_eq!(slug("../../etc/passwd"), "etc_passwd");
        assert_eq!(slug("A B  C"), "a_b_c");
        assert!(!slug("../../etc/passwd").contains('/') && !slug("../../etc").contains('.'));
    }

    #[test]
    fn draft_maps_to_a_valid_one_node_tooldef() {
        let def = draft_to_tooldef(&draft()).expect("map");
        assert_eq!(def.id, "my_test_move");
        assert_eq!(def.side, Side::Blue);
        assert_eq!(def.category, Category::Harden);
        assert_eq!(def.nodes.len(), 1);
        assert_eq!(def.nodes[0].id, "my_test_move");
        assert!(matches!(def.nodes[0].effect, Effect::SetFlag(StateFlag::AesEnforced)));
        assert_eq!(def.gate.len(), 2);
        arsenal::validate(&def).expect("the mapped move validates");
    }

    #[test]
    fn save_writes_lists_and_deletes() {
        let dir = std::env::temp_dir().join(format!("pw_builder_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        let def = save(&dir, &draft(), &reg).expect("save");
        assert_eq!(def.id, "my_test_move");
        assert!(dir.join("my_test_move.ron").exists());

        let listed = list(&dir);
        let ids: Vec<&str> = listed["moves"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"my_test_move") && ids.contains(&"kerberoast"));
        // built-in flagged read-only, authored editable
        let mine = listed["moves"].as_array().unwrap().iter().find(|m| m["id"] == "my_test_move").unwrap();
        assert_eq!(mine["authored"], true);
        let builtin = listed["moves"].as_array().unwrap().iter().find(|m| m["id"] == "kerberoast").unwrap();
        assert_eq!(builtin["authored"], false);

        delete(&dir, "my_test_move").expect("delete authored");
        assert!(!dir.join("my_test_move.ron").exists());
        assert!(delete(&dir, "kerberoast").is_err(), "cannot delete a built-in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_validates_without_writing() {
        let dir = std::env::temp_dir().join(format!("pw_builder_check_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        // a bad draft: claims to produce a fact its effect never establishes
        let mut bad = draft();
        bad.produces = vec!["path_severed".into()]; // SetFlag(AesEnforced) does not set path_severed
        let errs = check(&bad, &reg).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("path_severed")), "got {errs:?}");
        assert!(!dir.exists(), "check must not create the dir or write anything");
    }

    /// A kerberoast-shaped 3-node draft: enum_spns (Produce spn_targets) → request_tgs
    /// (requires spn_targets, Produce tgs_hash) → crack_hash (requires tgs_hash, GrantCred,
    /// guarded on AES-not-enforced). This is the canonical multi-step move the canvas authors.
    fn composite_draft() -> MoveDraft {
        MoveDraft {
            name: "My Roast".into(), side: "Red".into(), category: "credential_access".into(),
            technique: "kerberoast".into(), summary: "roast".into(), narrative: String::new(),
            gate: vec![GateRow::Fact { fact: "reaches_dc".into(), want: true }],
            effect: None,
            detection_surface: vec![],
            produces: vec!["has_cred".into()],
            nodes: vec![
                NodeDraft {
                    id: "enum_spns".into(), requires: vec![], produces_keys: vec!["spn_targets".into()],
                    guards: vec![],
                    effect: EffectDraft { kind: "Produce".into(), params: json!({ "key": "spn_targets", "value": ["MSSQLSvc/dc01"] }) },
                    detection_surface: vec!["recon".into()], narrative: "found SPNs".into(),
                },
                NodeDraft {
                    id: "request_tgs".into(), requires: vec!["spn_targets".into()], produces_keys: vec!["tgs_hash".into()],
                    guards: vec![],
                    effect: EffectDraft { kind: "Produce".into(), params: json!({ "key": "tgs_hash", "value": "$krb5tgs$" }) },
                    detection_surface: vec!["kerberoast".into()], narrative: "got TGS".into(),
                },
                NodeDraft {
                    id: "crack_hash".into(), requires: vec!["tgs_hash".into()], produces_keys: vec![],
                    guards: vec![GuardDraft {
                        req: GateRow::Fact { fact: "aes_enforced".into(), want: false },
                        else_narrative: "AES enforced — ticket uncrackable".into(), else_surface: vec![],
                    }],
                    effect: EffectDraft { kind: "GrantCred".into(), params: json!({ "principal": "range\\svc", "secret": "pw", "via": "kerberoast" }) },
                    detection_surface: vec![], narrative: "cracked".into(),
                },
            ],
        }
    }

    #[test]
    fn multi_node_draft_maps_validates_and_round_trips() {
        let def = draft_to_tooldef(&composite_draft()).expect("map composite");
        assert_eq!(def.id, "my_roast");
        assert_eq!(def.nodes.len(), 3, "three wired nodes");
        assert_eq!(def.nodes[0].produces_keys, vec!["spn_targets".to_string()]);
        assert_eq!(def.nodes[1].requires, vec!["spn_targets".to_string()]);
        assert_eq!(def.nodes[2].requires, vec!["tgs_hash".to_string()]);
        assert_eq!(def.nodes[2].guards.len(), 1, "crack step keeps its AES guard");
        assert!(matches!(def.nodes[2].effect, Effect::GrantCred { .. }));
        // the DAG resolves + no dangling reads + leaves-behind holds
        arsenal::validate(&def).expect("composite validates");
        // round-trips through RON identically
        let ron = to_ron(&def).expect("serialize");
        let reparsed = arsenal::parse_tool(&ron).unwrap_or_else(|e| panic!("re-parse: {e}\n{ron}"));
        assert_eq!(serde_json::to_value(&def).unwrap(), serde_json::to_value(&reparsed).unwrap());
    }

    #[test]
    fn multi_node_rejects_a_dangling_wire() {
        // request_tgs requires a key nothing produces → arsenal::validate must reject it.
        let dir = std::env::temp_dir().join(format!("pw_builder_dangle_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        let mut d = composite_draft();
        d.nodes[1].requires = vec!["nonexistent_key".into()];
        let errs = check(&d, &reg).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("nonexistent_key")), "got {errs:?}");
    }

    #[test]
    fn multi_node_rejects_duplicate_node_ids() {
        let mut d = composite_draft();
        d.nodes[1].id = "enum_spns".into(); // collide with node 0
        let err = draft_to_tooldef(&d).unwrap_err();
        assert!(err.contains("enum_spns") && err.to_lowercase().contains("dupl"), "got {err}");
    }

    #[test]
    fn single_node_path_still_works_when_no_nodes_given() {
        // back-compat: the existing /build form posts a top-level effect and no `nodes`.
        let def = draft_to_tooldef(&draft()).expect("single-node still maps");
        assert_eq!(def.nodes.len(), 1);
        assert_eq!(def.nodes[0].id, "my_test_move");
    }

    #[test]
    fn save_rejects_builtin_id_collision() {
        let dir = std::env::temp_dir().join(format!("pw_builder_collide_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = arsenal::registry_with_authored(&dir);
        let mut d = draft();
        d.name = "kerberoast".into(); // collides with a built-in
        let errs = save(&dir, &d, &reg).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("kerberoast") && e.to_lowercase().contains("exist")), "got {errs:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
