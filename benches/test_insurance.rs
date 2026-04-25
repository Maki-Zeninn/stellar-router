use crate::{Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_insurance_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    // Attempting to add insurance for non-existent pet should return false
    let expiry = env.ledger().timestamp() + 31536000;
    let result = client.add_insurance_policy(
        &1,
        &String::from_str(&env, "POL-123"),
        &String::from_str(&env, "PetProtect"),
        &String::from_str(&env, "Comprehensive"),
        &1000,
        &50000,
        &expiry,
    );
    assert_eq!(result, false);

    // Register a pet
    // fn register_pet(
    //     env: Env,
    //     owner: Address,
    //     name: String,
    //     birthday: String,
    //     gender: Gender,
    //     species: Species,
    //     color: String,
    //     breed: String,
    //     weight: u32,
    //     microchip_id: Option<String>,
    //     privacy_level: PrivacyLevel,
    // ) -> u64
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Brown"),
        &String::from_str(&env, "Labrador"),
        &25,
        &None,
        &PrivacyLevel::Public,
    );

    // Add insurance
    let expiry2 = env.ledger().timestamp() + 31536000;
    let result = client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "POL-123"),
        &String::from_str(&env, "PetProtect"),
        &String::from_str(&env, "Comprehensive"),
        &1000,
        &50000,
        &expiry2,
    );
    assert_eq!(result, true);

    // Get insurance
    let policy = client.get_pet_insurance(&pet_id).unwrap();
    assert_eq!(policy.policy_id, String::from_str(&env, "POL-123"));
    assert_eq!(policy.provider, String::from_str(&env, "PetProtect"));
    assert_eq!(
        policy.coverage_type,
        String::from_str(&env, "Comprehensive")
    );
    assert_eq!(policy.premium, 1000);
    assert_eq!(policy.coverage_limit, 50000);
    assert_eq!(policy.active, true);

    // Update insurance status
    let update_result = client.update_insurance_status(&pet_id, &false);
    assert_eq!(update_result, true);

    let updated_policy = client.get_pet_insurance(&pet_id).unwrap();
    assert_eq!(updated_policy.active, false);
}
