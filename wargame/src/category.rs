//! Tier-1 taxonomy: the ATT&CK-tactic kill chain. A `Category` is the *stage* a tool
//! belongs to (Red) or the attacker stage it counters / the cross-cutting Detection lane
//! (Blue). This is the fixed topology; tools compose over it. Facts key on category
//! progress, never on a specific tool — that is what keeps the arsenal open-ended.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    InitialAccess,
    Discovery,
    CredentialAccess,
    PrivilegeEscalation,
    LateralMovement,
    Exfiltration,
    /// Blue's cross-cutting monitoring/hunting lane — not a single attack stage.
    Detection,
    /// Red's cross-cutting evasion lane — reserved, no tool yet. (ATT&CK TA0005 — an attack
    /// tactic, NOT a defensive lane; see `is_defensive`.)
    DefenseEvasion,

    // ── remaining ATT&CK tactics ──
    Reconnaissance,
    ResourceDevelopment,
    Execution,
    Persistence,
    Collection,
    CommandAndControl,
    Impact,

    // ── D3FEND defensive lanes (cross-cutting, no chain order, no tactic id) ──
    Harden,
    Isolate,
    Evict,
    Deceive,
    Model,
}

impl Category {
    /// Categories that must each hold at least one tool in the current arsenal.
    pub const ENFORCED: [Category; 6] = [
        Category::InitialAccess,
        Category::Discovery,
        Category::CredentialAccess,
        Category::PrivilegeEscalation,
        Category::LateralMovement,
        Category::Detection,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            Category::InitialAccess => "initial_access",
            Category::Discovery => "discovery",
            Category::CredentialAccess => "credential_access",
            Category::PrivilegeEscalation => "privilege_escalation",
            Category::LateralMovement => "lateral_movement",
            Category::Exfiltration => "exfiltration",
            Category::Detection => "detection",
            Category::DefenseEvasion => "defense_evasion",
            Category::Reconnaissance => "reconnaissance",
            Category::ResourceDevelopment => "resource_development",
            Category::Execution => "execution",
            Category::Persistence => "persistence",
            Category::Collection => "collection",
            Category::CommandAndControl => "command_and_control",
            Category::Impact => "impact",
            Category::Harden => "harden",
            Category::Isolate => "isolate",
            Category::Evict => "evict",
            Category::Deceive => "deceive",
            Category::Model => "model",
        }
    }

    /// Position in the linear kill chain; `None` for cross-cutting lanes.
    pub fn chain_order(&self) -> Option<u8> {
        match self {
            Category::InitialAccess => Some(0),
            Category::Discovery => Some(1),
            Category::CredentialAccess => Some(2),
            Category::PrivilegeEscalation => Some(3),
            Category::LateralMovement => Some(4),
            Category::Exfiltration => Some(5),
            _ => None,
        }
    }

    /// True for D3FEND defensive lanes (including the pre-existing `Detection`). All ATT&CK
    /// tactics — including `DefenseEvasion`, which is red's evasion tactic (TA0005) — are false.
    pub fn is_defensive(&self) -> bool {
        matches!(self, Category::Detection | Category::Harden | Category::Isolate
            | Category::Evict | Category::Deceive | Category::Model)
    }

    /// MITRE ATT&CK tactic id; empty string for D3FEND defensive lanes (no ATT&CK id applies).
    pub fn tactic_id(&self) -> &'static str {
        match self {
            Category::Reconnaissance => "TA0043", Category::ResourceDevelopment => "TA0042",
            Category::InitialAccess => "TA0001", Category::Execution => "TA0002",
            Category::Persistence => "TA0003", Category::PrivilegeEscalation => "TA0004",
            Category::DefenseEvasion => "TA0005", Category::CredentialAccess => "TA0006",
            Category::Discovery => "TA0007", Category::LateralMovement => "TA0008",
            Category::Collection => "TA0009", Category::CommandAndControl => "TA0011",
            Category::Exfiltration => "TA0010", Category::Impact => "TA0040",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_chain_is_strictly_ordered() {
        let chain = [
            Category::InitialAccess, Category::Discovery, Category::CredentialAccess,
            Category::PrivilegeEscalation, Category::LateralMovement, Category::Exfiltration,
        ];
        let orders: Vec<u8> = chain.iter().map(|c| c.chain_order().unwrap()).collect();
        assert!(orders.windows(2).all(|w| w[0] < w[1]), "kill chain must be strictly increasing");
    }

    #[test]
    fn cross_cutting_categories_have_no_chain_order() {
        assert_eq!(Category::Detection.chain_order(), None);
        assert_eq!(Category::DefenseEvasion.chain_order(), None);
    }

    #[test]
    fn keys_are_unique_and_stable() {
        let all = [
            Category::InitialAccess, Category::Discovery, Category::CredentialAccess,
            Category::PrivilegeEscalation, Category::LateralMovement, Category::Exfiltration,
            Category::Detection, Category::DefenseEvasion,
            Category::Reconnaissance, Category::ResourceDevelopment, Category::Execution,
            Category::Persistence, Category::Collection, Category::CommandAndControl,
            Category::Impact, Category::Harden, Category::Isolate, Category::Evict,
            Category::Deceive, Category::Model,
        ];
        let mut keys: Vec<&str> = all.iter().map(|c| c.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), all.len(), "every category needs a distinct key");
        assert_eq!(Category::CredentialAccess.key(), "credential_access");
    }

    #[test]
    fn defensive_lanes_have_no_attack_id_or_chain_order() {
        assert_eq!(Category::Harden.tactic_id(), "");
        assert_eq!(Category::Harden.chain_order(), None);
        assert!(Category::Detection.is_defensive());
        assert!(!Category::DefenseEvasion.is_defensive());
    }
}
