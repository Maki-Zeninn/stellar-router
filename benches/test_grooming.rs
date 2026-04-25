use crate::{Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_add_grooming_record() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    let grooming_id = client.add_grooming_record(
        &pet_id,
        &String::from_str(&env, "Full Grooming"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 2592000),
        &5000,
        &String::from_str(&env, "Haircut and bath"),
    );

    assert_eq!(grooming_id, 1);
}

#[test]
fn test_get_grooming_history() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_grooming_record(
        &pet_id,
        &String::from_str(&env, "Full Grooming"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 2592000),
        &5000,
        &String::from_str(&env, "Haircut and bath"),
    );

    client.add_grooming_record(
        &pet_id,
        &String::from_str(&env, "Nail Trim"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 1296000),
        &1500,
        &String::from_str(&env, "Nail trimming only"),
    );

    let history = client.get_grooming_history(&pet_id);
    assert_eq!(history.len(), 2);
}

#[test]
fn test_get_next_grooming_date() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_grooming_record(
        &pet_id,
        &String::from_str(&env, "Full Grooming"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 2592000),
        &5000,
        &String::from_str(&env, "Haircut and bath"),
    );

    let next_date = client.get_next_grooming_date(&pet_id);
    assert!(next_date > 0);
}

#[test]
fn test_get_grooming_expenses() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    client.add_grooming_record(
        &pet_id,
        &String::from_str(&env, "Full Grooming"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 2592000),
        &5000,
        &String::from_str(&env, "Haircut and bath"),
    );

    client.add_grooming_record(
        &pet_id,
        &String::from_str(&env, "Nail Trim"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 1296000),
        &1500,
        &String::from_str(&env, "Nail trimming only"),
    );

    let total_expenses = client.get_grooming_expenses(&pet_id);
    assert_eq!(total_expenses, 6500);
}

#[test]
#[should_panic]
fn test_add_grooming_record_invalid_pet() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init_admin(&admin);

    client.add_grooming_record(
        &999,
        &String::from_str(&env, "Full Grooming"),
        &String::from_str(&env, "Pet Spa"),
        &env.ledger().timestamp(),
        &(env.ledger().timestamp() + 2592000),
        &5000,
        &String::from_str(&env, "Haircut and bath"),
    );
}

#[test]
fn test_empty_grooming_history() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    let history = client.get_grooming_history(&pet_id);
    assert_eq!(history.len(), 0);

    let next_date = client.get_next_grooming_date(&pet_id);
    assert_eq!(next_date, 0);

    let expenses = client.get_grooming_expenses(&pet_id);
    assert_eq!(expenses, 0);
}
