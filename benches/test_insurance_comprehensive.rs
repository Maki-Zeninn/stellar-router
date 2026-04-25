use crate::{Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_add_insurance_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2021-05-15"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden"),
        &String::from_str(&env, "Golden Retriever"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    let expiry = env.ledger().timestamp() + 31536000; // 1 year
    let result = client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "INS-2024-001"),
        &String::from_str(&env, "PetGuard Insurance"),
        &String::from_str(&env, "Premium"),
        &2500,
        &100000,
        &expiry,
    );

    assert_eq!(result, true);
}

#[test]
fn test_get_pet_insurance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Luna"),
        &String::from_str(&env, "2022-03-10"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "White"),
        &String::from_str(&env, "Persian"),
        &5,
        &None,
        &PrivacyLevel::Public,
    );

    let expiry = env.ledger().timestamp() + 31536000;
    client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "CAT-INS-789"),
        &String::from_str(&env, "Feline Care Plus"),
        &String::from_str(&env, "Basic"),
        &1500,
        &25000,
        &expiry,
    );

    let policy = client.get_pet_insurance(&pet_id);
    assert!(policy.is_some());

    let policy = policy.unwrap();
    assert_eq!(policy.policy_id, String::from_str(&env, "CAT-INS-789"));
    assert_eq!(policy.provider, String::from_str(&env, "Feline Care Plus"));
    assert_eq!(policy.coverage_type, String::from_str(&env, "Basic"));
    assert_eq!(policy.premium, 1500);
    assert_eq!(policy.coverage_limit, 25000);
    assert_eq!(policy.expiry_date, expiry);
    assert_eq!(policy.active, true);
}

#[test]
fn test_update_insurance_status() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Charlie"),
        &String::from_str(&env, "2020-08-20"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Black"),
        &String::from_str(&env, "Labrador"),
        &28,
        &None,
        &PrivacyLevel::Public,
    );

    let expiry = env.ledger().timestamp() + 31536000;
    client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "DOG-2024-456"),
        &String::from_str(&env, "Canine Coverage"),
        &String::from_str(&env, "Comprehensive"),
        &3000,
        &150000,
        &expiry,
    );

    // Deactivate insurance
    let result = client.update_insurance_status(&pet_id, &false);
    assert_eq!(result, true);

    let policy = client.get_pet_insurance(&pet_id).unwrap();
    assert_eq!(policy.active, false);

    // Reactivate insurance
    let result = client.update_insurance_status(&pet_id, &true);
    assert_eq!(result, true);

    let policy = client.get_pet_insurance(&pet_id).unwrap();
    assert_eq!(policy.active, true);
}

#[test]
fn test_insurance_for_nonexistent_pet() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let expiry = env.ledger().timestamp() + 31536000;
    let result = client.add_insurance_policy(
        &999,
        &String::from_str(&env, "FAKE-001"),
        &String::from_str(&env, "No Provider"),
        &String::from_str(&env, "None"),
        &1000,
        &10000,
        &expiry,
    );

    assert_eq!(result, false);
}

#[test]
fn test_get_insurance_for_pet_without_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Bella"),
        &String::from_str(&env, "2023-01-01"),
        &Gender::Female,
        &Species::Dog,
        &String::from_str(&env, "Brown"),
        &String::from_str(&env, "Beagle"),
        &12,
        &None,
        &PrivacyLevel::Public,
    );

    let policy = client.get_pet_insurance(&pet_id);
    assert!(policy.is_none());
}

#[test]
fn test_update_nonexistent_insurance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Rocky"),
        &String::from_str(&env, "2022-06-15"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Gray"),
        &String::from_str(&env, "Husky"),
        &25,
        &None,
        &PrivacyLevel::Public,
    );

    let result = client.update_insurance_status(&pet_id, &false);
    assert_eq!(result, false);
}

#[test]
fn test_insurance_policy_fields() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Milo"),
        &String::from_str(&env, "2021-11-30"),
        &Gender::Male,
        &Species::Cat,
        &String::from_str(&env, "Orange"),
        &String::from_str(&env, "Tabby"),
        &8,
        &None,
        &PrivacyLevel::Public,
    );

    let start_time = env.ledger().timestamp();
    let expiry = start_time + 31536000;

    client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "POLICY-XYZ-123"),
        &String::from_str(&env, "Pet Health Co"),
        &String::from_str(&env, "Accident & Illness"),
        &1800,
        &75000,
        &expiry,
    );

    let policy = client.get_pet_insurance(&pet_id).unwrap();

    // Verify all fields
    assert_eq!(policy.policy_id, String::from_str(&env, "POLICY-XYZ-123"));
    assert_eq!(policy.provider, String::from_str(&env, "Pet Health Co"));
    assert_eq!(
        policy.coverage_type,
        String::from_str(&env, "Accident & Illness")
    );
    assert_eq!(policy.premium, 1800);
    assert_eq!(policy.coverage_limit, 75000);
    assert_eq!(policy.start_date, start_time);
    assert_eq!(policy.expiry_date, expiry);
    assert_eq!(policy.active, true);
}
