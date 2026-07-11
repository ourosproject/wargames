//! The referee's `RuleSet` — the game's rules expressed as *data*, so they're adaptive
//! and user-editable (config file / dashboard / node-builder) without touching engine
//! code. Change the game by changing this struct's values, not by editing the referee.

use serde::{Deserialize, Serialize};

use crate::facts::{Fact, Requirement};

/// A way for red to win: red wins the moment EVERY requirement here holds at once. Reuses the
/// same condition alphabet that gates moves — victory and legality speak one language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinCondition {
    pub name: String,
    pub all_of: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    // ── scoring weights ──
    pub red_objective_points: i32, // red achieved an objective (cred, access, DA)
    pub red_stealth_bonus: i32,    // red's action produced no alert
    pub blue_detect_points: i32,   // blue detected a technique
    pub blue_contain_points: i32,  // blue contained / blocked / evicted
    pub blue_deceive_points: i32,  // red tripped a honeytoken / decoy

    // ── Rules of Engagement toggles ──
    pub enforce_no_overfit: bool,    // reject/penalize detections overfit to one run
    pub detect_before_respond: bool, // blue may only respond to observed techniques
    pub battlefield_frozen: bool,    // reject cards that alter base topology/firewall

    // ── win conditions ──
    pub max_rounds: u32,
    pub red_win_conditions: Vec<WinCondition>, // red wins if ANY condition is fully satisfied
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            red_objective_points: 10,
            red_stealth_bonus: 5,
            blue_detect_points: 8,
            blue_contain_points: 10,
            blue_deceive_points: 12,
            enforce_no_overfit: true,
            detect_before_respond: true,
            battlefield_frozen: true,
            max_rounds: 20,
            red_win_conditions: vec![WinCondition {
                name: "domain_admin".into(),
                all_of: vec![Requirement::have(Fact::DomainAdmin)],
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Fact, Requirement};

    #[test]
    fn default_ruleset_wins_on_domain_admin_only() {
        let rs = RuleSet::default();
        assert_eq!(rs.red_win_conditions.len(), 1);
        assert_eq!(rs.red_win_conditions[0].name, "domain_admin");
        assert_eq!(rs.red_win_conditions[0].all_of, vec![Requirement::have(Fact::DomainAdmin)]);
    }

    #[test]
    fn win_condition_is_a_conjunction_of_facts() {
        let wc = WinCondition {
            name: "silent_heist".into(),
            all_of: vec![Requirement::have(Fact::HasCred), Requirement::have(Fact::DomainAdmin)],
        };
        assert_eq!(wc.all_of.len(), 2);
    }
}
