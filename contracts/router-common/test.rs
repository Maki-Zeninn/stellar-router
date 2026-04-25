use super::{ContractError, DataKey, OwnershipRecord, PetOwnershipContract, PetOwnershipContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Error, Vec};

fn setup() -> (Env, Address, Address, u64) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let pet_id = 1;

    (env, owner, new_owner, pet_id)
}

fn create_pending_transfer(
    client: &PetOwnershipContractClient,
    pet_id: u64,
    owner: &Address,
    new_owner: &Address,
) {
    client.create_pet(&pet_id, owner);
    client.initiate_transfer(&pet_id, new_owner);
}

#[test]
fn accept_transfer_errors_when_history_is_missing() {
    let (env, owner, new_owner, pet_id) = setup();
    let contract_id = env.register_contract(None, PetOwnershipContract);
    let client = PetOwnershipContractClient::new(&env, &contract_id);
    create_pending_transfer(&client, pet_id, &owner, &new_owner);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::OwnershipHistory(pet_id));
    });

    let result = client.try_accept_transfer(&pet_id);
    assert_eq!(
        result,
        Err(Ok(Error::from_contract_error(
            ContractError::EmptyOwnershipHistory as u32,
        )))
    );
}

#[test]
fn accept_transfer_errors_when_history_is_empty() {
    let (env, owner, new_owner, pet_id) = setup();
    let contract_id = env.register_contract(None, PetOwnershipContract);
    let client = PetOwnershipContractClient::new(&env, &contract_id);
    create_pending_transfer(&client, pet_id, &owner, &new_owner);

    let empty_history = Vec::<OwnershipRecord>::new(&env);
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::OwnershipHistory(pet_id), &empty_history);
    });

    let result = client.try_accept_transfer(&pet_id);
    assert_eq!(
        result,
        Err(Ok(Error::from_contract_error(
            ContractError::EmptyOwnershipHistory as u32,
        )))
    );
}

#[test]
fn cancel_transfer_errors_when_stale() {
    let (env, owner, new_owner, pet_id) = setup();
    let contract_id = env.register_contract(None, PetOwnershipContract);
    let client = PetOwnershipContractClient::new(&env, &contract_id);
    
    // Create pet and initiate a transfer
    create_pending_transfer(&client, pet_id, &owner, &new_owner);

    // Deliberately alter the pet's current_owner to simulate a divergent state
    let rogue_owner = Address::generate(&env);
    env.as_contract(&contract_id, || {
        let mut pet: super::Pet = env.storage().persistent().get(&DataKey::Pet(pet_id)).unwrap();
        pet.current_owner = rogue_owner;
        env.storage().persistent().set(&DataKey::Pet(pet_id), &pet);
    });

    // The original owner tries to cancel the transfer, but they no longer match pet.current_owner
    let result = client.try_cancel_transfer(&pet_id);
    assert_eq!(
        result,
        Err(Ok(Error::from_contract_error(
            ContractError::StaleCancellation as u32,
        )))
    );
}
