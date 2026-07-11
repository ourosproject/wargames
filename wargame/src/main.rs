//! Purple Range — wargame binary.
//!
//! Default: serve the live "watch the match" dashboard (axum + SSE, HTML embedded).
//! `cargo run -- cli`: run one game to the terminal instead.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    response::{sse::Event, Html, Sse},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use purple_wargame::card::Environment;
use purple_wargame::arsenal::{default_registry, authored_dir, registry_with_authored, vocabulary};
use purple_wargame::builder::{self, MoveDraft};
use purple_wargame::env::{LiveEnvironment, SimEnvironment};
use purple_wargame::referee::{Agent, HeuristicAgent, ModelAgent, Referee};
use purple_wargame::session::{side_from_key, Match, Seat};
use purple_wargame::{history, model, scenario, GameState, Host, RuleSet, Side, Technique};

/// Shared store of in-progress human-playable matches, keyed by id.
type Matches = Arc<Mutex<HashMap<String, Match>>>;

/// Pick the backend: `LiveEnvironment` fires real range actions when live mode is on.
fn make_env(live: bool) -> Box<dyn Environment> {
    if live {
        Box::new(LiveEnvironment::new())
    } else {
        Box::new(SimEnvironment::new())
    }
}

fn live_mode() -> bool {
    std::env::var("WARGAME_LIVE").ok().as_deref() == Some("1")
}

/// Whether agent seats should be model-backed by default (env). Per-call flags can also force it.
fn model_mode() -> bool {
    matches!(std::env::var("WARGAME_MODEL_ON").ok().as_deref(), Some("1") | Some("true"))
}

/// Build an agent seat: model-backed (LLM picks, heuristic fallback) when `use_model`, else pure
/// heuristic. Both are reproducible from the match seed.
fn make_agent(side: Side, seed: u64, use_model: bool) -> Box<dyn Agent + Send> {
    if use_model {
        Box::new(ModelAgent::seeded(side, seed))
    } else {
        Box::new(HeuristicAgent::seeded(side, seed))
    }
}

fn new_game_state() -> GameState {
    GameState::new(vec![
        Host { id: "kali".into(), zone: "VLAN10".into(), label: "kali".into(), foothold: true, reachable_by_red: true },
        Host { id: "dc01".into(), zone: "VLAN30".into(), label: "dc01".into(), foothold: false, reachable_by_red: true },
    ])
}

fn new_referee() -> Referee {
    Referee { rules: RuleSet { max_rounds: 8, ..RuleSet::default() }, registry: default_registry() }
}

/// Referee for human-playable matches: built-ins PLUS authored moves. Kept separate from
/// `new_referee` (built-ins only), which stays wired to `run_cli` and the autonomous `game` SSE
/// so the balance guard and the AI demo never see experimental moves.
fn play_referee() -> Referee {
    Referee { rules: RuleSet { max_rounds: 8, ..RuleSet::default() }, registry: registry_with_authored(&authored_dir()) }
}

/// Build a fresh game state with a seeded scenario applied — this is what makes each match a
/// different environment (planted vulns + EDR maturity vary; reproducible from the seed).
fn game_state_for(seed: u64) -> GameState {
    let sc = scenario::pick(seed);
    let mut st = new_game_state();
    st.scenario = sc.name.to_string();
    st.seed = seed;
    st.misconfigs = sc.misconfigs.to_vec();
    st.baseline = sc.baseline.to_vec();
    // Apply the scenario's segmentation: red starts on the edge and must traverse in.
    st.red_zones = vec![sc.topo.entry.to_string()];
    st.objective_zone = sc.topo.objective.to_string();
    st.edges = sc.topo.edges.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    st.zone_path = ordered_zones(&st); // immutable layout, captured before any edge is cut
    st
}

