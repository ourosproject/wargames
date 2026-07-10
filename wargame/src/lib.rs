//! Purple Range — wargame engine.
//!
//! THICK ENGINE, THIN MODEL: this crate owns the round loop, the Rules of Engagement,
//! scoring, and ALL action execution (the card library). A pluggable model only picks
//! one card + params per turn from a legal menu; a built-in heuristic policy can play
//! with no model at all. The engine is the authority; the model is only an advisor.
//!
//! Extensibility is first-class:
//! - cards register in a [`CardRegistry`] (adding an attack/defense is one line);
//! - the referee's rules live in a data-driven [`RuleSet`] (retune the game, no code edit);
//! - exploits/defenses can be built as [`graph::CompositeCard`]s — dependency graphs of
//!   function-node [`graph::Primitive`]s — the execution model the node-based builder plugs into.
//!
//! See `wargame/WARGAME.md` for the full design spec.

pub mod arsenal;
pub mod card;
pub mod category;
pub mod cards;
pub mod effects;
pub mod env;
pub mod facts;
pub mod graph;
pub mod history;
pub mod model;
pub mod referee;
pub mod registry;
pub mod rules;
pub mod scenario;
pub mod session;
pub mod state;
pub mod tool;

pub use card::{Card, CardSpec, Environment, Move, Outcome};
pub use referee::{Agent, AgentView, HeuristicAgent, Referee, RoundReport};
pub use registry::CardRegistry;
pub use rules::RuleSet;
pub use state::{Alert, Cred, Detection, GameState, Host, Scoreboard, Side, Technique};
