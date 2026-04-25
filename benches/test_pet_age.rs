use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

#[test]
fn test_age_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    env.ledger().with_mut(|l| l.timestamp = 2_000_000_000);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "1963280000"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30u32,
        &None,
        &PrivacyLevel::Public,
    );

    let (years, months) = client.get_pet_age(&pet_id);
    assert_eq!((years, months), (1, 2));
}

#[test]
fn test_age_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let (years, months) = client.get_pet_age(&9999);
    assert_eq!((years, months), (0, 0));

    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let owner = Address::generate(&env);
    let future_pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Future Pet"),
        &String::from_str(&env, "1000100"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "Cream"),
        &8u32,
        &None,
        &PrivacyLevel::Public,
    );

    let (future_years, future_months) = client.get_pet_age(&future_pet_id);
    assert_eq!((future_years, future_months), (0, 0));
}

#[test]
fn test_age_calculation_from_iso_date() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    env.ledger().with_mut(|l| l.timestamp = 1_609_459_200);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Milo"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Mixed"),
        &String::from_str(&env, "Black"),
        &18u32,
        &None,
        &PrivacyLevel::Public,
    );

    let (years, months) = client.get_pet_age(&pet_id);
    assert_eq!((years, months), (1, 0));
}

#[test]
#[should_panic]
fn test_register_pet_rejects_invalid_birthday_format() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "01/01/2020"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30u32,
        &None,
        &PrivacyLevel::Public,
    );
}
