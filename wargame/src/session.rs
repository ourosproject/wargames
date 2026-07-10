//! Human-playable match sessions.
//!
//! A [`Match`] persists between HTTP requests so a human can take a seat (1P or 2P) and submit
//! one card per turn while the engine plays the other seat with a heuristic (or, later, a model).
//! A round is red-phase then blue-phase; [`Match::auto_advance`] plays through any agent phases
//! until it reaches a human's turn (or the game ends), so the human only ever acts on their turn.
//!
//! Fog of war: [`Match::view`] takes a perspective side and returns ONLY that side's legal hand,
//! plus a side-filtered feed. The referee remains the sole holder of ground truth.

use serde_json::{json, Value};

use crate::card::{Environment, Move};
use crate::referee::{Agent, Referee};
use crate::state::{GameState, RoundScore, Side, Technique};

/// Who occupies a side. The agent is boxed (+Send for axum's shared match store) so either the
/// heuristic or the model backend can play.
pub enum Seat {
    Human,
    Agent(Box<dyn Agent + Send>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Red,
    Blue,
    Finished,
}

pub struct Match {
    pub id: String,
    pub state: GameState,
    pub env: Box<dyn Environment>,
    pub referee: Referee,
    pub red: Seat,
    pub blue: Seat,
    pub phase: Phase,
    pub winner: Option<Side>,
    pub feed: Vec<String>,
    round_rd: i32,
    round_bd: i32,
    recorded: bool,
}

impl Match {
    pub fn new(id: String, red: Seat, blue: Seat, state: GameState, env: Box<dyn Environment>, referee: Referee) -> Self {
        let mut m = Self {
            id, state, env, referee, red, blue,
            phase: Phase::Red, winner: None, feed: Vec::new(), round_rd: 0, round_bd: 0,
            recorded: false,
        };
        m.referee.begin_round(&mut m.state);
        // Tee up the first human turn (plays red's opening move if red is an agent). Skip for
        // all-agent (spectate) matches so they don't run to completion at creation — use `step`.
        if m.has_human() {
            m.auto_advance();
        }
        m
    }

    fn seat(&self, side: Side) -> &Seat {
        match side {
            Side::Red => &self.red,
            Side::Blue => &self.blue,
        }
    }
    pub fn is_human(&self, side: Side) -> bool {
        matches!(self.seat(side), Seat::Human)
    }
    fn has_human(&self) -> bool {
        self.is_human(Side::Red) || self.is_human(Side::Blue)
    }
    pub fn awaiting(&self) -> Option<Side> {
        match self.phase {
            Phase::Red => Some(Side::Red),
            Phase::Blue => Some(Side::Blue),
            Phase::Finished => None,
        }
    }

    fn finish_round(&mut self) {
        self.state.scoreboard.rounds.push(RoundScore {
            round: self.state.round,
            red_delta: self.round_rd,
            blue_delta: self.round_bd,
            note: String::new(),
        });
        self.round_rd = 0;
        self.round_bd = 0;
    }

    fn apply_red(&mut self, m: &Move) {
        let r = self.referee.red_phase(&mut self.state, m, self.env.as_mut());
        self.feed.extend(r.lines);
        self.round_rd += r.red_delta;
        self.round_bd += r.blue_delta;
        if r.finished {
            self.winner = r.winner;
            self.finish_round();
            self.phase = Phase::Finished;
        } else {
            self.phase = Phase::Blue;
        }
    }

    fn apply_blue(&mut self, m: &Move) {
        let b = self.referee.blue_phase(&mut self.state, m, self.env.as_mut());
        self.feed.extend(b.lines);
        self.round_bd += b.blue_delta;
        if let Some(w) = self.referee.check_timeout(&self.state) {
            self.winner = Some(w);
            self.finish_round();
            self.phase = Phase::Finished;
        } else {
            self.finish_round();
            self.referee.begin_round(&mut self.state);
            self.phase = Phase::Red;
        }
    }

    fn apply_or_stall(&mut self, side: Side, mv: Option<Move>) {
        match mv {
            Some(m) => match side {
                Side::Red => self.apply_red(&m),
                Side::Blue => self.apply_blue(&m),
            },
            None => match side {
                Side::Red => {
                    self.feed.push("  🔴 RED   no legal moves — stalled".into());
                    self.phase = Phase::Blue;
                }
                Side::Blue => {
                    self.feed.push("  🔵 BLUE  no legal moves".into());
                    if let Some(w) = self.referee.check_timeout(&self.state) {
                        self.winner = Some(w);
                        self.finish_round();
                        self.phase = Phase::Finished;
                    } else {
                        self.finish_round();
                        self.referee.begin_round(&mut self.state);
                        self.phase = Phase::Red;
                    }
                }
            },
        }
    }

    fn agent_move(&mut self, side: Side) -> Option<Move> {
        let view = self.referee.view_for(side, &self.state);
        match side {
            Side::Red => if let Seat::Agent(a) = &mut self.red { a.choose(&view) } else { None },
            Side::Blue => if let Seat::Agent(a) = &mut self.blue { a.choose(&view) } else { None },
        }
    }

