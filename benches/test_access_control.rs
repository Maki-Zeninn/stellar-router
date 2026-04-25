use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env, Symbol, Vec,
};

#[test]
fn test_remove_pet_from_owner_index_missing_last_entry_does_not_panic() {
    // Simulates index inconsistency: PetCountByOwner says 2 but the last
    // index slot (index 2) is absent. remove_pet_from_owner_index must
    // return early instead of panicking.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);

    // Register two pets so the owner index has two entries.
    let pet1 = client.register_pet(
        &owner,
        &String::from_str(&env, "Alpha"),
        &String::from_str(&env, "1000000"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Black"),
        &20u32,
        &None,
        &PrivacyLevel::Public,
    );
    let _pet2 = client.register_pet(
        &owner,
        &String::from_str(&env, "Beta"),
        &String::from_str(&env, "1000000"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "White"),
        &5u32,
        &None,
        &PrivacyLevel::Public,
    );

    // Corrupt the index: remove the last slot entry (index 2) directly from
    // storage so the count says 2 but slot 2 is missing.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .remove(&DataKey::OwnerPetIndex((owner.clone(), 2u64)));
    });

    // Initiate a transfer of pet1 — this calls remove_pet_from_owner_index
    // internally. With the fix it must complete without panicking.
    client.transfer_pet_ownership(&pet1, &new_owner);
    client.accept_pet_transfer(&pet1);

    // pet1 now belongs to new_owner; the call did not panic.
    assert_eq!(client.get_pet_owner(&pet1), Some(new_owner));
}

#[test]
fn test_grant_access() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &25u32,
        &None,
        &PrivacyLevel::Public,
    );

    let result = client.grant_access(&pet_id, &grantee, &AccessLevel::Basic, &None);
    assert!(result);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::Basic);
}

#[test]
fn test_grant_access_with_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2021-05-15"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Yellow"),
        &30u32,
        &None,
        &PrivacyLevel::Private,
    );

    let now = env.ledger().timestamp();
    let expires_at = now + 3600; // Expires in 1 hour

    let result = client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &Some(expires_at));
    assert!(result);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::Full);
}

#[test]
fn test_revoke_access() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Luna"),
        &String::from_str(&env, "2022-03-20"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "Cream"),
        &8u32,
        &None,
        &PrivacyLevel::Restricted,
    );

    client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);
    assert_eq!(client.check_access(&pet_id, &grantee), AccessLevel::Full);

    let result = client.revoke_access(&pet_id, &grantee);
    assert!(result);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::None);
}

#[test]
fn test_access_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Rex"),
        &String::from_str(&env, "2019-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Boxer"),
        &String::from_str(&env, "Brindle"),
        &28u32,
        &None,
        &PrivacyLevel::Private,
    );

    let now = 1000;
    env.ledger().with_mut(|l| l.timestamp = now);

    let expires_at = now + 100;
    client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &Some(expires_at));

    assert_eq!(client.check_access(&pet_id, &grantee), AccessLevel::Full);

    env.ledger().with_mut(|l| l.timestamp = expires_at + 1);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::None);
}

#[test]
fn test_access_level_enforcement_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Whiskers"),
        &String::from_str(&env, "2020-06-10"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Persian"),
        &String::from_str(&env, "White"),
        &5u32,
        &None,
        &PrivacyLevel::Restricted,
    );

    client.grant_access(&pet_id, &grantee, &AccessLevel::Basic, &None);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::Basic);
}

#[test]
fn test_access_level_enforcement_full() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Charlie"),
        &String::from_str(&env, "2018-11-22"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Beagle"),
        &String::from_str(&env, "Tricolor"),
        &12u32,
        &None,
        &PrivacyLevel::Private,
    );

    client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::Full);
}

#[test]
fn test_owner_has_full_access() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Bella"),
        &String::from_str(&env, "2021-02-14"),
        &Gender::Female,
        &Species::Dog,
        &String::from_str(&env, "Poodle"),
        &String::from_str(&env, "Black"),
        &10u32,
        &None,
        &PrivacyLevel::Private,
    );

    let access_level = client.check_access(&pet_id, &owner);
    assert_eq!(access_level, AccessLevel::Full);
}

