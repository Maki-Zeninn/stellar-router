use crate::{
    Gender, InsuranceClaimStatus, PetChainContract, PetChainContractClient, PrivacyLevel, Species,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_submit_and_get_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Rex"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Brown"),
        &String::from_str(&env, "Mixed"),
        &20,
        &None,
        &PrivacyLevel::Public,
    );

    let expiry = env.ledger().timestamp() + 31536000;
    client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "POL-12345"),
        &String::from_str(&env, "Happy Pets Inc"),
        &String::from_str(&env, "Comprehensive"),
        &100,
        &10000,
        &expiry,
    );

    let claim_amount = 500;
    let claim_desc = String::from_str(&env, "Vet visit for broken leg");

    let claim_id = client
        .submit_insurance_claim(&pet_id, &claim_amount, &claim_desc)
        .unwrap();

    let claim = client.get_insurance_claim(&claim_id).unwrap();

    assert_eq!(claim.claim_id, claim_id);
    assert_eq!(claim.pet_id, pet_id);
    assert_eq!(claim.policy_id, String::from_str(&env, "POL-12345"));
    assert_eq!(claim.amount, claim_amount);
    assert_eq!(claim.status, InsuranceClaimStatus::Pending);
    assert_eq!(claim.description, claim_desc);
}

#[test]
fn test_submit_claim_inactive_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Whiskers"),
        &String::from_str(&env, "2021-02-02"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Black"),
        &String::from_str(&env, "Domestic Shorthair"),
        &4,
        &None,
        &PrivacyLevel::Public,
    );

    let expiry = env.ledger().timestamp() + 31536000;
    client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "POL-INACTIVE"),
        &String::from_str(&env, "Sad Pets Inc"),
        &String::from_str(&env, "Accident Only"),
        &50,
        &5000,
        &expiry,
    );

    client.update_insurance_status(&pet_id, &false);

    let result = client.submit_insurance_claim(
        &pet_id,
        &100,
        &String::from_str(&env, "Attempted claim on inactive policy"),
    );

    assert!(result.is_none());
}

#[test]
fn test_update_claim_status() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2019-05-05"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden"),
        &String::from_str(&env, "Golden Retriever"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    let expiry = env.ledger().timestamp() + 31536000;
    client.add_insurance_policy(
        &pet_id,
        &String::from_str(&env, "POL-987"),
        &String::from_str(&env, "Best Pets"),
        &String::from_str(&env, "Premium"),
        &200,
        &20000,
        &expiry,
    );

    let claim_id = client
        .submit_insurance_claim(
            &pet_id,
            &1500,
            &String::from_str(&env, "Surgery for hip dysplasia"),
        )
        .unwrap();

    let _ = client.update_insurance_claim_status(&claim_id, &InsuranceClaimStatus::Approved);

    let updated_claim = client.get_insurance_claim(&claim_id).unwrap();

    assert_eq!(updated_claim.status, InsuranceClaimStatus::Approved);
}

#[test]
fn test_get_all_pet_claims() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Luna"),
        &String::from_str(&env, "2022-10-10"),
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
        &String::from_str(&env, "POL-CAT-01"),
        &String::from_str(&env, "Feline Care"),
        &String::from_str(&env, "Standard"),
        &80,
        &8000,
        &expiry,
    );

    client.submit_insurance_claim(&pet_id, &200, &String::from_str(&env, "Routine checkup"));

    client.submit_insurance_claim(&pet_id, &150, &String::from_str(&env, "Vaccinations"));

    let claims = client.get_pet_insurance_claims(&pet_id);

    assert_eq!(claims.len(), 2);
    assert_eq!(claims.get(0).unwrap().amount, 200);
    assert_eq!(claims.get(1).unwrap().amount, 150);
}