/// The zones from the entry to the objective in traversal order, so the dashboard can lay the
/// segmentation out as a left-to-right front. Follows the (linear) edge chain from red's start.
fn ordered_zones(st: &GameState) -> Vec<String> {
    let mut path = Vec::new();
    let mut cur = st.red_zones.first().cloned().unwrap_or_default();
    let mut guard = 0;
    while !cur.is_empty() && guard < 16 {
        path.push(cur.clone());
        if cur == st.objective_zone { break; }
        let next = st.edges.iter().find(|(f, _)| f == &cur).map(|(_, t)| t.clone());
        match next { Some(n) => cur = n, None => break }
        guard += 1;
    }
    path
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("m{n:x}")
}

/// Build a match for a mode: "spectate" (both agents), "1p" (+ side the human takes), "2p".
fn make_match(mode: &str, side: &str, live: bool, use_model: bool) -> Match {
    let seed = scenario::fresh_seed();
    let agent = |s| Seat::Agent(make_agent(s, seed, use_model));
    let (red, blue) = match mode {
        "1p" => {
            if side == "blue" { (agent(Side::Red), Seat::Human) } else { (Seat::Human, agent(Side::Blue)) }
        }
        "2p" => (Seat::Human, Seat::Human),
        _ => (agent(Side::Red), agent(Side::Blue)),
    };
    Match::new(new_id(), red, blue, game_state_for(seed), make_env(live), play_referee())
}

fn err_json(msg: impl Into<String>) -> Json<Value> {
    Json(json!({ "error": msg.into() }))
}

// ── human-playable match endpoints ──────────────────────────────────────────────────

/// POST /api/match  { mode, side, live }  → create a session, return the caller's view.
async fn new_match(State(store): State<Matches>, Json(req): Json<Value>) -> Json<Value> {
    let mode = req.get("mode").and_then(|v| v.as_str()).unwrap_or("spectate");
    let side = req.get("side").and_then(|v| v.as_str()).unwrap_or("red");
    let live = req.get("live").and_then(|v| v.as_bool()).unwrap_or(false) || live_mode();
    let use_model = req.get("model").and_then(|v| v.as_bool()).unwrap_or(false) || model_mode();
    let m = make_match(mode, side, live, use_model);
    // Perspective: the human's side (1p), else none (spectate/2p pick per request).
    let perspective = match mode {
        "1p" => side_from_key(side),
        _ => None,
    };
    let view = m.view(perspective);
    store.lock().unwrap().insert(m.id.clone(), m);
    Json(view)
}

/// GET /api/match/:id?side=red|blue  → the current view from a seat's perspective.
async fn get_match(State(store): State<Matches>, Path(id): Path<String>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let perspective = q.get("side").and_then(|s| side_from_key(s));
    match store.lock().unwrap().get(&id) {
        Some(m) => Json(m.view(perspective)),
        None => err_json("no such match"),
    }
}

/// GET /api/legal?match=<id>&side=red|blue  → that side's legal hand.
async fn legal(State(store): State<Matches>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let id = q.get("match").cloned().unwrap_or_default();
    let side = match q.get("side").and_then(|s| side_from_key(s)) {
        Some(s) => s,
        None => return err_json("side must be red|blue"),
    };
    match store.lock().unwrap().get(&id) {
        Some(m) => Json(json!({ "match": id, "side": q.get("side"), "legal": m.legal(side) })),
        None => err_json("no such match"),
    }
}

/// POST /api/move  { match, side, card, params }  → apply a human move, return updated view.
async fn do_move(State(store): State<Matches>, Json(req): Json<Value>) -> Json<Value> {
    let id = req.get("match").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let side = match req.get("side").and_then(|v| v.as_str()).and_then(side_from_key) {
        Some(s) => s,
        None => return err_json("side must be red|blue"),
    };
    let card = match req.get("card").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return err_json("card required"),
    };
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
    let mut guard = store.lock().unwrap();
    let m = match guard.get_mut(&id) {
        Some(m) => m,
        None => return err_json("no such match"),
    };
    match m.submit(side, card, params) {
        Ok(()) => {
            m.record_if_finished();
            Json(m.view(Some(side)))
        }
        Err(e) => {
            let mut v = m.view(Some(side));
            if let Some(obj) = v.as_object_mut() {
                obj.insert("error".into(), json!(e));
            }
            Json(v)
        }
    }
}

