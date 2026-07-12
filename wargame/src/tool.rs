//! A move as data: a `ToolDef` (identity + preconditions + facts-left-behind + steps) wrapped
//! by `DataTool`, which implements the existing `Card` trait. The interpreter runs the steps in
//! dependency order, exactly reproducing the old hand-written `play()` bodies.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::card::{Card, Environment, Outcome};
use crate::category::Category;
use crate::effects::Effect;
use crate::facts::{Fact, Requirement};
use crate::graph::{resolve_order_keys, Context};
use crate::state::{GameState, Side, Technique};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guard {
    pub req: Requirement,
    pub else_narrative: String,
    #[serde(default)]
    pub else_surface: Vec<Technique>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub produces_keys: Vec<String>,
    #[serde(default)]
    pub guards: Vec<Guard>,
    pub effect: Effect,
    #[serde(default)]
    pub ok_surface: Vec<Technique>,
    pub ok_narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub id: String,
    pub side: Side,
    pub technique: Technique,
    pub category: Category,
    pub summary: String,
    #[serde(default)]
    pub gate: Vec<Requirement>,
    #[serde(default)]
    pub produces: Vec<Fact>,
    #[serde(default)]
    pub params_schema: Option<Value>,
    pub nodes: Vec<Node>,
}

/// A move loaded from data. Implements `Card`, so it lives in the same registry and plays
/// exactly like the old hand-written cards.
pub struct DataTool {
    def: ToolDef,
}

impl DataTool {
    pub fn new(def: ToolDef) -> Self {
        Self { def }
    }
    pub fn def(&self) -> &ToolDef {
        &self.def
    }
}

