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
    /// Red's cross-cutting evasion lane — reserved, no tool yet.
    DefenseEvasion,
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
            Category::Detection | Category::DefenseEvasion => None,
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
        ];
        let mut keys: Vec<&str> = all.iter().map(|c| c.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), all.len(), "every category needs a distinct key");
        assert_eq!(Category::CredentialAccess.key(), "credential_access");
    }
}
