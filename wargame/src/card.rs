//! Cards = the coded action library. Every red technique and blue response is a `Card`.
//!
//! The model only *chooses* a card + params; the card's code *executes* it. Execution
//! always goes through an [`Environment`] backend (simulator or live range) — never the
//! model. This is the "thick engine, thin model" boundary.
//!
//! Extensibility (per the node-builder goal): a card only has to produce a [`CardSpec`]
//! and implement `play`. Register it and it's instantly live everywhere — the referee's
//! legal-move menu, the thin model's choices, the dashboard, and the node builder.

use serde::{Deserialize, Serialize};

use crate::category::Category;
use crate::facts::{Fact, Requirement};
use crate::state::{Alert, GameState, Side, Technique};

/// A move an agent (model / heuristic / human) submits: pick one card + its params.
/// This is the *only* thing the thin model ever produces, and it is schema-constrained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    pub side: Side,
    pub card: String,              // matches `Card::id`
    pub params: serde_json::Value, // card-specific, constrained by `CardSpec::params_schema`
}

/// Result of executing a card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub success: bool,
    pub narrative: String,
    /// Techniques this action exposed on the wire/host — what blue *could* detect.
    pub detection_surface: Vec<Technique>,
}

/// Introspectable metadata for a card — the single source of truth that the model menu,
/// the dashboard, and the node-based builder all read. Adding a card = registering
/// something that produces one of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSpec {
    pub id: String,
    pub side: Side,
    pub technique: Technique,
    pub category: Category,
    pub summary: String,
    /// JSON Schema for this card's params — used to constrain the thin model's output.
    pub params_schema: serde_json::Value,
}

/// The execution backend. `SimEnvironment` computes outcomes from a model of the range;
/// `LiveEnvironment` performs the real action over SSH and reads Wazuh. Swappable so the
/// game logic can be developed and replayed without the live lab.
pub trait Environment: Send {
    fn kind(&self) -> &'static str;

    /// Realize a card/primitive's effect, keyed by its `id`. In [`SimEnvironment`] this is a
    /// deterministic stand-in that returns success with an empty narrative (so the caller keeps
    /// its own flavor and the sim game is unchanged). In [`LiveEnvironment`] this SSH/wmiexec-
    /// executes the real command on the range and reports what actually happened — the card
    /// then gates its game bookkeeping on that real result.
    fn act(&mut self, action: &str, params: &serde_json::Value, state: &GameState) -> Outcome;

    /// After red's action, pull the alerts blue can actually see this round. Live: real Wazuh.
    /// Sim: none — the referee models sim detection inline from the action's detection surface.
    fn observe(&mut self, state: &GameState) -> Vec<Alert>;
}

/// Fire a card's effect through the environment, folding the real (or simulated) result into a
/// game [`Outcome`]. The card supplies `fallback` (its own narrative, used when the env returns
/// none, i.e. in sim) and the authoritative `surface` (its ATT&CK exposure). The caller applies
/// its state bookkeeping only when the returned outcome is `success`.
pub fn realize(
    env: &mut dyn Environment,
    action: &str,
    params: &serde_json::Value,
    state: &GameState,
    fallback: &str,
    surface: Vec<Technique>,
) -> Outcome {
    let r = env.act(action, params, state);
    Outcome {
        success: r.success,
        narrative: if r.narrative.trim().is_empty() { fallback.to_string() } else { r.narrative },
        detection_surface: surface,
    }
}

/// A card in the library. Object-safe so cards live in `Vec<Box<dyn Card>>`.
/// `Send + Sync` so the registry can cross `.await` points (SSE, async server).
pub trait Card: Send + Sync {
    fn id(&self) -> &'static str;
    fn side(&self) -> Side;
    fn technique(&self) -> Technique;
    fn describe(&self) -> &'static str;

    /// JSON Schema for params (default: no params). Cards with parameters override this.
    fn params_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    /// A sane default params value for heuristic / quick play (default: empty object).
    fn default_params(&self, _state: &GameState) -> serde_json::Value {
        serde_json::json!({})
    }

    /// Tier-1 kill-chain category this card belongs to / counters.
    /// Required — every card declares it (compile error otherwise).
    fn category(&self) -> Category;

    /// Declarative legality — the facts/probes that must hold. Provided `precondition`
    /// evaluates these; cards should override this, not `precondition`.
    fn requires(&self) -> Vec<Requirement> { vec![] }

    /// Facts this card flips true on success (for the forest/builder; not consumed yet).
    fn produces(&self) -> Vec<Fact> { vec![] }

    /// Declared ATT&CK exposure — the instance signature blue could detect.
    fn detection_surface(&self) -> Vec<Technique> { vec![] }

    /// Legal in this state? Provided: all requirements satisfied. Overridable for cards not
    /// yet migrated to `requires()`.
    fn precondition(&self, state: &GameState) -> bool {
        self.requires().iter().all(|r| r.satisfied(state))
    }

    /// Execute the card: run it through the environment, mutating game state.
    fn play(
        &self,
        state: &mut GameState,
        params: &serde_json::Value,
        env: &mut dyn Environment,
    ) -> Outcome;

    /// Introspectable metadata (provided — built from the accessors above).
    fn spec(&self) -> CardSpec {
        CardSpec {
            id: self.id().to_string(),
            side: self.side(),
            technique: self.technique(),
            category: self.category(),
            summary: self.describe().to_string(),
            params_schema: self.params_schema(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;

    // A card that declares requirements but no precondition override uses the provided path.
    struct Dummy;
    impl Card for Dummy {
        fn id(&self) -> &'static str { "dummy" }
        fn side(&self) -> Side { Side::Blue }
        fn technique(&self) -> Technique { Technique::Recon }
        fn category(&self) -> Category { Category::Detection }
        fn describe(&self) -> &'static str { "dummy" }
        fn requires(&self) -> Vec<crate::facts::Requirement> {
            vec![crate::facts::Requirement::lack(crate::facts::Fact::Monitoring)]
        }
        fn play(&self, _s: &mut GameState, _p: &serde_json::Value, _e: &mut dyn Environment) -> Outcome {
            Outcome { success: true, narrative: String::new(), detection_surface: vec![] }
        }
    }

    #[test]
    fn provided_precondition_evaluates_requires() {
        let mut s = GameState::new(vec![]);
        assert!(Dummy.precondition(&s), "monitoring off → lack(Monitoring) satisfied");
        s.monitoring = true;
        assert!(!Dummy.precondition(&s), "monitoring on → lack(Monitoring) fails");
    }

    #[test]
    fn spec_carries_category() {
        assert_eq!(Dummy.spec().category, Category::Detection);
    }
}
