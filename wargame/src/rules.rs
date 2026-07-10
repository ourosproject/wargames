//! The referee's `RuleSet` — the game's rules expressed as *data*, so they're adaptive
//! and user-editable (config file / dashboard / node-builder) without touching engine
//! code. Change the game by changing this struct's values, not by editing the referee.

use serde::{Deserialize, Serialize};

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
    pub red_wins_on_da: bool, // red reaching Domain Admin ends the game as a red win
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
            red_wins_on_da: true,
        }
    }
}