    /// Play through agent phases until it's a human's turn or the game ends.
    pub fn auto_advance(&mut self) {
        while let Some(side) = self.awaiting() {
            if self.is_human(side) {
                break;
            }
            let mv = self.agent_move(side);
            self.apply_or_stall(side, mv);
        }
    }

    /// Advance exactly one agent phase (spectator stepping). No-op if awaiting a human/finished.
    pub fn step(&mut self) {
        if let Some(side) = self.awaiting() {
            if !self.is_human(side) {
                let mv = self.agent_move(side);
                self.apply_or_stall(side, mv);
            }
        }
    }

    /// Submit a human move for `side`. Validates it's that side's turn, that the seat is human,
    /// and that the card is currently legal; then applies it and auto-advances agent responses.
    pub fn submit(&mut self, side: Side, card: String, params: Value) -> Result<(), String> {
        if self.awaiting() != Some(side) {
            return Err(format!("not {}'s turn", side_key(side)));
        }
        if !self.is_human(side) {
            return Err("that seat is not human".into());
        }
        if !self.referee.registry.legal(side, &self.state).iter().any(|s| s.id == card) {
            return Err(format!("'{card}' is not a legal move for {} right now", side_key(side)));
        }
        let m = Move { side, card, params };
        match side {
            Side::Red => self.apply_red(&m),
            Side::Blue => self.apply_blue(&m),
        }
        self.auto_advance();
        Ok(())
    }

    /// Legal hand for a side (fog of war — the only cards that side may pick).
    pub fn legal(&self, side: Side) -> Vec<Value> {
        self.referee
            .registry
            .legal(side, &self.state)
            .iter()
            .map(|s| json!({
                "id": s.id, "side": format!("{:?}", s.side), "technique": s.technique.as_key(),
                "summary": s.summary, "params_schema": s.params_schema,
            }))
            .collect()
    }

    fn visible_feed(&self, perspective: Option<Side>) -> Vec<String> {
        match perspective {
            None => self.feed.clone(),
            // Red sees only its own actions + stealth confirmations — never blue's detections.
            Some(Side::Red) => self
                .feed
                .iter()
                .filter(|l| {
                    let t = l.trim_start();
                    t.starts_with('🔴') || t.starts_with('👻')
                })
                .cloned()
                .collect(),
            // Blue sees everything it observes — but not red's private "slipped past unseen".
            Some(Side::Blue) => self
                .feed
                .iter()
                .filter(|l| !l.trim_start().starts_with('👻'))
                .cloned()
                .collect(),
        }
    }

    /// Append this match's coverage to the history log exactly once, when it finishes.
    /// Idempotent — safe to call after every handler advance.
    pub fn record_if_finished(&mut self) {
        if self.phase == Phase::Finished && !self.recorded {
            let winner = self.winner.map(side_key).unwrap_or("");
            crate::history::record(&self.state, winner, self.env.kind(), self.state.round);
            self.recorded = true;
        }
    }

    /// Side-filtered snapshot for the UI. `perspective` = the seat asking (None = spectator).
    pub fn view(&self, perspective: Option<Side>) -> Value {
        let awaiting = self.awaiting();
        json!({
            "id": self.id,
            "round": self.state.round,
            "phase": match self.phase { Phase::Red => "red", Phase::Blue => "blue", Phase::Finished => "finished" },
            "awaiting": awaiting.map(side_key),
            "your_turn": perspective.is_some() && awaiting == perspective,
            "finished": self.phase == Phase::Finished,
            "winner": self.winner.map(side_key),
            "red": self.state.scoreboard.red,
            "blue": self.state.scoreboard.blue,
            "mode": self.env.kind(),
            "bf": {
                "red_cred": self.state.has_cracked_cred(),
                "bloodhound": self.state.performed_technique(Technique::BloodHound),
                "dc_owned": self.state.red_reached_da,
                "blue_watching": !self.state.detections.is_empty() || self.state.monitoring,
                "alerts": self.state.alerts.len(),
                "honeytokens": self.state.honeytokens,
                "zones": self.state.red_zones,
                "edges": self.state.edges,
                "attack_ready": self.state.attack_ready(),
            },
            "legal": perspective.map(|s| self.legal(s)).unwrap_or_default(),
            "feed": self.visible_feed(perspective),
            "coverage": serde_json::to_value(self.state.coverage()).unwrap_or(Value::Null),
            "scenario": self.state.scenario,
            "seed": self.state.seed,
            "topo": {
                "entry": self.state.zone_path.first().cloned().unwrap_or_default(),
                "objective": self.state.objective_zone,
                "path": self.state.zone_path,
                "edges": self.state.edges,
            },
        })
    }
}

pub fn side_key(s: Side) -> &'static str {
    match s {
        Side::Red => "red",
        Side::Blue => "blue",
    }
}

pub fn side_from_key(s: &str) -> Option<Side> {
    match s {
        "red" => Some(Side::Red),
        "blue" => Some(Side::Blue),
        _ => None,
    }
}
