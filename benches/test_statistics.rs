use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_env() -> (Env, PetChainContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);
    (env, client)
}

fn register_pet_with_species(
    client: &PetChainContractClient,
    env: &Env,
    owner: &Address,
    species: Species,
) -> u64 {
    client.register_pet(
        owner,
        &String::from_str(env, "Pet"),
        &String::from_str(env, "2020-01-01"),
        &Gender::Male,
        &species,
        &String::from_str(env, "Breed"),
        &String::from_str(env, "Color"),
        &10u32,
        &None,
        &PrivacyLevel::Public,
    )
}

#[test]
fn test_get_total_pets() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    assert_eq!(client.get_total_pets(), 0);

    register_pet_with_species(&client, &env, &owner, Species::Dog);
    assert_eq!(client.get_total_pets(), 1);

    register_pet_with_species(&client, &env, &owner, Species::Cat);
    assert_eq!(client.get_total_pets(), 2);
}

#[test]
fn test_get_species_count() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    register_pet_with_species(&client, &env, &owner, Species::Dog);
    register_pet_with_species(&client, &env, &owner, Species::Dog);
    register_pet_with_species(&client, &env, &owner, Species::Cat);

    assert_eq!(client.get_species_count(&String::from_str(&env, "Dog")), 2);
    assert_eq!(client.get_species_count(&String::from_str(&env, "Cat")), 1);
    assert_eq!(client.get_species_count(&String::from_str(&env, "Bird")), 0);
}

#[test]
fn test_get_active_pets_count() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let id1 = register_pet_with_species(&client, &env, &owner, Species::Dog);
    let id2 = register_pet_with_species(&client, &env, &owner, Species::Cat);

    assert_eq!(client.get_active_pets_count(), 0);

    client.activate_pet(&id1);
    assert_eq!(client.get_active_pets_count(), 1);

    client.activate_pet(&id2);
    assert_eq!(client.get_active_pets_count(), 2);

    client.deactivate_pet(&id1);
    assert_eq!(client.get_active_pets_count(), 1);

    // Activating an already-active pet must not double-count.
    client.activate_pet(&id2);
    assert_eq!(client.get_active_pets_count(), 1);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_activate_pet_requires_owner_auth() {
    let env = Env::default();
    env.mock_all_auths(); // Mock auth for registration
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let _non_owner = Address::generate(&env);

    let pet_id = register_pet_with_species(&client, &env, &owner, Species::Dog);

    // Clear auth mocking so real auth checks apply
    env.set_auths(&[]);

    // Attempting to activate pet should panic due to missing auth
    client.activate_pet(&pet_id);
}