#[test]
fn test_get_authorized_users() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee1 = Address::generate(&env);
    let grantee2 = Address::generate(&env);
    let grantee3 = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Rocky"),
        &String::from_str(&env, "2020-08-05"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Bulldog"),
        &String::from_str(&env, "Brown"),
        &22u32,
        &None,
        &PrivacyLevel::Restricted,
    );

    client.grant_access(&pet_id, &grantee1, &AccessLevel::Basic, &None);
    client.grant_access(&pet_id, &grantee2, &AccessLevel::Full, &None);
    client.grant_access(&pet_id, &grantee3, &AccessLevel::Basic, &None);

    let authorized_users = client.get_authorized_users(&pet_id);
    assert_eq!(authorized_users.len(), 3);
    assert!(authorized_users.contains(grantee1));
    assert!(authorized_users.contains(grantee2));
    assert!(authorized_users.contains(grantee3));
}

#[test]
fn test_get_authorized_users_excludes_revoked() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee1 = Address::generate(&env);
    let grantee2 = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Daisy"),
        &String::from_str(&env, "2019-12-25"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Maine Coon"),
        &String::from_str(&env, "Gray"),
        &7u32,
        &None,
        &PrivacyLevel::Private,
    );

    client.grant_access(&pet_id, &grantee1, &AccessLevel::Full, &None);
    client.grant_access(&pet_id, &grantee2, &AccessLevel::Basic, &None);

    client.revoke_access(&pet_id, &grantee1);

    let authorized_users = client.get_authorized_users(&pet_id);
    assert_eq!(authorized_users.len(), 1);
    assert!(!authorized_users.contains(grantee1));
    assert!(authorized_users.contains(grantee2));
}

#[test]
fn test_get_authorized_users_excludes_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee1 = Address::generate(&env);
    let grantee2 = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Milo"),
        &String::from_str(&env, "2020-04-18"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Corgi"),
        &String::from_str(&env, "Orange"),
        &11u32,
        &None,
        &PrivacyLevel::Restricted,
    );

    let now = 1000;
    env.ledger().with_mut(|l| l.timestamp = now);

    let expires_at = now + 100;
    client.grant_access(&pet_id, &grantee1, &AccessLevel::Full, &Some(expires_at));
    client.grant_access(&pet_id, &grantee2, &AccessLevel::Basic, &None);

    env.ledger().with_mut(|l| l.timestamp = expires_at + 1);

    let authorized_users = client.get_authorized_users(&pet_id);
    assert_eq!(authorized_users.len(), 1);
    assert!(!authorized_users.contains(grantee1));
    assert!(authorized_users.contains(grantee2));
}

#[test]
fn test_get_pets_by_owner_single_owner_returns_only_owned_pets() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let other_owner = Address::generate(&env);

    client.register_pet(
        &owner,
        &String::from_str(&env, "Alpha"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Black"),
        &20u32,
        &None,
        &PrivacyLevel::Public,
    );
    client.register_pet(
        &other_owner,
        &String::from_str(&env, "Other"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "White"),
        &5u32,
        &None,
        &PrivacyLevel::Public,
    );

    let pets = client.get_pets_by_owner(&owner, &0u64, &10u32);
    assert_eq!(pets.len(), 1);
    assert_eq!(pets.get(0).unwrap().name, String::from_str(&env, "Alpha"));
}

#[test]
fn test_get_pets_by_owner_multiple_pets_returns_in_index_order() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    client.register_pet(
        &owner,
        &String::from_str(&env, "Alpha"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Black"),
        &20u32,
        &None,
        &PrivacyLevel::Public,
    );
    client.register_pet(
        &owner,
        &String::from_str(&env, "Beta"),
        &String::from_str(&env, "2020-02-01"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "White"),
        &5u32,
        &None,
        &PrivacyLevel::Public,
    );
    client.register_pet(
        &owner,
        &String::from_str(&env, "Gamma"),
        &String::from_str(&env, "2020-03-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Beagle"),
        &String::from_str(&env, "Brown"),
        &12u32,
        &None,
        &PrivacyLevel::Public,
    );

    let pets = client.get_pets_by_owner(&owner, &0u64, &10u32);
    assert_eq!(pets.len(), 3);
    assert_eq!(pets.get(0).unwrap().name, String::from_str(&env, "Alpha"));
    assert_eq!(pets.get(1).unwrap().name, String::from_str(&env, "Beta"));
    assert_eq!(pets.get(2).unwrap().name, String::from_str(&env, "Gamma"));
}

