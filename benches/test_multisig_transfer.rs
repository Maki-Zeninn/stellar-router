use crate::{Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn setup_test_env<'a>(
    env: &'a Env,
) -> (
    PetChainContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let signer1 = Address::generate(env);
    let signer2 = Address::generate(env);
    let new_owner = Address::generate(env);

    client.init_admin(&owner);

    (client, owner, signer1, signer2, new_owner)
}

fn register_test_pet(client: &PetChainContractClient, env: &Env, owner: &Address) -> u64 {
    client.register_pet(
        owner,
        &String::from_str(env, "TestPet"),
        &String::from_str(env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(env, "Labrador"),
        &String::from_str(env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    )
}

#[test]
fn test_configure_multisig() {
    let env = Env::default();
    let (client, owner, signer1, signer2, _) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    let result = client.configure_multisig(&pet_id, &signers, &2);
    assert!(result);

    let config = client.get_multisig_config(&pet_id);
    assert!(config.is_some());

    let config = config.unwrap();
    assert_eq!(config.pet_id, pet_id);
    assert_eq!(config.threshold, 2);
    assert_eq!(config.signers.len(), 3);
    assert!(config.enabled);
}

#[test]
#[should_panic]
fn test_configure_multisig_invalid_threshold_zero() {
    let env = Env::default();
    let (client, owner, signer1, _, _) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());

    client.configure_multisig(&pet_id, &signers, &0);
}

#[test]
#[should_panic]
fn test_configure_multisig_invalid_threshold_exceeds() {
    let env = Env::default();
    let (client, owner, signer1, _, _) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());

    client.configure_multisig(&pet_id, &signers, &3);
}

#[test]
#[should_panic]
fn test_configure_multisig_owner_not_in_signers() {
    let env = Env::default();
    let (client, owner, signer1, signer2, _) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
}

#[test]
fn test_disable_multisig() {
    let env = Env::default();
    let (client, owner, signer1, signer2, _) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);

    let result = client.disable_multisig(&pet_id);
    assert!(result);

    let config = client.get_multisig_config(&pet_id).unwrap();
    assert!(!config.enabled);
}

#[test]
fn test_require_multisig_for_transfer() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);

    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);
    assert_eq!(proposal_id, 1);

    let proposal = client.get_transfer_proposal(&proposal_id);
    assert!(proposal.is_some());

    let proposal = proposal.unwrap();
    assert_eq!(proposal.pet_id, pet_id);
    assert_eq!(proposal.to, new_owner);
    assert_eq!(proposal.signatures.len(), 1);
    assert!(!proposal.executed);
}

#[test]
#[should_panic]
fn test_require_multisig_for_transfer_not_configured() {
    let env = Env::default();
    let (client, owner, _, _, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    client.require_multisig_for_transfer(&pet_id, &new_owner);
}

#[test]
#[should_panic]
fn test_require_multisig_for_transfer_disabled() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    client.disable_multisig(&pet_id);

    client.require_multisig_for_transfer(&pet_id, &new_owner);
}

#[test]
fn test_sign_transfer_proposal() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    let result = client.sign_transfer_proposal(&proposal_id, &signer1);
    assert!(result);

    let proposal = client.get_transfer_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.signatures.len(), 2);
}

#[test]
#[should_panic]
fn test_sign_transfer_proposal_unauthorized() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &signer2);
}

#[test]
#[should_panic]
fn test_sign_transfer_proposal_duplicate() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &owner);
}

#[test]
fn test_multisig_transfer_pet_success() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &signer1);

    let result = client.multisig_transfer_pet(&proposal_id);
    assert!(result);

    let pet_owner = client.get_pet_owner(&pet_id).unwrap();
    assert_eq!(pet_owner, new_owner);

    let proposal = client.get_transfer_proposal(&proposal_id).unwrap();
    assert!(proposal.executed);
}

#[test]
#[should_panic]
fn test_multisig_transfer_pet_threshold_not_met() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &3);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &signer1);

    client.multisig_transfer_pet(&proposal_id);
}

#[test]
#[should_panic]
fn test_multisig_transfer_pet_already_executed() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &signer1);
    client.multisig_transfer_pet(&proposal_id);

    client.multisig_transfer_pet(&proposal_id);
}

#[test]
fn test_multisig_transfer_with_all_signers() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &3);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &signer1);
    client.sign_transfer_proposal(&proposal_id, &signer2);

    let result = client.multisig_transfer_pet(&proposal_id);
    assert!(result);

    let pet_owner = client.get_pet_owner(&pet_id).unwrap();
    assert_eq!(pet_owner, new_owner);
}

#[test]
fn test_multisig_config_per_pet() {
    let env = Env::default();
    let (client, owner, signer1, signer2, _) = setup_test_env(&env);
    let pet_id1 = register_test_pet(&client, &env, &owner);
    let pet_id2 = register_test_pet(&client, &env, &owner);

    let mut signers1 = Vec::new(&env);
    signers1.push_back(owner.clone());
    signers1.push_back(signer1.clone());

    let mut signers2 = Vec::new(&env);
    signers2.push_back(owner.clone());
    signers2.push_back(signer1.clone());
    signers2.push_back(signer2.clone());

    client.configure_multisig(&pet_id1, &signers1, &2);
    client.configure_multisig(&pet_id2, &signers2, &3);

    let config1 = client.get_multisig_config(&pet_id1).unwrap();
    let config2 = client.get_multisig_config(&pet_id2).unwrap();

    assert_eq!(config1.threshold, 2);
    assert_eq!(config1.signers.len(), 2);

    assert_eq!(config2.threshold, 3);
    assert_eq!(config2.signers.len(), 3);
}

#[test]
fn test_ownership_history_after_multisig_transfer() {
    let env = Env::default();
    let (client, owner, signer1, signer2, new_owner) = setup_test_env(&env);
    let pet_id = register_test_pet(&client, &env, &owner);

    let mut signers = Vec::new(&env);
    signers.push_back(owner.clone());
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());

    client.configure_multisig(&pet_id, &signers, &2);
    let proposal_id = client.require_multisig_for_transfer(&pet_id, &new_owner);

    client.sign_transfer_proposal(&proposal_id, &signer1);
    client.multisig_transfer_pet(&proposal_id);

    let history = client.get_ownership_history(&pet_id);
    assert_eq!(history.len(), 2);

    let last_record = history.get(1).unwrap();
    assert_eq!(last_record.previous_owner, owner);
    assert_eq!(last_record.new_owner, new_owner);
}
