// wargame/tests/taxonomy.rs
use purple_wargame::arsenal::default_registry;
use purple_wargame::category::Category;
use std::collections::BTreeMap;

#[test]
fn every_enforced_category_holds_at_least_one_tool() {
    let reg = default_registry();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for spec in reg.all_specs() {
        *counts.entry(spec.category.key()).or_default() += 1;
    }
    for cat in Category::ENFORCED {
        assert!(counts.get(cat.key()).copied().unwrap_or(0) >= 1,
            "enforced category {} has no tool", cat.key());
    }
}

#[test]
fn each_card_has_the_expected_category() {
    let reg = default_registry();
    let expect: &[(&str, Category)] = &[
        ("initial_access", Category::InitialAccess),
        ("pivot", Category::LateralMovement),
        ("recon", Category::Discovery),
        ("kerberoast", Category::CredentialAccess),
        ("asrep_roast", Category::CredentialAccess),
        ("bloodhound", Category::Discovery),
        ("escalate_da", Category::PrivilegeEscalation),
        ("monitor", Category::Detection),
        ("active_response", Category::Detection),
        ("hunt", Category::Detection),
        ("deploy_detection", Category::Detection),
        ("remediate_acl", Category::Harden),
        ("enforce_aes", Category::Harden),
        ("enforce_preauth", Category::Harden),
        ("rotate_creds", Category::Evict),
        ("segment", Category::Isolate),
    ];
    for (id, cat) in expect {
        let spec = reg.get(id).unwrap().spec();
        assert_eq!(spec.category, *cat, "card {id} has wrong category");
    }
}

#[test]
fn data_arsenal_loads_and_passes_all_checks() {
    // default_registry panics if any file fails per-move or set validation.
    let reg = purple_wargame::arsenal::default_registry();
    assert_eq!(reg.len(), 25, "all 25 moves load (16 base + 9 primitives-expansion)");
}

#[test]
fn every_enforced_category_has_a_move() {
    use purple_wargame::category::Category;
    let defs: Vec<_> = purple_wargame::arsenal::TOOL_FILES.iter()
        .map(|s| purple_wargame::arsenal::parse_tool(s).unwrap())
        .collect();
    for cat in Category::ENFORCED {
        assert!(defs.iter().any(|d| d.category == cat), "no move in category {}", cat.key());
    }
}

#[test]
fn every_move_passes_per_move_validation() {
    for src in purple_wargame::arsenal::TOOL_FILES {
        let def = purple_wargame::arsenal::parse_tool(src).unwrap();
        purple_wargame::arsenal::validate(&def).unwrap_or_else(|e| panic!("validation failed: {e:?}"));
    }
}
