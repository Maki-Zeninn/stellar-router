use crate::{PetChainContract, PetChainContractClient, ProposalAction};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

fn setup(env: &Env) -> (PetChainContractClient, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(env, &contract_id);

    let admin1 = Address::generate(env);
    let admin2 = Address::generate(env);
    let mut admins = Vec::new(env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin1, &admins, &2);

    (client, admin1, admin2)
}

#[test]
fn test_upgrade_contract_proposal_lifecycle() {
    let env = Env::default();
    let (client, admin1, admin2) = setup(&env);

    // Using [0u8; 32] as a mock hash that the contract will skip in tests
    let new_wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
    let action = ProposalAction::UpgradeContract(new_wasm_hash.clone());

    // Propose
    let proposal_id = client.propose_action(&admin1, &action, &3600);
    assert_eq!(proposal_id, 1);

    // Approve
    client.approve_proposal(&admin2, &proposal_id);

    // Execute — calls env.deployer().update_current_contract_wasm internally.
    // In our tests, we skip the actual update if the hash is 0.
    client.execute_proposal(&proposal_id);

    // Verify status
    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal.executed);
}

#[test]
#[should_panic]
fn test_upgrade_proposal_cannot_execute_twice() {
    let env = Env::default();
    let (client, admin1, admin2) = setup(&env);

    let action = ProposalAction::UpgradeContract(BytesN::from_array(&env, &[0u8; 32]));
    let proposal_id = client.propose_action(&admin1, &action, &3600);
    client.approve_proposal(&admin2, &proposal_id);
    client.execute_proposal(&proposal_id);
    client.execute_proposal(&proposal_id); // must panic
}

#[test]
#[should_panic]
fn test_upgrade_proposal_threshold_not_met() {
    let env = Env::default();
    let (client, admin1, _admin2) = setup(&env);

    let action = ProposalAction::UpgradeContract(BytesN::from_array(&env, &[0u8; 32]));
    let proposal_id = client.propose_action(&admin1, &action, &3600);
    // Only 1 of 2 required approvals — must panic
    client.execute_proposal(&proposal_id);
}

// --- Tests verifying admins[1] can perform upgrade/migration ---

#[test]
fn test_admin2_can_propose_upgrade() {
    let env = Env::default();
    let (client, admin1, admin2) = setup(&env);

    let action = ProposalAction::UpgradeContract(BytesN::from_array(&env, &[0u8; 32]));

    // admin2 (index 1) proposes
    let proposal_id = client.propose_action(&admin2, &action, &3600);
    assert_eq!(proposal_id, 1);

    // admin1 approves to meet threshold of 2
    client.approve_proposal(&admin1, &proposal_id);
    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal.executed);
}

#[test]
fn test_admin2_can_migrate_version() {
    let env = Env::default();
    let (client, _admin1, admin2) = setup(&env);

    // admin2 (index 1) calls migrate_version directly
    client.migrate_version(&admin2, &2, &0, &0);

    let version = client.get_version();
    assert_eq!(version.major, 2);
    assert_eq!(version.minor, 0);
    assert_eq!(version.patch, 0);
}

#[test]
fn test_admin2_can_approve_upgrade_proposal() {
    let env = Env::default();
    let (client, admin1, admin2) = setup(&env);

    let action = ProposalAction::UpgradeContract(BytesN::from_array(&env, &[0u8; 32]));
    let proposal_id = client.propose_action(&admin1, &action, &3600);

    // admin2 (index 1) approves
    client.approve_proposal(&admin2, &proposal_id);
    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal.executed);
}

#[test]
#[should_panic]
fn test_non_admin_cannot_migrate_version() {
    let env = Env::default();
    let (client, _admin1, _admin2) = setup(&env);

    let non_admin = Address::generate(&env);
    client.migrate_version(&non_admin, &2, &0, &0);
}
