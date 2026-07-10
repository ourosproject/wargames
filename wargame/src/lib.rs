//! Purple Range — wargame engine.
//!
//! THICK ENGINE, THIN MODEL: this crate owns the round loop, the Rules of Engagement,
//! scoring, and ALL action execution (the card library). A pluggable model only picks
//! one card + params per turn from a legal menu; a built-in heuristic policy can play
//! with no model at all. The engine is the authority; the model is only an advisor.
//!
//! Extensibility is first-class:
//! - moves are authored as `.ron` data files, loaded and validated by [`arsenal`] (adding an
//!   attack/defense is one data file, no code edit);
//! - the referee's rules live in a data-driven [`RuleSet`] (retune the game, no code edit);
//! - the dependency-graph execution model ([`graph::resolve_order_keys`]) topologically orders
//!   each move's steps by their requires/produces blackboard keys.
//!
//! See `wargame/WARGAME.md` for the full design spec.

pub mod arsenal;
pub mod card;
pub mod category;
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