#[test]
fn test_get_pets_by_owner_supports_pagination() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    client.register_pet(
        &owner,
        &String::from_str(&env, "Alpha"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Black"),
        &20u32,
        &None,
        &PrivacyLevel::Public,
    );
    client.register_pet(
        &owner,
        &String::from_str(&env, "Beta"),
        &String::from_str(&env, "2020-02-01"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "White"),
        &5u32,
        &None,
        &PrivacyLevel::Public,
    );
    client.register_pet(
        &owner,
        &String::from_str(&env, "Gamma"),
        &String::from_str(&env, "2020-03-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Beagle"),
        &String::from_str(&env, "Brown"),
        &12u32,
        &None,
        &PrivacyLevel::Public,
    );

    let page = client.get_pets_by_owner(&owner, &1u64, &2u32);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap().name, String::from_str(&env, "Beta"));
    assert_eq!(page.get(1).unwrap().name, String::from_str(&env, "Gamma"));

    assert_eq!(client.get_pets_by_owner(&owner, &5u64, &2u32).len(), 0);
    assert_eq!(client.get_pets_by_owner(&owner, &0u64, &0u32).len(), 0);
}

#[test]
fn test_get_access_grant() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Shadow"),
        &String::from_str(&env, "2021-07-30"),
        &Gender::Male,
        &Species::Cat,
        &String::from_str(&env, "Black Cat"),
        &String::from_str(&env, "Black"),
        &6u32,
        &None,
        &PrivacyLevel::Private,
    );

    let now = env.ledger().timestamp();
    let expires_at = now + 7200;

    client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &Some(expires_at));

    let grant = client.get_access_grant(&pet_id, &grantee).unwrap();
    assert_eq!(grant.pet_id, pet_id);
    assert_eq!(grant.granter, owner);
    assert_eq!(grant.grantee, grantee);
    assert_eq!(grant.access_level, AccessLevel::Full);
    assert_eq!(grant.expires_at, Some(expires_at));
    assert!(grant.is_active);
}

#[test]
fn test_multiple_access_levels() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let vet = Address::generate(&env);
    let family_member = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Cooper"),
        &String::from_str(&env, "2020-09-12"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "German Shepherd"),
        &String::from_str(&env, "Black and Tan"),
        &35u32,
        &None,
        &PrivacyLevel::Private,
    );

    client.grant_access(&pet_id, &vet, &AccessLevel::Full, &None);
    client.grant_access(&pet_id, &family_member, &AccessLevel::Basic, &None);

    assert_eq!(client.check_access(&pet_id, &vet), AccessLevel::Full);
    assert_eq!(
        client.check_access(&pet_id, &family_member),
        AccessLevel::Basic
    );
    assert_eq!(client.check_access(&pet_id, &owner), AccessLevel::Full);
}

#[test]
fn test_no_access_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Zoe"),
        &String::from_str(&env, "2022-01-10"),
        &Gender::Female,
        &Species::Dog,
        &String::from_str(&env, "Husky"),
        &String::from_str(&env, "Gray and White"),
        &24u32,
        &None,
        &PrivacyLevel::Private,
    );

    let access_level = client.check_access(&pet_id, &stranger);
    assert_eq!(access_level, AccessLevel::None);
}

#[test]
fn test_permanent_access() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let grantee = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Oscar"),
        &String::from_str(&env, "2019-05-20"),
        &Gender::Male,
        &Species::Cat,
        &String::from_str(&env, "Tabby"),
        &String::from_str(&env, "Orange"),
        &9u32,
        &None,
        &PrivacyLevel::Restricted,
    );

    client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);

    let grant = client.get_access_grant(&pet_id, &grantee).unwrap();
    assert!(grant.expires_at.is_none());

    let now = env.ledger().timestamp();
    env.ledger().with_mut(|l| l.timestamp = now + 1_000_000);

    let access_level = client.check_access(&pet_id, &grantee);
    assert_eq!(access_level, AccessLevel::Full);
}

#[test]
fn test_access_logs_are_capped() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Bounded"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Retriever"),
        &String::from_str(&env, "Gold"),
        &20u32,
        &None,
        &PrivacyLevel::Private,
    );

    let log_owner = Address::generate(&env);
    let mut logs = Vec::new(&env);
    for id in 0..MAX_LOG_ENTRIES {
        logs.push_back(AccessLog {
            id: id as u64,
            pet_id,
            user: log_owner.clone(),
            action: AccessAction::Read,
            timestamp: id as u64,
            details: String::from_str(&env, "seed"),
        });
    }
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&(Symbol::new(&env, "access_logs"), pet_id), &logs);
    });

    let grantee = Address::generate(&env);
    client.grant_access(&pet_id, &grantee, &AccessLevel::Basic, &None);

    let logs = client.get_access_logs(&pet_id);
    assert_eq!(logs.len(), MAX_LOG_ENTRIES);
}