/// POST /api/step  { match, side? }  → advance one agent phase (spectator stepping).
async fn step_match(State(store): State<Matches>, Json(req): Json<Value>) -> Json<Value> {
    let id = req.get("match").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let perspective = req.get("side").and_then(|v| v.as_str()).and_then(side_from_key);
    let mut guard = store.lock().unwrap();
    match guard.get_mut(&id) {
        Some(m) => {
            m.step();
            m.record_if_finished();
            Json(m.view(perspective))
        }
        None => err_json("no such match"),
    }
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../public/wargame.html"))
}

/// GET /api/history?n=20 → recent finished-match coverage records (newest first).
async fn history_api(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let n = q.get("n").and_then(|s| s.parse::<usize>().ok()).unwrap_or(20).clamp(1, 200);
    Json(json!({ "matches": history::recent(n) }))
}

/// The Claude-designed dashboard prototype (React/Babel, currently simulated) served
/// same-origin so it can be wired to the live /api/* endpoints.
async fn design_page() -> Html<&'static str> {
    Html(include_str!("../public/design.html"))
}

/// Vendored anime.js v4 (UMD) — served locally so the dashboard stays self-contained / air-gapped.
async fn anime_js() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../public/anime.min.js"),
    )
}

async fn vocabulary_api() -> Json<Value> {
    Json(vocabulary())
}

async fn list_tools() -> Json<Value> {
    Json(builder::list(&authored_dir()))
}

async fn save_tool(Json(draft): Json<MoveDraft>) -> Json<Value> {
    let dir = authored_dir();
    let reg = registry_with_authored(&dir);
    match builder::save(&dir, &draft, &reg) {
        Ok(def) => Json(json!({ "ok": true, "id": def.id })),
        Err(errs) => Json(json!({ "ok": false, "errors": errs })),
    }
}

/// Non-destructive validation for the "Validate" button — checks the draft but writes nothing.
async fn validate_tool(Json(draft): Json<MoveDraft>) -> Json<Value> {
    let reg = registry_with_authored(&authored_dir());
    match builder::check(&draft, &reg) {
        Ok(def) => Json(json!({ "ok": true, "id": def.id })),
        Err(errs) => Json(json!({ "ok": false, "errors": errs })),
    }
}

