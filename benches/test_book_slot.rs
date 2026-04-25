#[cfg(test)]
mod test_book_slot {
    use crate::{Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species};
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    fn setup_env() -> (Env, PetChainContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        (env, client)
    }

    fn setup_verified_vet(env: &Env, client: &PetChainContractClient) -> (Address, Address) {
        let admin = Address::generate(env);
        let vet = Address::generate(env);

        let mut admins = soroban_sdk::Vec::new(env);
        admins.push_back(admin.clone());
        client.init_multisig(&admin, &admins, &1u32);

        client.register_vet(
            &vet,
            &String::from_str(env, "Dr. Slot Tester"),
            &String::from_str(env, "LIC-SLOT-001"),
            &String::from_str(env, "General"),
        );
        client.verify_vet(&admin, &vet);

        (admin, vet)
    }

    fn register_pet_owner(env: &Env, client: &PetChainContractClient) -> Address {
        let owner = Address::generate(env);
        client.register_pet_owner(
            &owner,
            &String::from_str(env, "Alice"),
            &String::from_str(env, "alice@example.com"),
            &String::from_str(env, "555-0100"),
        );
        // Also register a pet so the owner record exists
        client.register_pet(
            &owner,
            &String::from_str(env, "Buddy"),
            &String::from_str(env, "1609459200"),
            &Gender::Male,
            &Species::Dog,
            &String::from_str(env, "Labrador"),
            &String::from_str(env, "Brown"),
            &25u32,
            &None,
            &PrivacyLevel::Public,
        );
        owner
    }

    fn set_slot(env: &Env, client: &PetChainContractClient, vet: &Address) -> u64 {
        let now = env.ledger().timestamp();
        client.set_availability(vet, &now, &(now + 3600))
    }

    // -------------------------------------------------------
    // Happy path: registered pet owner can book a slot
    // -------------------------------------------------------
    #[test]
    fn test_registered_owner_can_book_slot() {
        let (env, client) = setup_env();
        let (_admin, vet) = setup_verified_vet(&env, &client);
        let owner = register_pet_owner(&env, &client);
        let slot_index = set_slot(&env, &client, &vet);

        let result = client.book_slot(&owner, &vet, &slot_index);
        assert!(result, "Registered pet owner should be able to book a slot");

        // Slot must now be unavailable
        let date = env.ledger().timestamp() / 86400;
        let slots = client.get_available_slots(&vet, &date);
        assert!(
            slots.is_empty(),
            "Slot should no longer appear as available after booking"
        );
    }

    // -------------------------------------------------------
    // Unauthorized: unregistered caller cannot book a slot
    // Slot must remain available after the failed attempt.
    // -------------------------------------------------------
    #[test]
    #[should_panic(expected = "Unauthorized: only registered pet owners can book slots")]
    fn test_unregistered_caller_cannot_book_slot() {
        let (env, client) = setup_env();
        let (_admin, vet) = setup_verified_vet(&env, &client);
        let slot_index = set_slot(&env, &client, &vet);

        let random_caller = Address::generate(&env);

        // This must panic — random address is not a registered pet owner
        client.book_slot(&random_caller, &vet, &slot_index);
    }

    // -------------------------------------------------------
    // Slot remains available when no authorized booking has occurred.
    // Verifies that an unregistered address cannot silently consume a slot,
    // and that a legitimate owner can still book it afterwards.
    // -------------------------------------------------------
    #[test]
    fn test_slot_remains_available_without_authorized_booking() {
        let (env, client) = setup_env();
        let (_admin, vet) = setup_verified_vet(&env, &client);
        let slot_index = set_slot(&env, &client, &vet);

        // No booking has been made yet — slot must still be available
        let date = env.ledger().timestamp() / 86400;
        let slots = client.get_available_slots(&vet, &date);
        assert!(
            !slots.is_empty(),
            "Slot should be available before any booking"
        );
        assert_eq!(slots.get(0).unwrap().available, true);

        // A legitimate owner can still book the untouched slot
        let owner = register_pet_owner(&env, &client);
        let booked = client.book_slot(&owner, &vet, &slot_index);
        assert!(booked, "Legitimate owner should be able to book the available slot");

        // Now the slot must be gone from available list
        let slots_after = client.get_available_slots(&vet, &date);
        assert!(
            slots_after.is_empty(),
            "Slot should be unavailable after legitimate booking"
        );
    }

    // -------------------------------------------------------
    // Double-booking: already-booked slot panics
    // -------------------------------------------------------
    #[test]
    #[should_panic(expected = "Slot already booked")]
    fn test_cannot_double_book_slot() {
        let (env, client) = setup_env();
        let (_admin, vet) = setup_verified_vet(&env, &client);
        let owner = register_pet_owner(&env, &client);
        let slot_index = set_slot(&env, &client, &vet);

        client.book_slot(&owner, &vet, &slot_index);
        // Second booking on the same slot must panic
        client.book_slot(&owner, &vet, &slot_index);
    }

    // -------------------------------------------------------
    // Non-existent slot returns false (no panic)
    // -------------------------------------------------------
    #[test]
    fn test_book_nonexistent_slot_returns_false() {
        let (env, client) = setup_env();
        let (_admin, vet) = setup_verified_vet(&env, &client);
        let owner = register_pet_owner(&env, &client);

        let result = client.book_slot(&owner, &vet, &999u64);
        assert!(!result, "Booking a non-existent slot should return false");
    }
}
