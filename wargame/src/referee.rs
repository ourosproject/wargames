//! The referee — neutral game master and SOLE holder of ground truth. Each side sees only
//! its own [`AgentView`] (fog of war). Blue's active-defense posture (continuous monitoring,
//! active response) is applied here.
//!
//! A round is two phases: red acts, then blue reacts. `run_round` drives both from agents
//! (autonomous play); [`Referee::red_phase`] / [`Referee::blue_phase`] apply a single chosen
//! [`Move`] so a human can occupy either seat (see `session`).

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::card::{CardSpec, Environment, Move};
use crate::facts::{self, FactRow};
use crate::registry::CardRegistry;
use crate::rules::RuleSet;
use crate::state::{Alert, GameState, RoundScore, Side, Technique};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentView {
    pub side: Side,
    pub round: u32,
    pub legal: Vec<CardSpec>,
    pub holds_valid_cred: bool,
    pub my_techniques: Vec<Technique>,
    pub alerts: Vec<Alert>,
    pub my_detections: Vec<Technique>,
    pub honeytokens: u32,
    /// The engagement facts this side legitimately knows this turn (fog-of-war filtered) —
    /// the true/false checker that lets an agent reason about *why* a move is legal.
    pub facts: Vec<FactRow>,
}

pub trait Agent {
    fn choose(&mut self, view: &AgentView) -> Option<Move>;
}