impl Card for DataTool {
    fn id(&self) -> &str {
        &self.def.id
    }
    fn side(&self) -> Side {
        self.def.side
    }
    fn technique(&self) -> Technique {
        self.def.technique
    }
    fn describe(&self) -> &str {
        &self.def.summary
    }
    fn category(&self) -> Category {
        self.def.category
    }
    fn requires(&self) -> Vec<Requirement> {
        self.def.gate.clone()
    }
    fn produces(&self) -> Vec<Fact> {
        self.def.produces.clone()
    }
    fn detection_surface(&self) -> Vec<Technique> {
        let mut out = Vec::new();
        for n in &self.def.nodes {
            for t in &n.ok_surface {
                if !out.contains(t) {
                    out.push(*t);
                }
            }
        }
        out
    }
    fn params_schema(&self) -> Value {
        self.def.params_schema.clone().unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} }))
    }
    fn default_params(&self, state: &GameState) -> Value {
        // Only the "write a detection rule" move has params: default to the highest-value
        // observed-but-unruled technique (reproduces the old deploy_detection default).
        if self.def.nodes.iter().any(|n| matches!(n.effect, Effect::DeployDetection)) {
            let t = state.alerts.iter().map(|a| a.technique).filter(|t| !state.has_detection(*t)).max_by_key(|t| t.value());
            return match t {
                Some(t) => serde_json::json!({ "technique": t.as_key() }),
                None => serde_json::json!({}),
            };
        }
        serde_json::json!({})
    }
    fn play(&self, state: &mut GameState, params: &Value, env: &mut dyn Environment) -> Outcome {
        let multi = self.def.nodes.len() > 1;
        let reqs: Vec<Vec<String>> = self.def.nodes.iter().map(|n| n.requires.clone()).collect();
        let prods: Vec<Vec<String>> = self.def.nodes.iter().map(|n| n.produces_keys.clone()).collect();
        let mut ctx = Context::new();
        let order = match resolve_order_keys(&reqs, &prods, &ctx.keys()) {
            Ok(o) => o,
            Err(e) => return Outcome { success: false, narrative: format!("[{}] {}", self.def.id, e), detection_surface: vec![] },
        };

        let mut surface: Vec<Technique> = Vec::new();
        let mut steps: Vec<String> = Vec::new();
        let mut single_narrative = String::new();

        for i in order {
            let node = &self.def.nodes[i];

            // Guards: first failing guard ends the move (no environment call).
            if let Some(g) = node.guards.iter().find(|g| !g.req.satisfied(state)) {
                for t in &g.else_surface {
                    if !surface.contains(t) {
                        surface.push(*t);
                    }
                }
                let narrative = if multi {
                    steps.push(format!("{}[FAIL]", node.id));
                    format!("[{}] dependency order: {}", self.def.id, steps.join(" -> "))
                } else {
                    g.else_narrative.clone()
                };
                return Outcome { success: false, narrative, detection_surface: surface };
            }

            let env_out = env.act(&node.id, params, state);
            let er = node.effect.apply(state, &mut ctx, params, env_out.success, &env_out.narrative, &node.ok_narrative);

            for t in &node.ok_surface {
                if !surface.contains(t) {
                    surface.push(*t);
                }
            }
            steps.push(format!("{}[{}]", node.id, if er.success { "ok" } else { "FAIL" }));
            single_narrative = er.narrative.unwrap_or_else(|| {
                if env_out.narrative.trim().is_empty() { node.ok_narrative.clone() } else { env_out.narrative.clone() }
            });

            if !er.success {
                let narrative = if multi {
                    format!("[{}] dependency order: {}", self.def.id, steps.join(" -> "))
                } else {
                    single_narrative.clone()
                };
                return Outcome { success: false, narrative, detection_surface: surface };
            }
        }

        let narrative = if multi {
            format!("[{}] dependency order: {}", self.def.id, steps.join(" -> "))
        } else {
            single_narrative
        };
        Outcome { success: true, narrative, detection_surface: surface }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SimEnvironment;
    use crate::state::{GameState, Host};

    fn base() -> GameState {
        GameState::new(vec![Host {
            id: "edge".into(), zone: "internet".into(), label: "edge".into(),
            foothold: false, reachable_by_red: true,
        }])
    }

    fn one_node(id: &str, effect: Effect, ok_narrative: &str, ok_surface: Vec<Technique>) -> Node {
        Node { id: id.into(), requires: vec![], produces_keys: vec![], guards: vec![], effect, ok_surface, ok_narrative: ok_narrative.into() }
    }

    #[test]
    fn single_node_tool_plays_its_effect_and_narrative() {
        let def = ToolDef {
            id: "monitor".into(), side: Side::Blue, technique: Technique::Recon,
            category: Category::Detection, summary: "watch".into(),
            gate: vec![Requirement::lack(Fact::Monitoring)], produces: vec![Fact::Monitoring],
            params_schema: None,
            nodes: vec![one_node("monitor", Effect::SetFlag(crate::effects::StateFlag::Monitoring), "monitoring ONLINE", vec![])],
        };
        let tool = DataTool::new(def);
        let mut s = base();
        let mut env = SimEnvironment::new();
        let o = tool.play(&mut s, &Value::Null, &mut env);
        assert!(o.success);
        assert!(s.monitoring);
        assert_eq!(o.narrative, "monitoring ONLINE");
    }

    #[test]
    fn multi_node_tool_runs_in_dependency_order_with_a_composite_narrative() {
        use crate::effects::StateFlag;
        let def = ToolDef {
            id: "chain".into(), side: Side::Red, technique: Technique::Kerberoast,
            category: Category::CredentialAccess, summary: "chain".into(),
            gate: vec![], produces: vec![], params_schema: None,
            nodes: vec![
                Node { id: "second".into(), requires: vec!["k".into()], produces_keys: vec![], guards: vec![],
                       effect: Effect::SetFlag(StateFlag::DomainAdmin), ok_surface: vec![Technique::LateralMove], ok_narrative: "b".into() },
                Node { id: "first".into(), requires: vec![], produces_keys: vec!["k".into()], guards: vec![],
                       effect: Effect::Produce { key: "k".into(), value: Value::Bool(true) }, ok_surface: vec![Technique::Recon], ok_narrative: "a".into() },
            ],
        };
        let tool = DataTool::new(def);
        let mut s = base();
        let mut env = SimEnvironment::new();
        let o = tool.play(&mut s, &Value::Null, &mut env);
        assert!(o.success);
        assert!(s.red_reached_da, "dependent node ran after its producer");
        assert_eq!(o.narrative, "[chain] dependency order: first[ok] -> second[ok]");
        assert_eq!(o.detection_surface, vec![Technique::Recon, Technique::LateralMove]);
    }

    #[test]
    fn guard_failure_stops_the_move_with_its_message_and_surface() {
        let def = ToolDef {
            id: "asrep_roast".into(), side: Side::Red, technique: Technique::AsRepRoast,
            category: Category::CredentialAccess, summary: "roast".into(),
            gate: vec![], produces: vec![], params_schema: None,
            nodes: vec![Node {
                id: "asrep_roast".into(), requires: vec![], produces_keys: vec![],
                guards: vec![Guard { req: Requirement::lack(Fact::PreauthEnforced), else_narrative: "AS-REP blocked — pre-auth enforced".into(), else_surface: vec![Technique::AsRepRoast] }],
                effect: Effect::Attempt, ok_surface: vec![Technique::AsRepRoast], ok_narrative: "cracked".into(),
            }],
        };
        let tool = DataTool::new(def);
        let mut s = base();
        s.preauth_required = true;
        let mut env = SimEnvironment::new();
        let o = tool.play(&mut s, &Value::Null, &mut env);
        assert!(!o.success);
        assert_eq!(o.narrative, "AS-REP blocked — pre-auth enforced");
        assert_eq!(o.detection_surface, vec![Technique::AsRepRoast]);
    }
}
