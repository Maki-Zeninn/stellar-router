use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

#[test]
fn test_owner_can_read_emergency_info() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &25u32,
        &None,
        &PrivacyLevel::Private,
    );

    let mut contacts = Vec::new(&env);
    contacts.push_back(EmergencyContact {
        name: String::from_str(&env, "Emergency Name"),
        phone: String::from_str(&env, "555-1234"),
        email: String::from_str(&env, "emergency@test.com"),
        relationship: String::from_str(&env, "Friend"),
        is_primary: true,
    });

    client.set_emergency_contacts(
        &pet_id,
        &contacts,
        &Vec::new(&env),
        &String::from_str(&env, ""),
    );

    // Owner can always read their own emergency info
    let info = client.get_emergency_info(&pet_id, &owner);
    assert_eq!(info.pet_id, pet_id);
    assert_eq!(info.species, String::from_str(&env, "Dog"));
    assert_eq!(info.emergency_contacts.len(), 1);
    assert_eq!(
        info.emergency_contacts.get(0).unwrap().phone,
        String::from_str(&env, "555-1234")
    );
}

#[test]
fn test_approved_responder_can_read_emergency_info() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let responder = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Rex"),
        &String::from_str(&env, "2019-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Boxer"),
        &String::from_str(&env, "Brindle"),
        &30u32,
        &None,
        &PrivacyLevel::Private,
    );

    let mut contacts = Vec::new(&env);
    contacts.push_back(EmergencyContact {
        name: String::from_str(&env, "Dr. Smith"),
        phone: String::from_str(&env, "555-9999"),
        email: String::from_str(&env, "dr@vet.com"),
        relationship: String::from_str(&env, "Vet"),
        is_primary: true,
    });

    client.set_emergency_contacts(
        &pet_id,
        &contacts,
        &Vec::new(&env),
        &String::from_str(&env, ""),
    );

    // Owner grants responder access
    client.add_emergency_responder(&pet_id, &responder);

    let info = client.get_emergency_info(&pet_id, &responder);
    assert_eq!(info.pet_id, pet_id);
    assert_eq!(info.emergency_contacts.len(), 1);
}

#[test]
#[should_panic]
fn test_unauthorized_address_cannot_read_emergency_info() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Luna"),
        &String::from_str(&env, "2021-03-20"),
        &Gender::Female,
        &Species::Cat,
        &String::from_str(&env, "Siamese"),
        &String::from_str(&env, "Cream"),
        &8u32,
        &None,
        &PrivacyLevel::Public,
    );

    client.set_emergency_contacts(
        &pet_id,
        &Vec::new(&env),
        &Vec::new(&env),
        &String::from_str(&env, ""),
    );

    // Stranger was never approved — must panic
    client.get_emergency_info(&pet_id, &stranger);
}

#[test]
#[should_panic]
fn test_revoked_responder_cannot_read_emergency_info() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let responder = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Labrador"),
        &String::from_str(&env, "Black"),
        &28u32,
        &None,
        &PrivacyLevel::Public,
    );

    client.set_emergency_contacts(
        &pet_id,
        &Vec::new(&env),
        &Vec::new(&env),
        &String::from_str(&env, ""),
    );

    client.add_emergency_responder(&pet_id, &responder);
    client.remove_emergency_responder(&pet_id, &responder);

    // Revoked responder must no longer have access
    client.get_emergency_info(&pet_id, &responder);
}

#[test]
fn test_emergency_data_filtering() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Rex"),
        &String::from_str(&env, "2019-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Boxer"),
        &String::from_str(&env, "Brindle"),
        &30u32,
        &None,
        &PrivacyLevel::Private,
    );

    let mut allergies = Vec::new(&env);
    allergies.push_back(Allergy {
        name: String::from_str(&env, "Penicillin"),
        severity: String::from_str(&env, "Critical"),
        is_critical: true,
    });
    allergies.push_back(Allergy {
        name: String::from_str(&env, "Dust"),
        severity: String::from_str(&env, "Mild"),
        is_critical: false,
    });

    client.set_emergency_contacts(
        &pet_id,
        &Vec::new(&env),
        &allergies,
        &String::from_str(&env, "Needs daily medication"),
    );

    let info = client.get_emergency_info(&pet_id, &owner);

    // Only the critical allergy should be returned
    assert_eq!(info.allergies.len(), 1);
    assert_eq!(
        info.allergies.get(0).unwrap().name,
        String::from_str(&env, "Penicillin")
    );
    assert!(info.allergies.get(0).unwrap().is_critical);
    assert_eq!(info.critical_alerts.len(), 1);
}
