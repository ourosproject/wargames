//! Scenarios — the variable environment. Each match rolls one, so the landscape is no longer
//! the same diorama every run. A scenario decides two things a defender actually cares about:
//!
//! * `misconfigs` — which attack paths are actually PLANTED. If Kerberoast isn't in the list,
//!   there's no roastable SPN and red's roast finds nothing — red must adapt, and coverage differs.
//! * `baseline`  — what the shop's default telemetry/EDR already catches without a bespoke rule.
//!   A mature SOC catches more out of the box; a quiet shop is blind until blue hunts + writes rules.
//!
//! Selection is seeded, so a run is reproducible ("re-run scenario seed 40213") — the property a
//! detection-validation tool needs. This is the foundation; topology/firewall traversal builds on it.

use crate::state::Technique::{self, *};

/// The segmentation graph red must traverse from the outside in. Red starts holding `entry`
/// (the internet edge) and must pivot along `edges` until it holds a zone that can reach
/// `objective` (the DC's zone) before any AD attack is even legal. A flat net is a short path;
/// a segmented one forces extra pivots — each a fresh chance for blue to catch east-west movement.
pub struct Topo {
    pub entry: &'static str,
    pub objective: &'static str,
    pub edges: &'static [(&'static str, &'static str)],
}

pub struct Scenario {
    pub name: &'static str,
    pub blurb: &'static str,
    /// Attack paths that exist in this environment (red techniques that can succeed).
    pub misconfigs: &'static [Technique],
    /// Techniques the default telemetry catches with no bespoke rule (EDR/SOC maturity).
    pub baseline: &'static [Technique],
    /// The network red has to cross to reach the DC.
    pub topo: Topo,
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "flat-net · weak service acct",
        blurb: "roastable svc_mssql w/ DCSync, no-preauth user; only replication is loud",
        misconfigs: &[Kerberoast, AsRepRoast, LateralMove],
        baseline: &[LateralMove],
        // Flat: one foothold and the DC is already reachable — no internal firewall.
        topo: Topo { entry: "internet", objective: "vlan30",
            edges: &[("internet", "vlan20"), ("vlan20", "vlan30")] },
    },
    Scenario {
        name: "AES-hardened domain",
        blurb: "RC4 disabled (no roast), but an AS-REP user and a DCSync path remain; recon logged",
        misconfigs: &[AsRepRoast, LateralMove],
        baseline: &[LateralMove, Recon],
        // Segmented: land in the DMZ, pivot through a workstation VLAN to reach the DC.
        topo: Topo { entry: "internet", objective: "vlan30",
            edges: &[("internet", "dmz"), ("dmz", "vlan20"), ("vlan20", "vlan30")] },
    },
    Scenario {
        name: "quiet EDR · blind shop",
        blurb: "every vuln present but default telemetry catches nothing — pure gap surface",
        misconfigs: &[Kerberoast, AsRepRoast, LateralMove],
        baseline: &[],
        // Flat and blind: a short path with no telemetry to catch the crossing.
        topo: Topo { entry: "internet", objective: "vlan30",
            edges: &[("internet", "vlan20"), ("vlan20", "vlan30")] },
    },
    Scenario {
        name: "mature SOC",
        blurb: "roast + DCSync planted, but the SOC watches the perimeter, east-west, roasting, and replication",
        misconfigs: &[Kerberoast, LateralMove],
        baseline: &[Kerberoast, LateralMove, Recon, InitialAccess, Pivot],
        // Segmented and watched: the deepest path (two internal pivots), and every hop is on a
        // wire the SOC reads — giving blue a window to re-segment and box red in before the DC.
        topo: Topo { entry: "internet", objective: "vlan30",
            edges: &[("internet", "dmz"), ("dmz", "vlan10"), ("vlan10", "vlan20"), ("vlan20", "vlan30")] },
    },
];

/// Pick a scenario by seed (reproducible). The seed is hashed first (splitmix64) so wall-clock
/// seeds — which are always multiples of 1000 and would collide under a bare `% n` — spread evenly.
pub fn pick(seed: u64) -> &'static Scenario {
    let mut z = seed.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    &SCENARIOS[(z as usize) % SCENARIOS.len()]
}

/// A fresh seed from the wall clock (the value is surfaced so a run can be reproduced).
/// A new match seed. `WARGAME_SEED` pins it, so a match (scenario + agent RNG streams) can be
/// replayed exactly — needed to A/B two agents over the same set of scenarios.
pub fn fresh_seed() -> u64 {
    if let Some(s) = std::env::var("WARGAME_SEED").ok().and_then(|s| s.parse::<u64>().ok()) {
        return s;
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
}