async fn delete_tool(Path(id): Path<String>) -> Json<Value> {
    match builder::delete(&authored_dir(), &id) {
        Ok(()) => Json(json!({ "ok": true })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

async fn build_page() -> Html<&'static str> {
    Html(include_str!("../public/build.html"))
}

async fn canvas_page() -> Html<&'static str> {
    Html(include_str!("../public/canvas.html"))
}

async fn catalog() -> Json<serde_json::Value> {
    let reg = registry_with_authored(&authored_dir());
    let cards: Vec<serde_json::Value> = reg
        .all_specs()
        .iter()
        .map(|s| json!({ "id": s.id, "side": format!("{:?}", s.side), "technique": s.technique.as_key(), "attack_id": s.technique.attack_id(), "attack_name": s.technique.attack_name(), "summary": s.summary }))
        .collect();
    Json(json!({ "cards": cards }))
}

/// Stream a fresh autonomous game round-by-round (paced for drama).
async fn game(Query(q): Query<HashMap<String, String>>) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    // `?live=1` runs against the real range; `?model=1` lets the LLM pick cards (heuristic fallback).
    let req_live = q.get("live").map(|v| v == "1").unwrap_or(false);
    let req_model = q.get("model").map(|v| v == "1").unwrap_or(false);
    let stream = async_stream::stream! {
        let seed = scenario::fresh_seed();
        let mut state = game_state_for(seed);
        let referee = new_referee();
        let use_model = req_model || model_mode();
        let mut red = make_agent(Side::Red, seed, use_model);
        let mut blue = make_agent(Side::Blue, seed, use_model);
        let live = req_live || live_mode();
        let mut env = make_env(live);

        yield Ok(Event::default().event("start").data(
            json!({
                "battlefield": "kali · VLAN10   →   dc01 · VLAN30 · range.local",
                "objective": "Domain Admin",
                "mode": if live { "LIVE — firing on the homelab range" } else { "sim" },
                "brain": if use_model { model::default_model() } else { "heuristic".into() },
                "scenario": state.scenario,
                "seed": state.seed,
                // the segmentation red must cross, so the UI can lay out the external→internal front
                "topo": {
                    "entry": state.red_zones.first().cloned().unwrap_or_default(),
                    "objective": state.objective_zone,
                    "path": state.zone_path,
                    "edges": state.edges,
                },
            }).to_string(),
        ));

        loop {
            tokio::time::sleep(Duration::from_millis(750)).await;
            let rep = referee.run_round(&mut state, red.as_mut(), blue.as_mut(), env.as_mut());
            let winner = match rep.winner {
                Some(Side::Red) => "red",
                Some(Side::Blue) => "blue",
                None => "",
            };
            let payload = json!({
                "round": rep.round,
                "lines": rep.lines,
                "red": state.scoreboard.red,
                "blue": state.scoreboard.blue,
                "finished": rep.finished,
                "winner": winner,
                "bf": {
                    "red_cred": state.has_cracked_cred(),
                    "bloodhound": state.performed_technique(Technique::BloodHound),
                    "dc_owned": state.red_reached_da,
                    "blue_watching": !state.detections.is_empty(),
                    "alerts": state.alerts.len(),
                    "honeytokens": state.honeytokens,
                    // topology (deep-#4): where red stands, the edges still open, whether it can reach AD
                    "zones": state.red_zones,
                    "edges": state.edges,
                    "attack_ready": state.attack_ready(),
                },
                "coverage": serde_json::to_value(state.coverage()).unwrap_or(Value::Null),
            });
            yield Ok(Event::default().event("round").data(payload.to_string()));
            if rep.finished {
                history::record(&state, winner, if live { "live" } else { "sim" }, rep.round);
                break;
            }
        }
        yield Ok(Event::default().event("end").data("{}"));
    };
    Sse::new(stream)
}

/// Roll dc01 back to the `wargame_baseline` snapshot — the per-match reset (a live match
/// really hardens the DC). Lab-specific: VM107 on the Proxmox host via the expect helper.
async fn reset_range() -> Json<serde_json::Value> {
    let home = std::env::var("HOME").unwrap_or_default();
    let ops = format!("{home}/Developer/development/proxmox-bench/redteam-ops");
    // Heal the RB3011 first: drop any WARGAME-SEG rules + the dead-man's-switch, so a reset
    // restores full lab connectivity even if a live match left segmentation in place.
    {
        let rosseg = format!("{ops}/rosseg.sh");
        let _ = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&rosseg).arg("revert").output()
        })
        .await;
    }
    let exp = format!("{ops}/labpass.exp");
    let res = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&exp)
            .args([
                "1017", "ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=10", "root@192.168.88.10",
                "qm rollback 107 wargame_baseline --start",
            ])
            .output()
    })
    .await;
    let ok = matches!(&res, Ok(Ok(o)) if o.status.success());
    Json(json!({
        "ok": ok,
        "msg": if ok { "dc01 rolling back to baseline — booting (~90s)" } else { "reset failed — check lab transport" },
    }))
}

