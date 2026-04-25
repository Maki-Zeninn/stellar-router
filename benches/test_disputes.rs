use crate::{
    DisputeStatus, Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_raise_and_get_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let target = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2021-06-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden"),
        &String::from_str(&env, "Golden Retriever"),
        &25,
        &None,
        &PrivacyLevel::Public,
    );

    let claim_amount = 1000;
    let reason = String::from_str(&env, "Pet not delivered");
    let evidence = String::from_str(&env, "ipfs://evidence_hash");

    let dispute_id =
        client.raise_dispute(&pet_id, &owner, &target, &claim_amount, &reason, &evidence);

    let dispute = client.get_dispute(&dispute_id).unwrap();

    assert_eq!(dispute.dispute_id, dispute_id);
    assert_eq!(dispute.pet_id, pet_id);
    assert_eq!(dispute.claimer, owner);
    assert_eq!(dispute.target, target);
    assert_eq!(dispute.amount, claim_amount);
    assert_eq!(dispute.reason, reason);
    assert_eq!(dispute.evidence_hash, evidence);
    assert_eq!(dispute.status, DisputeStatus::Pending);
}

#[test]
fn test_resolve_dispute_admin_only() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init_admin(&admin);

    let owner = Address::generate(&env);
    let target = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Bella"),
        &String::from_str(&env, "2022-01-01"),
        &Gender::Female,
        &Species::Dog,
        &String::from_str(&env, "White"),
        &String::from_str(&env, "Samoyed"),
        &20,
        &None,
        &PrivacyLevel::Public,
    );

    let dispute_id = client.raise_dispute(
        &pet_id,
        &owner,
        &target,
        &500,
        &String::from_str(&env, "Minor issue"),
        &String::from_str(&env, "ipfs://hash"),
    );

    // Resolve as admin
    let success = client.resolve_dispute(&dispute_id, &DisputeStatus::ResolvedInFavorOfClaimer);
    assert!(success);

    let resolved_dispute = client.get_dispute(&dispute_id).unwrap();
    assert_eq!(
        resolved_dispute.status,
        DisputeStatus::ResolvedInFavorOfClaimer
    );
    assert!(resolved_dispute.resolved_at.is_some());
}

#[test]
#[should_panic(expected = "Admin not set")]
fn test_resolve_dispute_no_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let target = Address::generate(&env);

    let pet_id = 1; // Dummy ID for this test

    let dispute_id = client.raise_dispute(
        &pet_id,
        &owner,
        &target,
        &500,
        &String::from_str(&env, "Reason"),
        &String::from_str(&env, "Hash"),
    );

    client.resolve_dispute(&dispute_id, &DisputeStatus::ResolvedInFavorOfClaimer);
}

#[test]
fn test_get_all_pet_disputes() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let target = Address::generate(&env);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Charlie"),
        &String::from_str(&env, "2020-05-15"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Black"),
        &String::from_str(&env, "Labrador"),
        &32,
        &None,
        &PrivacyLevel::Public,
    );

    client.raise_dispute(
        &pet_id,
        &owner,
        &target,
        &100,
        &String::from_str(&env, "Reason 1"),
        &String::from_str(&env, "Hash 1"),
    );

    client.raise_dispute(
        &pet_id,
        &owner,
        &target,
        &200,
        &String::from_str(&env, "Reason 2"),
        &String::from_str(&env, "Hash 2"),
    );

    let disputes = client.get_pet_disputes(&pet_id);
    assert_eq!(disputes.len(), 2);
    assert_eq!(disputes.get(0).unwrap().amount, 100);
    assert_eq!(disputes.get(1).unwrap().amount, 200);
}
