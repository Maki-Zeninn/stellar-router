use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env,
};

#[test]
#[should_panic]

fn test_upgrade_contract_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Try to upgrade without initializing admin - should panic with typed error
    client.upgrade_contract(&new_wasm_hash);
}

#[test]
#[should_panic]

fn test_propose_upgrade_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let proposer = Address::generate(&env);
    let new_wasm_hash = BytesN::from_array(&env, &[2u8; 32]);

    // Try to propose upgrade without initializing admin - should panic with typed error
    client.propose_upgrade(&proposer, &new_wasm_hash);
}

#[test]
#[should_panic]

fn test_approve_upgrade_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    // Try to approve upgrade without initializing admin - should panic with typed error
    client.approve_upgrade(&1u64);
}

#[test]
#[should_panic]

fn test_migrate_version_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    // Try to migrate version without initializing admin - should panic with typed error
    client.migrate_version(&1u32, &0u32, &0u32);
}

#[test]
#[should_panic]

fn test_verify_vet_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vet = Address::generate(&env);

    // Register vet first
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Smith"),
        &String::from_str(&env, "LIC-001"),
        &String::from_str(&env, "Surgery"),
    );

    // Try to verify vet without initializing admin - should panic with typed error
    client.verify_vet(&admin, &vet);
}

#[test]
#[should_panic]

fn test_revoke_vet_license_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vet = Address::generate(&env);

    // Register vet first
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Jones"),
        &String::from_str(&env, "LIC-002"),
        &String::from_str(&env, "Dentistry"),
    );

    // Try to revoke license without initializing admin - should panic with typed error
    client.revoke_vet_license(&admin, &vet);
}

#[test]
#[should_panic]

fn test_propose_action_without_admin_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let proposer = Address::generate(&env);

    // Create a simple proposal action
    let action = ProposalAction::VerifyVet(Address::generate(&env));

    // Try to propose action without initializing admin - should panic with typed error
    client.propose_action(&proposer, &action, &3600u64);
}

#[test]
fn test_admin_methods_work_after_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vet = Address::generate(&env);

    // Initialize admin
    client.init_admin(&admin);

    // Register vet
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Wilson"),
        &String::from_str(&env, "LIC-003"),
        &String::from_str(&env, "General"),
    );

    // Now verifying vet should work
    let result = client.verify_vet(&admin, &vet);
    assert!(result);

    // Vet should be verified
    assert!(client.is_verified_vet(&vet));
}

#[test]
fn test_multisig_admin_methods_work_after_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    // Initialize multisig admin
    client.init_multisig(&admin, &admins, &1u32);

    // Now proposing action should work
    let action = ProposalAction::VerifyVet(Address::generate(&env));

    let proposal_id = client.propose_action(&admin, &action, &3600u64);
    assert_eq!(proposal_id, 1u64);

    // Approving with the other admin should work
    client.approve_proposal(&admin2, &proposal_id);
}
