//! Interactive setup wizard — `purple-range setup`.
//! Asks a few questions and writes range.config.json (+ .env for passwords).

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{self, Write};

fn ask(prompt: &str) -> String {
    print!("{prompt}");
    io::stdout().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s).ok();
    s.trim().to_string()
}
fn ask_or(prompt: &str, default: &str) -> String {
    let a = ask(prompt);
    if a.is_empty() { default.to_string() } else { a }
}

pub fn run() {
    println!("\n  ◈ Purple Range setup — a few questions build your range.config.json\n");
    let name = ask_or("  Range name [My Homelab Range]: ", "My Homelab Range");
    let port: u16 = ask("  Dashboard port [4899]: ").parse().unwrap_or(4899);
    let ssh_cfg = ask_or("  SSH config file for key-auth hosts [~/.ssh/config]: ", "~/.ssh/config");

    let mut hosts: Map<String, Value> = Map::new();
    let mut env: Vec<String> = Vec::new();
    let mut zones: Vec<String> = Vec::new();
    let mut zone_role: HashMap<String, String> = HashMap::new();

    println!("\n  Add your hosts — attacker, targets, and (optionally) a SIEM. Empty name to finish.\n");
    let mut i = 1;
    loop {
        let nm = ask(&format!("  host #{i} name (e.g. attacker, target-a) [done]: "));
        if nm.is_empty() {
            break;
        }
        let role = ask_or("     role — attacker / victim / siem [victim]: ", "victim").to_lowercase();
        let ip = ask("     ip or hostname: ");
        let zone = ask_or("     zone id (e.g. v10 attacker, v20 targets, v40 siem) [v20]: ", "v20");
        if !zones.contains(&zone) {
            zones.push(zone.clone());
        }
        // dominant role per zone (attacker/siem win over victim)
        let slot = zone_role.entry(zone.clone()).or_insert_with(|| role.clone());
        if role != "victim" {
            *slot = role.clone();
        }
        let auth = ask_or("     ssh auth — key / pw [key]: ", "key").to_lowercase();

        let mut h: Map<String, Value> = Map::new();
        h.insert("ip".into(), json!(ip));
        h.insert("role".into(), json!(role));
        h.insert("zone".into(), json!(zone));
        h.insert("auth".into(), json!(auth));
        if auth == "pw" {
            let user = ask_or("     ssh user [root]: ", "root");
            let envn = format!("RANGE_{}_PW", nm.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
            h.insert("user".into(), json!(user));
            h.insert("pw_env".into(), json!(envn));
            let pw = ask("     ssh password (saved to .env, gitignored): ");
            env.push(format!("{envn}={pw}"));
        } else {
            let cfg = ask_or(&format!("     ssh config Host alias [{nm}]: "), &nm);
            h.insert("cfg".into(), json!(cfg));
        }
        if role == "victim" {
            let home = ask_or("     remote home dir [/home/user]: ", "/home/user");
            h.insert("home".into(), json!(home));
        }
        hosts.insert(nm.clone(), Value::Object(h));
        i += 1;
    }
    let inf = ask("\n  AI inference endpoint URL for the AI-plane (blank to skip): ");

    // ── topology: lane by role, firewall attacker → victims allow ──
    let lane_color = |lane: &str| match lane {
        "left" => "atk",
        "top" => "cyan",
        "right" => "purple",
        _ => "gold",
    };
    let v_arr = ["center", "right"];
    let mut v_lanes = v_arr.iter();
    let mut zone_defs: Vec<Value> = Vec::new();
    for z in &zones {
        let r = zone_role.get(z).map(|s| s.as_str()).unwrap_or("victim");
        let lane = if r == "attacker" {
            "left"
        } else if r == "siem" {
            "top"
        } else {
            v_lanes.next().copied().unwrap_or("center")
        };
        let label = match z.strip_prefix('v') {
            Some(n) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => format!("VLAN {n}"),
            _ => z.clone(),
        };
        zone_defs.push(json!({ "id": z, "label": label, "lane": lane, "color": lane_color(lane) }));
    }
    let attacker_zone = zone_role.iter().find(|(_, r)| *r == "attacker").map(|(z, _)| z.clone());
    let mut firewall: Vec<Value> = Vec::new();
    if let Some(az) = &attacker_zone {
        let mut vzs: Vec<String> = zone_role.iter().filter(|(_, r)| *r == "victim").map(|(z, _)| z.clone()).collect();
        vzs.sort();
        vzs.dedup();
        for vz in vzs {
            firewall.push(json!({ "from": az, "to": vz, "rule": "allow" }));
        }
    }
    let siem_host = hosts.iter().find(|(_, h)| h["role"] == "siem").map(|(n, _)| n.clone());

    let mut cfg = json!({
        "name": name, "port": port,
        "transport": { "ssh_config": ssh_cfg, "password_helper": "bin/sshpass.exp", "connect_timeout": 8 },
        "inference": if inf.is_empty() { json!({ "url": "", "model": "" }) } else { json!({ "url": inf, "model": "qwen2.5:7b", "allow_insecure": true }) },
        "hosts": hosts,
        "topology": { "zones": zone_defs, "firewall": firewall },
    });
    if let Some(s) = siem_host {
        cfg["siem"] = json!({ "host": s, "alerts_path": "/var/ossec/logs/alerts/alerts.json", "sudo_pw_env": "RANGE_SIEM_SUDO_PW" });
    }

    std::fs::write("range.config.json", serde_json::to_string_pretty(&cfg).unwrap() + "\n").expect("write range.config.json");
    if !env.is_empty() {
        std::fs::write(".env", env.join("\n") + "\n").expect("write .env");
    }
    println!("\n  ✓ wrote range.config.json{}", if env.is_empty() { "" } else { " + .env" });
    println!("  ✎ tip: edit range.config.json to add \"drop\"/\"pivot\" firewall rules for real segmentation.");
    println!("\n  start it →  purple-range   →  http://localhost:{port}\n");
}