fn run_cli(live: bool, use_model: bool) {
    let seed = scenario::fresh_seed();
    let mut state = game_state_for(seed);
    let referee = new_referee();
    let mut red = make_agent(Side::Red, seed, use_model);
    let mut blue = make_agent(Side::Blue, seed, use_model);
    let mut env = make_env(live);
    println!("PURPLE RANGE · WARGAME   RED vs BLUE");
    println!("  mode: {}", if live { "LIVE — firing real actions on the homelab range" } else { "sim (offline)" });
    println!("  brain: {}", if use_model { model::default_model() } else { "heuristic".into() });
    println!("  scenario: {} (seed {})\n", state.scenario, state.seed);
    loop {
        let rep = referee.run_round(&mut state, red.as_mut(), blue.as_mut(), env.as_mut());
        println!("── Round {} ──", rep.round);
        for l in &rep.lines {
            println!("{}", l);
        }
        println!("   score:  RED {}  BLUE {}\n", state.scoreboard.red, state.scoreboard.blue);
        if rep.finished {
            let cov = state.coverage();
            println!("── DETECTION COVERAGE ──");
            for r in &cov.rows {
                let flag = match r.fidelity.as_str() { "overfit" => "  ⚑ OVERFIT (won't generalize — refine)", "noisy" => "  ⚑ NOISY (false positives — refine)", _ => "" };
                println!("  {:<10} {:<11} fired {}  {}{}", r.attack_id, r.technique, r.fired,
                    if r.gap { format!("⚠ GAP → write a rule on {}", r.data_source) }
                    else { format!("✓ {} via {}{}", r.detected, r.source, r.latency.map(|l| format!(" ({l}r)")).unwrap_or_default()) },
                    flag);
            }
            println!("  coverage: {}% raw · {}% ROBUST ({}/{} techniques) · gaps: [{}]",
                cov.coverage_pct, cov.robust_pct, cov.techniques_detected, cov.techniques_fired, cov.gaps.join(", "));
            if !cov.overfit.is_empty() || !cov.noisy.is_empty() {
                println!("  ⚑ refine rules — overfit: [{}]  noisy: [{}]", cov.overfit.join(", "), cov.noisy.join(", "));
            }
            println!();
            let winner = match rep.winner {
                Some(Side::Red) => {
                    let why = if state.win_reason.is_empty() { "Domain Admin".to_string() } else { state.win_reason.replace('_', " ") };
                    println!("RED WINS — {why} in {} rounds.", rep.round);
                    "red"
                }
                Some(Side::Blue) => { println!("BLUE WINS — held the line {} rounds.", rep.round); "blue" }
                None => "",
            };
            history::record(&state, winner, if live { "live" } else { "sim" }, rep.round);
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "live") || live_mode();
    if live {
        // so the SSE game handler (which reads the env) also runs live
        std::env::set_var("WARGAME_LIVE", "1");
    }
    let use_model = args.iter().any(|a| a == "model") || model_mode();
    if use_model {
        // so the SSE game handler (which reads the env) is model-backed too
        std::env::set_var("WARGAME_MODEL_ON", "1");
    }
    if args.get(1).map(|s| s.as_str()) == Some("cli") {
        run_cli(live, use_model);
        return;
    }
    // Fire a single control at the LIVE range and print the real outcome. Operator tool +
    // per-action verification of the live wiring:  cargo run -- fire enforce_aes
    if args.get(1).map(|s| s.as_str()) == Some("fire") {
        let action = args.get(2).cloned().unwrap_or_default();
        let mut env = LiveEnvironment::new();
        let state = new_game_state();
        let o = env.act(&action, &json!({}), &state);
        println!("action  = {action}\nsuccess = {}\noutcome = {}", o.success, o.narrative);
        return;
    }
    let matches: Matches = Arc::new(Mutex::new(HashMap::new()));
    let app = Router::new()
        .route("/", get(index))
        .route("/design", get(design_page))
        .route("/build", get(build_page))
        .route("/canvas", get(canvas_page))
        .route("/anime.min.js", get(anime_js))
        .route("/api/vocabulary", get(vocabulary_api))
        .route("/api/tools", get(list_tools).post(save_tool))
        .route("/api/tools/validate", post(validate_tool))
        .route("/api/tools/:id", axum::routing::delete(delete_tool))
        .route("/api/catalog", get(catalog))
        .route("/api/game", get(game))
        .route("/api/history", get(history_api))
        .route("/api/reset", post(reset_range))
        .route("/api/match", post(new_match))
        .route("/api/match/:id", get(get_match))
        .route("/api/legal", get(legal))
        .route("/api/move", post(do_move))
        .route("/api/step", post(step_match))
        .with_state(matches);

    let port: u16 = std::env::var("WARGAME_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4850);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    println!("\n  ◈  Purple Range · Wargame dashboard  →  http://localhost:{port}\n");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
