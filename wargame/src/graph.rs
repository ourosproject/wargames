//! The dependency-graph layer — how a move's steps are *ordered* by their data dependencies.
//!
//! Each move is a set of steps wired by **dependency** (step B depends on step A because B reads
//! a blackboard key that A produces). [`resolve_order_keys`] resolves a legal execution order
//! from the steps' requires/produces key sets — the same elastic, dependency-ranked lattice idea
//! from the brain/forest, applied to move construction. [`Context`] is the blackboard passed
//! between steps at play time.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// Blackboard passed between steps — outputs of one node become inputs of dependents.
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

/// Topologically order items described only by their (requires, produces) blackboard keys.
/// Returns indices in a legal execution order, or an error on a cycle / missing input.
pub fn resolve_order_keys(reqs: &[Vec<String>], prods: &[Vec<String>], initial: &HashSet<String>) -> Result<Vec<usize>, String> {
    let n = reqs.len();
    let mut scheduled = vec![false; n];
    let mut available = initial.clone();
    let mut order = Vec::new();
    loop {
        let mut progressed = false;
        for i in 0..n {
            if scheduled[i] {
                continue;
            }
            if reqs[i].iter().all(|r| available.contains(r)) {
                scheduled[i] = true;
                for p in &prods[i] {
                    available.insert(p.clone());
                }
                order.push(i);
                progressed = true;
            }
        }
        if order.len() == n {
            break;
        }
        if !progressed {
            return Err("unsatisfiable dependency graph (cycle or missing input)".into());
        }
    }
    Ok(order)
}
