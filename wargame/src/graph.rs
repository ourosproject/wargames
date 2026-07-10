//! The dependency-graph layer — how exploits/defenses are *composed* from primitives.
//!
//! This is the node-based builder's execution model: an exploit is a set of **function-node
//! primitives** wired by **dependency** (node B depends on node A because B reads a key that
//! A produces). The engine resolves the dependency order and runs them — the same elastic,
//! dependency-ranked lattice idea from the brain/forest, applied to attack construction.
//!
//! A [`CompositeCard`] is a graph of primitives that satisfies the `Card` trait, so it lives
//! in the same registry as hand-written code cards (the hybrid, option C).

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::card::{Card, Environment, Outcome};
use crate::category::Category;
use crate::facts::{Fact, Requirement};
use crate::state::{GameState, Side, Technique};

/// Blackboard passed between primitives — outputs of one node become inputs of dependents.
#[derive(Default)]
pub struct Context {
    data: HashMap<String, Value>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&mut self, key: &str, value: Value) {
        self.data.insert(key.to_string(), value);
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }
    pub fn keys(&self) -> HashSet<String> {
        self.data.keys().cloned().collect()
    }
}

/// Result of running one primitive node.
pub struct PrimitiveResult {
    pub success: bool,
    pub note: String,
    /// Techniques this node exposed — bubbles up into the composite's detection surface.
    pub detection_surface: Vec<Technique>,
}

/// A function-node: the atomic unit the node-builder wires together. Dependencies are
/// *implicit* — a node depends on whichever nodes produce the keys it `requires`.
pub trait Primitive: Send + Sync {
    fn id(&self) -> &'static str;
    fn describe(&self) -> &'static str;
    /// Blackboard keys this node reads (its inbound dependency edges).
    fn requires(&self) -> Vec<&'static str> {
        vec![]
    }
    /// Blackboard keys this node writes (what dependents can consume).
    fn produces(&self) -> Vec<&'static str> {
        vec![]
    }
    fn run(&self, ctx: &mut Context, state: &mut GameState, env: &mut dyn Environment) -> PrimitiveResult;
}

/// Topologically order nodes by their data dependencies (requires/produces).
/// Returns indices in a legal execution order, or an error on a cycle / missing input.
fn resolve_order(nodes: &[Box<dyn Primitive>], initial: &HashSet<String>) -> Result<Vec<usize>, String> {
    let mut scheduled = vec![false; nodes.len()];
    let mut available = initial.clone();
    let mut order = Vec::new();
    loop {
        let mut progressed = false;
        for i in 0..nodes.len() {
            if scheduled[i] {
                continue;
            }
            if nodes[i].requires().iter().all(|r| available.contains(*r)) {
                scheduled[i] = true;
                for p in nodes[i].produces() {
                    available.insert(p.to_string());
                }
                order.push(i);
                progressed = true;
            }
        }
        if order.len() == nodes.len() {
            break;
        }
        if !progressed {
            return Err("unsatisfiable dependency graph (cycle or missing input)".into());
        }
    }
    Ok(order)
}

/// A card composed from primitive function-nodes wired by dependency. Implements `Card`,
/// so it registers and plays exactly like a hand-written code card.
pub struct CompositeCard {
    pub id: &'static str,
    pub side: Side,
    pub technique: Technique,
    pub summary: &'static str,
    pub category: Category,
    pub requires: Vec<Requirement>,
    pub produces: Vec<Fact>,
    pub surface: Vec<Technique>,
    pub nodes: Vec<Box<dyn Primitive>>,
}

impl Card for CompositeCard {
    fn id(&self) -> &'static str {
        self.id
    }
    fn side(&self) -> Side {
        self.side
    }
    fn technique(&self) -> Technique {
        self.technique
    }
    fn describe(&self) -> &'static str {
        self.summary
    }
    fn category(&self) -> Category {
        self.category
    }
    fn requires(&self) -> Vec<Requirement> {
        self.requires.clone()
    }
    fn produces(&self) -> Vec<Fact> {
        self.produces.clone()
    }
    fn detection_surface(&self) -> Vec<Technique> {
        self.surface.clone()
    }
    fn play(&self, state: &mut GameState, _params: &Value, env: &mut dyn Environment) -> Outcome {
        let mut ctx = Context::new();
        let order = match resolve_order(&self.nodes, &ctx.keys()) {
            Ok(o) => o,
            Err(e) => {
                return Outcome {
                    success: false,
                    narrative: format!("[{}] {}", self.id, e),
                    detection_surface: vec![],
                }
            }
        };
        let mut surface = Vec::new();
        let mut steps = Vec::new();
        let mut ok = true;
        for i in order {
            let r = self.nodes[i].run(&mut ctx, state, env);
            steps.push(format!("{}[{}]", self.nodes[i].id(), if r.success { "ok" } else { "FAIL" }));
            surface.extend(r.detection_surface);
            if !r.success {
                ok = false;
                break;
            }
        }
        Outcome {
            success: ok,
            narrative: format!("[{}] dependency order: {}", self.id, steps.join(" -> ")),
            detection_surface: surface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;
    use crate::facts::Requirement;
    use crate::card::Card;

    #[test]
    fn composite_precondition_uses_requires_data() {
        let c = CompositeCard {
            id: "t", side: Side::Red, technique: Technique::Kerberoast, summary: "t",
            category: Category::CredentialAccess,
            requires: vec![Requirement::have(crate::facts::Fact::ReachesDc)],
            produces: vec![], surface: vec![], nodes: vec![],
        };
        let mut s = GameState::new(vec![]);
        assert!(!c.precondition(&s), "ReachesDc false → illegal");
        s.add_zone("vlan30");
        assert!(c.precondition(&s), "ReachesDc true → legal");
    }
}
