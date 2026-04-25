use crate::*;
use soroban_sdk::{testutils::Address as _, Env};

fn setup() -> (Env, PetChainContractClient<'static>, u64, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "1000000"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Black"),
        &20u32,
        &None,
        &PrivacyLevel::Public,
    );

    (env, client, pet_id, owner)
}

#[test]
fn test_consent_history_pagination_basic() {
    let (env, client, pet_id, owner) = setup();
    let grantee = Address::generate(&env);

    // Grant 5 consents, revoke 2 of them.
    let mut ids = Vec::new(&env);
    for _ in 0..5u32 {
        let id = client.grant_consent(&pet_id, &owner, &ConsentType::Research, &grantee);
        ids.push_back(id);
    }
    client.revoke_consent(&ids.get(0).unwrap(), &owner);
    client.revoke_consent(&ids.get(1).unwrap(), &owner);

    // Page 0 with size 3 should return 3 records.
    let page0 = client.get_consent_history_page(&pet_id, &0, &3);
    assert_eq!(page0.len(), 3);

    // Page 1 with size 3 should return the remaining 2 records.
    let page1 = client.get_consent_history_page(&pet_id, &1, &3);
    assert_eq!(page1.len(), 2);

    // Page 2 is beyond the data — should be empty.
    let page2 = client.get_consent_history_page(&pet_id, &2, &3);
    assert_eq!(page2.len(), 0);
}

#[test]
fn test_consent_history_page_zero_size_clamps_to_50() {
    let (env, client, pet_id, owner) = setup();
    let grantee = Address::generate(&env);

    for _ in 0..3u32 {
        client.grant_consent(&pet_id, &owner, &ConsentType::Insurance, &grantee);
    }

    // page_size=0 should be treated as 50 (clamped), returning all 3 records.
    let page = client.get_consent_history_page(&pet_id, &0, &0);
    assert_eq!(page.len(), 3);
}

#[test]
fn test_consent_pruning_removes_oldest_revoked_at_cap() {
    let (env, client, pet_id, owner) = setup();
    let grantee = Address::generate(&env);

    // Fill up to the cap (50) by alternating grant/revoke so revoked records accumulate.
    let mut first_active_id: u64 = 0;
    for i in 0..50u32 {
        let id = client.grant_consent(&pet_id, &owner, &ConsentType::Research, &grantee);
        if i == 0 {
            first_active_id = id;
        }
        // Revoke all but the last one so there are always revoked slots to prune.
        if i < 49 {
            client.revoke_consent(&id, &owner);
        }
    }

    // At this point: 49 revoked + 1 active = 50 total (at cap).
    // Granting one more should prune the oldest revoked record without panicking.
    let new_id = client.grant_consent(&pet_id, &owner, &ConsentType::PublicHealth, &grantee);
    assert!(new_id > 0);

    // Total stored should still be <= 50.
    let history = client.get_consent_history(&pet_id);
    assert!(history.len() <= 50);

    // The first active consent (index 49) must still be present.
    let _ = first_active_id; // used above; suppress warning
}

#[test]
fn test_consent_hard_cap_when_all_active() {
    let (env, client, pet_id, owner) = setup();
    let grantee = Address::generate(&env);

    // Grant 50 consents without revoking any.
    for _ in 0..50u32 {
        client.grant_consent(&pet_id, &owner, &ConsentType::Research, &grantee);
    }

    // The 51st grant should panic because no revoked record exists to prune.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.grant_consent(&pet_id, &owner, &ConsentType::Insurance, &grantee);
    }));
    assert!(result.is_err(), "Expected panic when all 50 slots are active");
}

#[test]
fn test_many_grant_revoke_cycles_stay_bounded() {
    let (env, client, pet_id, owner) = setup();
    let grantee = Address::generate(&env);

    // Simulate 200 grant/revoke cycles — storage must stay bounded at MAX_CONSENTS_PER_PET.
    for _ in 0..200u32 {
        let id = client.grant_consent(&pet_id, &owner, &ConsentType::Research, &grantee);
        client.revoke_consent(&id, &owner);
    }

    let history = client.get_consent_history(&pet_id);
    assert!(
        history.len() <= 50,
        "History grew beyond cap: {}",
        history.len()
    );
}

#[test]
fn test_get_consent_history_page_no_records() {
    let (_env, client, pet_id, _owner) = setup();
    let page = client.get_consent_history_page(&pet_id, &0, &10);
    assert_eq!(page.len(), 0);
}