#[test]
fn test_access_logs_retain_newest_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Recent"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Shorthair"),
        &String::from_str(&env, "Gray"),
        &6u32,
        &None,
        &PrivacyLevel::Restricted,
    );

    let log_owner = Address::generate(&env);
    let mut logs = Vec::new(&env);
    for id in 0..MAX_LOG_ENTRIES {
        logs.push_back(AccessLog {
            id: id as u64,
            pet_id,
            user: log_owner.clone(),
            action: AccessAction::Read,
            timestamp: id as u64,
            details: String::from_str(&env, "seed"),
        });
    }
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&(Symbol::new(&env, "access_logs"), pet_id), &logs);
    });

    let grantee = Address::generate(&env);
    client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);

    let logs = client.get_access_logs(&pet_id);
    assert_eq!(logs.get(0).unwrap().id, 1);
    assert_eq!(logs.get(logs.len() - 1).unwrap().id, MAX_LOG_ENTRIES as u64);
    assert_eq!(logs.get(0).unwrap().action, AccessAction::Read);
    assert_eq!(
        logs.get(logs.len() - 1).unwrap().action,
        AccessAction::Grant
    );
}

#[test]
fn test_get_vaccination_history_pagination_first_page() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Brown"),
        &25u32,
        &None,
        &PrivacyLevel::Public,
    );

    // Set up a verified vet
    let admin = Address::generate(&env);
    let vet = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    client.init_multisig(&admin, &admins, &1u32);
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Test"),
        &String::from_str(&env, "LIC-001"),
        &String::from_str(&env, "General"),
    );
    client.verify_vet(&admin, &vet);

    // Add multiple vaccinations
    client.add_vaccination(
        &pet_id,
        &vet,
        &crate::VaccineType::Rabies,
        &String::from_str(&env, "RabiesVax"),
        &1000u64,
        &2000u64,
        &String::from_str(&env, "BATCH-001"),
    );
    client.add_vaccination(
        &pet_id,
        &vet,
        &crate::VaccineType::Parvovirus,
        &String::from_str(&env, "ParvoVax"),
        &1000u64,
        &2000u64,
        &String::from_str(&env, "BATCH-002"),
    );
    client.add_vaccination(
        &pet_id,
        &vet,
        &crate::VaccineType::Bordetella,
        &String::from_str(&env, "BordetellaVax"),
        &1000u64,
        &2000u64,
        &String::from_str(&env, "BATCH-003"),
    );

    // Test first page with limit 2
    let history = client.get_vaccination_history(&pet_id, &0u64, &2u32);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().vaccine_type, crate::VaccineType::Rabies);
    assert_eq!(history.get(1).unwrap().vaccine_type, crate::VaccineType::Parvovirus);
}

#[test]
fn test_get_vaccination_history_pagination_out_of_bounds_offset() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Brown"),
        &25u32,
        &None,
        &PrivacyLevel::Public,
    );

    // Set up a verified vet
    let admin = Address::generate(&env);
    let vet = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    client.init_multisig(&admin, &admins, &1u32);
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Test"),
        &String::from_str(&env, "LIC-001"),
        &String::from_str(&env, "General"),
    );
    client.verify_vet(&admin, &vet);

    // Add one vaccination
    client.add_vaccination(
        &pet_id,
        &vet,
        &crate::VaccineType::Rabies,
        &String::from_str(&env, "RabiesVax"),
        &1000u64,
        &2000u64,
        &String::from_str(&env, "BATCH-001"),
    );

    // Test out-of-bounds offset
    let history = client.get_vaccination_history(&pet_id, &10u64, &5u32);
    assert_eq!(history.len(), 0);
}

#[test]
fn test_get_vaccination_history_pagination_limit_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Brown"),
        &25u32,
        &None,
        &PrivacyLevel::Public,
    );

    // Set up a verified vet
    let admin = Address::generate(&env);
    let vet = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    client.init_multisig(&admin, &admins, &1u32);
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Test"),
        &String::from_str(&env, "LIC-001"),
        &String::from_str(&env, "General"),
    );
    client.verify_vet(&admin, &vet);

    // Add vaccinations
    client.add_vaccination(
        &pet_id,
        &vet,
        &crate::VaccineType::Rabies,
        &String::from_str(&env, "RabiesVax"),
        &1000u64,
        &2000u64,
        &String::from_str(&env, "BATCH-001"),
    );

    // Test limit of 0
    let history = client.get_vaccination_history(&pet_id, &0u64, &0u32);
    assert_eq!(history.len(), 0);
}