/// Built-in no-model policy. Red climbs toward DA and switches tactics off its own feedback;
/// blue plays the active-defense game: get eyes, sever the win path, then lock the vectors.
pub struct HeuristicAgent {
    pub side: Side,
    rng: u64,
}
impl HeuristicAgent {
    pub fn new(side: Side) -> Self {
        Self::seeded(side, 0)
    }
    /// Seeded so a match is reproducible, but red's tempo/technique choice VARIES across seeds —
    /// that variation (plus the scenario's EDR maturity) is what makes the outcome a real contest
    /// rather than a fixed script. Red/blue draw from decorrelated streams of the same match seed.
    pub fn seeded(side: Side, seed: u64) -> Self {
        let salt = match side { Side::Red => 0x00C0FFEE_u64, Side::Blue => 0x00BEEF_u64 };
        Self { side, rng: seed ^ salt ^ 0x9E3779B97F4A7C15 }
    }
    fn next(&mut self) -> u64 {
        let mut x = if self.rng == 0 { 0x9E3779B97F4A7C15 } else { self.rng };
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
    /// Weighted-random choice among candidate card ids.
    fn pick(&mut self, cands: &[(&'static str, u32)]) -> Option<&'static str> {
        let total: u32 = cands.iter().map(|c| c.1).sum();
        if total == 0 {
            return None;
        }
        let mut r = (self.next() % total as u64) as u32;
        for (id, w) in cands {
            if r < *w {
                return Some(*id);
            }
            r -= *w;
        }
        cands.last().map(|c| c.0)
    }
}
impl Agent for HeuristicAgent {
    fn choose(&mut self, view: &AgentView) -> Option<Move> {
        if view.legal.is_empty() {
            return None;
        }
        let has = |id: &str| view.legal.iter().any(|s| s.id.as_str() == id);
        let mv = |id: &str| Some(Move { side: view.side, card: id.to_string(), params: json!({}) });

        match view.side {
            Side::Red => {
                // take the win when it's on the board; otherwise vary the setup order/tempo so
                // escalate unlocks at different rounds — the race blue is trying to beat.
                // Beeline to DA: take the win, else advance the kill chain by its shortest next step.
                if has("escalate_da") {
                    return mv("escalate_da");
                }
                // Can't touch AD from the outside — break in, then pivot through segmentation first.
                if has("initial_access") {
                    return mv("initial_access");
                }
                if has("pivot") {
                    return mv("pivot");
                }
                if !view.holds_valid_cred {
                    // No cred yet — roast for one. Which roast is a gamble: the wrong one for this
                    // scenario (Kerberoast vs an AES domain) fails and burns a round of tempo — the
                    // slice of non-determinism blue is racing against.
                    let mut c: Vec<(&'static str, u32)> = Vec::new();
                    if has("kerberoast") { c.push(("kerberoast", 45)); }
                    if has("asrep_roast") { c.push(("asrep_roast", 45)); }
                    if let Some(id) = self.pick(&c) {
                        return mv(id);
                    }
                }
                // Have a cred — map the path to DA, then escalate. Don't dawdle on recon.
                if has("bloodhound") {
                    return mv("bloodhound");
                }
                if has("recon") {
                    return mv("recon");
                }
                mv(&view.legal[0].id)
            }
            Side::Blue => {
                // eyes on → sever the crown-jewel path → lock vectors → hunt/rule for coverage
                for id in ["monitor", "segment", "remediate_acl", "hunt", "deploy_detection", "enforce_aes", "enforce_preauth", "active_response", "rotate_creds"] {
                    if has(id) {
                        return mv(id);
                    }
                }
                mv(&view.legal[0].id)
            }
        }
    }
}

/// Model-backed agent: an LLM picks one card from the legal menu; the [`HeuristicAgent`] is the
/// fallback for any failure (endpoint down, bad/illegal reply, timeout) so the engine stays
/// playable. A per-side instance = a separate conversation, so red and blue never share context
/// (the fog-of-war invariant holds — each only ever sees its own [`AgentView`]).
pub struct ModelAgent {
    side: Side,
    fallback: HeuristicAgent,
}
impl ModelAgent {
    pub fn seeded(side: Side, seed: u64) -> Self {
        Self { side, fallback: HeuristicAgent::seeded(side, seed) }
    }
    /// Query the model for a card id, or None on any failure.
    fn ask(&self, view: &AgentView) -> Option<String> {
        let role = match self.side {
            Side::Red => "You are RED, the attacker in an Active Directory range. Objective: reach Domain Admin. You begin OUTSIDE the network and must break in (initial_access), pivot through segmentation toward the domain, then abuse AD (roast a credential, map the path with bloodhound, escalate) to take DA. You can only attack the domain once you have traversed close enough to reach it.",
            Side::Blue => "You are BLUE, the defender of an Active Directory range. Objective: keep RED off Domain Admin. \
                 CRITICAL: detection alone does NOT stop RED — writing a rule or hunting only tells you what happened, it does not close the road to DA. You WIN by CUTTING THE PATH, not by watching RED walk it. RED loses on time; every turn you spend on passive coverage while a path-cut is legal is a turn RED uses to escalate. \
                 Reason ONLY from your own view (the alerts you can see, the detections you hold, the legal menu) — you cannot see RED's internal state. Follow this strict priority; always take the highest one whose card is on the legal menu this turn: \
                 (1) If 'monitor' is legal, play it FIRST — you are blind until monitoring is online and nothing else you do is reliable without it. \
                 (2) The MOMENT a path-cut is legal, take it — these end the game in your favor: 'remediate_acl' removes the DCSync/GenericAll route to Domain Admin (the single most important move — no path, no DA even if RED holds a cred), and 'segment' firewalls RED's frontier off while it is still pivoting toward the DC (act before RED reaches the domain — you cannot un-ring that bell). If both are legal, prefer 'remediate_acl'; take 'segment' while RED is still traversing. \
                 (3) Only once no path-cut is available, harden the vectors your alerts actually show: 'enforce_aes' (kills Kerberoast payoff), 'enforce_preauth' (kills AS-REP roasting), 'rotate_creds' (voids a stolen credential). \
                 (4) Only when none of the above is legal, spend the turn on coverage: 'hunt' to surface a technique that slipped past (this is also how you DETECT recon/bloodhound, which is what unlocks 'remediate_acl'), then 'deploy_detection' to write a rule for what you observed. \
                 'active_response' is a cheap arm-once — fine early, but it is NOT a substitute for a path-cut. Never idle on hunt/deploy_detection while 'remediate_acl' or 'segment' sits on the menu.",
        };
        let menu: Vec<serde_json::Value> = view.legal.iter().map(|c| json!({
            "card": c.id, "technique": c.technique.as_key(), "does": c.summary
        })).collect();
        // The engagement facts (the true/false checker) — a compact map of the yes/no truths
        // this side knows. Lets the model reason from state ("scout detected → remediate_acl is
        // unlocked; path not yet severed → still winnable") instead of a flat event blob.
        let engagement_facts: serde_json::Map<String, serde_json::Value> =
            view.facts.iter().map(|f| (f.fact.clone(), json!(f.holds))).collect();
        let situation = json!({
            "round": view.round,
            "you_hold_a_valid_cred": view.holds_valid_cred,
            "your_past_moves": view.my_techniques.iter().map(|t| t.as_key()).collect::<Vec<_>>(),
            "alerts_you_can_see": view.alerts.iter().map(|a| a.technique.as_key()).collect::<Vec<_>>(),
            "detections_you_hold": view.my_detections.iter().map(|t| t.as_key()).collect::<Vec<_>>(),
            "engagement_facts": engagement_facts,
        });
        let system = format!(
            "{role}\nTHICK ENGINE / THIN MODEL: each turn you choose exactly ONE card from the legal menu. \
             Respond with JSON only, no prose: {{\"card\":\"<id>\",\"why\":\"<short reason>\"}}. \
             The card value MUST be one of the menu ids."
        );
        let user = format!(
            "Your situation:\n{}\n\nLegal cards this turn (pick exactly one):\n{}\n\nChoose the single best card for your objective. JSON only.",
            serde_json::to_string_pretty(&situation).unwrap_or_default(),
            serde_json::to_string_pretty(&menu).unwrap_or_default(),
        );
        let reply = crate::model::chat_json(&system, &user)?;
        reply.get("card").and_then(|v| v.as_str()).map(|s| s.trim().to_string())
    }
}
impl Agent for ModelAgent {
    fn choose(&mut self, view: &AgentView) -> Option<Move> {
        if view.legal.is_empty() {
            return None;
        }
        let dbg = std::env::var("WARGAME_MODEL_DEBUG").is_ok();
        if let Some(card) = self.ask(view) {
            if view.legal.iter().any(|c| c.id == card) {
                if dbg { eprintln!("[model] {:?} chose {card}", view.side); }
                return Some(Move { side: view.side, card, params: json!({}) });
            }
            if dbg { eprintln!("[model] {:?} illegal reply '{card}' -> heuristic", view.side); }
        } else if dbg {
            eprintln!("[model] {:?} no/failed reply -> heuristic", view.side);
        }
        // endpoint down / bad or illegal reply → engine stays playable via the heuristic
        self.fallback.choose(view)
    }
}

pub struct RoundReport {
    pub round: u32,
    pub lines: Vec<String>,
    pub red_delta: i32,
    pub blue_delta: i32,
    pub finished: bool,
    pub winner: Option<Side>,
}

/// Result of applying ONE side's move (half a round). Deltas are already added to the
/// scoreboard totals; these are for the round-level report / feed.
pub struct PhaseReport {
    pub side: Side,
    pub lines: Vec<String>,
    pub red_delta: i32,
    pub blue_delta: i32,
    pub finished: bool,
    pub winner: Option<Side>,
}

pub struct Referee {
    pub rules: RuleSet,
    pub registry: CardRegistry,
}

impl Referee {
    /// Side-filtered view (fog of war) — the only thing an agent (or a human UI) may see.
    pub fn view_for(&self, side: Side, state: &GameState) -> AgentView {
        let legal = self.registry.legal(side, state);
        match side {
            Side::Red => AgentView {
                side, round: state.round, legal,
                holds_valid_cred: state.has_cracked_cred(),
                my_techniques: state.performed.clone(),
                alerts: vec![], my_detections: vec![], honeytokens: 0,
                facts: facts::table_for(side, state),
            },
            Side::Blue => AgentView {
                side, round: state.round, legal,
                holds_valid_cred: false, my_techniques: vec![],
                alerts: state.alerts.clone(),
                my_detections: state.detections.iter().map(|d| d.technique).collect(),
                honeytokens: state.honeytokens,
                facts: {
                    let mut f = facts::table_for(side, state);
                    f.extend(facts::blue_detection_rows(state));
                    f
                },
            },
        }
    }

    fn resolve_params(&self, mv: &Move, state: &GameState) -> serde_json::Value {
        let empty = mv.params.is_null() || mv.params.as_object().map(|o| o.is_empty()).unwrap_or(false);
        if empty {
            if let Some(c) = self.registry.get(&mv.card) {
                return c.default_params(state);
            }
        }
        mv.params.clone()
    }

    /// Start of a round — bump the counter. Call once before `red_phase`.
    pub fn begin_round(&self, state: &mut GameState) {
        state.round += 1;
    }

    /// Apply red's chosen move: run the card, then resolve what blue sees (specific rules, or
    /// everything if monitoring), honeytokens, stealth bonus, real Wazuh observations, and the
    /// DA win check. Scoreboard totals are updated here.
    pub fn red_phase(&self, state: &mut GameState, m: &Move, env: &mut dyn Environment) -> PhaseReport {
        let mut lines = Vec::new();
        let mut rd = 0;
        let mut bd = 0;
        let mut finished = false;
        let mut winner = None;

        let params = self.resolve_params(m, state);
        if let Some(card) = self.registry.get(&m.card) {
            let outcome = card.play(state, &params, env);
            state.performed.push(card.technique());
            if outcome.success {
                rd += self.rules.red_objective_points;
            }

            lines.push(format!("  🔴 RED   {}", outcome.narrative));
            lines.push(format!("         └─ \"{}\"", flavor(card.id())));

            // Each technique red exposes is a detection OPPORTUNITY. Record it, then see whether
            // blue actually catches it — via a deployed rule or loud-enough baseline telemetry.
            // No fiat omniscience: uncaught exposures become coverage GAPS in the report.
            let mut surface: Vec<Technique> = Vec::new();
            for t in outcome.detection_surface.iter().copied() {
                if !surface.contains(&t) {
                    surface.push(t);
                }
            }
            let mut caught: Vec<Technique> = Vec::new();
            for t in &surface {
                state.record_attack(*t);
                let by_rule = state.has_detection(*t);
                let by_baseline = state.monitoring && state.baseline_covers(*t);
                if by_rule || by_baseline {
                    caught.push(*t);
                    let src = if by_rule { "rule" } else { "baseline" };
                    state.mark_detected(*t, src);
                    state.alerts.push(Alert { round: state.round, technique: *t, source: src.into(), rule_id: format!("rule-{}", t.as_key()), level: 10 });
                }
            }

            if !caught.is_empty() {
                bd += self.rules.blue_detect_points;
                let names: Vec<&str> = caught.iter().map(|t| t.as_key()).collect();
                lines.push(format!("         🚨 detected ({}): {}", if caught.iter().all(|t| state.has_detection(*t)) { "rule" } else { "baseline" }, names.join(", ")));
                if state.auto_response {
                    let mut contained = vec![];
                    for i in 0..state.creds.len() {
                        if state.creds[i].cracked && caught.contains(&state.creds[i].via) {
                            state.creds[i].cracked = false;
                            contained.push(state.creds[i].principal.clone());
                        }
                    }
                    if !contained.is_empty() {
                        bd += self.rules.blue_contain_points;
                        lines.push(format!("         🐕 active response contained {}", contained.join(", ")));
                    }
                }
            }
            // Whatever red exposed that blue did NOT catch is a live coverage gap.
            let gaps: Vec<&str> = surface.iter().filter(|t| !caught.contains(t)).map(|t| t.as_key()).collect();
            if !gaps.is_empty() {
                if outcome.success {
                    rd += self.rules.red_stealth_bonus;
                }
                lines.push(format!("         ⚠ GAP — undetected: {}", gaps.join(", ")));
            }

            // Live: fold in real Wazuh observations (sim returns none). A real alert closes the
            // gap for that technique — records the true (possibly delayed) detection for MTTD.
            for a in env.observe(state) {
                state.mark_detected(a.technique, "wazuh");
                if !state.alerts.iter().any(|x| x.technique == a.technique && x.round == a.round) {
                    bd += self.rules.blue_detect_points;
                    lines.push(format!("         🛰  wazuh: real alert for {}", a.technique.as_key()));
                    state.alerts.push(a);
                }
            }

            if self.rules.red_wins_on_da && state.red_reached_da {
                finished = true;
                winner = Some(Side::Red);
            }
        }

        state.scoreboard.red += rd;
        state.scoreboard.blue += bd;
        PhaseReport { side: Side::Red, lines, red_delta: rd, blue_delta: bd, finished, winner }
    }

    /// Apply blue's chosen move: run the card and score it (contain vs detect).
    pub fn blue_phase(&self, state: &mut GameState, m: &Move, env: &mut dyn Environment) -> PhaseReport {
        let mut lines = Vec::new();
        let mut bd = 0;

        let params = self.resolve_params(m, state);
        if let Some(card) = self.registry.get(&m.card) {
            let outcome = card.play(state, &params, env);
            if outcome.success {
                bd += match card.id() {
                    "remediate_acl" | "enforce_aes" | "enforce_preauth" | "rotate_creds" => self.rules.blue_contain_points,
                    _ => self.rules.blue_detect_points,
                };
            }
            lines.push(format!("  🔵 BLUE  {}", outcome.narrative));
            lines.push(format!("         └─ \"{}\"", flavor(card.id())));
        }

        state.scoreboard.blue += bd;
        PhaseReport { side: Side::Blue, lines, red_delta: 0, blue_delta: bd, finished: false, winner: None }
    }

    /// End-of-round timeout: blue wins if red hasn't reached DA by the round cap.
    pub fn check_timeout(&self, state: &GameState) -> Option<Side> {
        if state.round >= self.rules.max_rounds {
            Some(Side::Blue)
        } else {
            None
        }
    }

    /// One full autonomous round: both sides chosen by their agents. Behavior is identical to
    /// the pre-phase implementation — the phase methods are the same code, just callable singly.
    pub fn run_round(&self, state: &mut GameState, red: &mut dyn Agent, blue: &mut dyn Agent, env: &mut dyn Environment) -> RoundReport {
        self.begin_round(state);
        let mut lines = Vec::new();
        let mut rd = 0;
        let mut bd = 0;
        let mut finished = false;
        let mut winner = None;

        match red.choose(&self.view_for(Side::Red, state)) {
            Some(m) => {
                let r = self.red_phase(state, &m, env);
                lines.extend(r.lines);
                rd += r.red_delta;
                bd += r.blue_delta;
                if r.finished {
                    finished = true;
                    winner = r.winner;
                }
            }
            None => lines.push("  🔴 RED   no legal moves — stalled".into()),
        }

        if !finished {
            match blue.choose(&self.view_for(Side::Blue, state)) {
                Some(m) => {
                    let b = self.blue_phase(state, &m, env);
                    lines.extend(b.lines);
                    bd += b.blue_delta;
                }
                None => lines.push("  🔵 BLUE  no legal moves".into()),
            }
            if let Some(w) = self.check_timeout(state) {
                finished = true;
                winner = Some(w);
            }
        }

        state.scoreboard.rounds.push(RoundScore { round: state.round, red_delta: rd, blue_delta: bd, note: String::new() });
        RoundReport { round: state.round, lines, red_delta: rd, blue_delta: bd, finished, winner }
    }
}

pub fn flavor(id: &str) -> &'static str {
    match id {
        "initial_access" => "knock knock — the edge was wide open",
        "pivot" => "one segment down. keep moving toward the crown.",
        "recon" => "let's see what we're working with...",
        "kerberoast" => "roasting service tickets like marshmallows",
        "asrep_roast" => "jbecker skipped pre-auth. rookie.",
        "bloodhound" => "the graph never lies — there's a path to DA",
        "escalate_da" => "DCSync — hand me krbtgt, I'll forge my own tickets",
        "monitor" => "eyes open. every packet, every process.",
        "active_response" => "you touch it, you lose it. instantly.",
        "remediate_acl" => "revoked that DCSync — no more krbtgt for you.",
        "enforce_aes" => "RC4 is dead here. crack AES, I'll wait.",
        "enforce_preauth" => "pre-auth on. no more free AS-REPs.",
        "rotate_creds" => "rotated — your tickets are worthless now",
        "deploy_detection" => "writing a rule for that — won't miss it twice",
        "hunt" => "not waiting for an alert — going looking",
        "segment" => "you're boxed in. that VLAN's a dead end now.",
        _ => "…",
    }
}
