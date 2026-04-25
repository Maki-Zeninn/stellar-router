use crate::{
    ActivityType, Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_add_activity_record() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    let activity_id = client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &5,
        &2000,
        &String::from_str(&env, "Morning walk in the park"),
    );

    assert_eq!(activity_id, 1);
}

#[test]
fn test_get_activity_history() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &5,
        &2000,
        &String::from_str(&env, "Morning walk"),
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Run,
        &15,
        &8,
        &1500,
        &String::from_str(&env, "Evening run"),
    );

    let history = client.get_activity_history(&pet_id);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().activity_type, ActivityType::Walk);
    assert_eq!(history.get(1).unwrap().activity_type, ActivityType::Run);
}

#[test]
fn test_activity_stats() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &5,
        &2000,
        &String::from_str(&env, "Walk 1"),
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Run,
        &20,
        &8,
        &1500,
        &String::from_str(&env, "Run 1"),
    );

    let (total_duration, total_distance) = client.get_activity_stats(&pet_id, &7);
    assert_eq!(total_duration, 50);
    assert_eq!(total_distance, 3500);
}

#[test]
fn test_multiple_activity_types() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &5,
        &2000,
        &String::from_str(&env, "Walk"),
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Run,
        &15,
        &8,
        &1500,
        &String::from_str(&env, "Run"),
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Play,
        &45,
        &6,
        &0,
        &String::from_str(&env, "Play time"),
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Training,
        &20,
        &4,
        &0,
        &String::from_str(&env, "Training session"),
    );

    let history = client.get_activity_history(&pet_id);
    assert_eq!(history.len(), 4);
}

#[test]
#[should_panic]
fn test_add_activity_nonexistent_pet() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    client.add_activity_record(
        &999,
        &ActivityType::Walk,
        &30,
        &5,
        &2000,
        &String::from_str(&env, "Walk"),
    );
}

#[test]
#[should_panic]
fn test_invalid_intensity() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &15,
        &2000,
        &String::from_str(&env, "Walk"),
    );
}

#[test]
fn test_activity_stats_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init_admin(&owner);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Max"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    let (total_duration, total_distance) = client.get_activity_stats(&pet_id, &7);
    assert_eq!(total_duration, 0);
    assert_eq!(total_distance, 0);
}
