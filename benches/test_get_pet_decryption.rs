// ============================================================
// get_pet DECRYPTION ERROR PROPAGATION TESTS
// ============================================================
//
// decrypt_sensitive_data is currently a passthrough (Ok(ciphertext.clone())).
// Corrupt data is therefore simulated by storing a Pet whose encrypted fields
// contain raw bytes that cannot be XDR-decoded as the expected type (String /
// Vec<Allergy>).  The fix ensures get_pet returns None rather than a partial
// profile with sentinel "Error" strings.

#[cfg(test)]
mod test_get_pet_decryption {
    use crate::{
        DataKey, EncryptedData, Gender, Pet, PetChainContract, PetChainContractClient,
        PrivacyLevel, Species,
    };
    use soroban_sdk::{
        testutils::Address as _, Address, Bytes, Env, String, Vec,
    };

    // ---- helpers ----

    fn setup() -> (Env, PetChainContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        (env, client)
    }

    fn register_pet(client: &PetChainContractClient, env: &Env, owner: &Address) -> u64 {
        client.register_pet(
            owner,
            &String::from_str(env, "Buddy"),
            &String::from_str(env, "2020-01-01"),
            &Gender::Male,
            &Species::Dog,
            &String::from_str(env, "Labrador"),
            &String::from_str(env, "Brown"),
            &25u32,
            &None,
            &PrivacyLevel::Public,
        )
    }

    /// Overwrite a stored Pet's encrypted_name with bytes that are not valid
    /// XDR for a soroban String, then assert get_pet returns None.
    fn corrupt_pet_name(env: &Env, pet_id: u64) {
        let mut pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .expect("pet must exist before corruption");

        // 0xFF bytes are not valid XDR for a soroban String
        let garbage = Bytes::from_array(env, &[0xFF, 0xFE, 0xFD, 0xFC]);
        pet.encrypted_name = EncryptedData {
            ciphertext: garbage.clone(),
            nonce: garbage,
        };

        env.storage()
            .instance()
            .set(&DataKey::Pet(pet_id), &pet);
    }

    fn corrupt_pet_birthday(env: &Env, pet_id: u64) {
        let mut pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .expect("pet must exist");

        let garbage = Bytes::from_array(env, &[0xDE, 0xAD, 0xBE, 0xEF]);
        pet.encrypted_birthday = EncryptedData {
            ciphertext: garbage.clone(),
            nonce: garbage,
        };

        env.storage()
            .instance()
            .set(&DataKey::Pet(pet_id), &pet);
    }

    fn corrupt_pet_breed(env: &Env, pet_id: u64) {
        let mut pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .expect("pet must exist");

        let garbage = Bytes::from_array(env, &[0x00, 0x01, 0x02, 0x03]);
        pet.encrypted_breed = EncryptedData {
            ciphertext: garbage.clone(),
            nonce: garbage,
        };

        env.storage()
            .instance()
            .set(&DataKey::Pet(pet_id), &pet);
    }

    fn corrupt_pet_allergies(env: &Env, pet_id: u64) {
        let mut pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .expect("pet must exist");

        let garbage = Bytes::from_array(env, &[0xAB, 0xCD, 0xEF, 0x01]);
        pet.encrypted_allergies = EncryptedData {
            ciphertext: garbage.clone(),
            nonce: garbage,
        };

        env.storage()
            .instance()
            .set(&DataKey::Pet(pet_id), &pet);
    }

    // ---- happy path: valid data still works ----

    #[test]
    fn test_get_pet_valid_data_returns_some() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        let result = client.get_pet(&pet_id);
        assert!(result.is_some(), "valid pet must return Some");
        let profile = result.unwrap();
        assert_eq!(profile.id, pet_id);
        // Confirm no sentinel "Error" strings leak through
        assert_ne!(profile.name, String::from_str(&env, "Error"));
        assert_ne!(profile.birthday, String::from_str(&env, "Error"));
        assert_ne!(profile.breed, String::from_str(&env, "Error"));
    }

    // ---- corrupt fields return None, not masked output ----

    #[test]
    fn test_corrupt_name_returns_none() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        corrupt_pet_name(&env, pet_id);

        let result = client.get_pet(&pet_id);
        assert!(
            result.is_none(),
            "corrupt name ciphertext must yield None, not a partial profile"
        );
    }

    #[test]
    fn test_corrupt_birthday_returns_none() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        corrupt_pet_birthday(&env, pet_id);

        let result = client.get_pet(&pet_id);
        assert!(
            result.is_none(),
            "corrupt birthday ciphertext must yield None"
        );
    }

    #[test]
    fn test_corrupt_breed_returns_none() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        corrupt_pet_breed(&env, pet_id);

        let result = client.get_pet(&pet_id);
        assert!(
            result.is_none(),
            "corrupt breed ciphertext must yield None"
        );
    }

    #[test]
    fn test_corrupt_allergies_returns_none() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        corrupt_pet_allergies(&env, pet_id);

        let result = client.get_pet(&pet_id);
        assert!(
            result.is_none(),
            "corrupt allergies ciphertext must yield None"
        );
    }

    /// Verify the old sentinel value "Error" is never returned for any field,
    /// even when all fields are corrupted simultaneously.
    #[test]
    fn test_all_fields_corrupt_never_returns_error_sentinel() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        corrupt_pet_name(&env, pet_id);
        corrupt_pet_birthday(&env, pet_id);
        corrupt_pet_breed(&env, pet_id);
        corrupt_pet_allergies(&env, pet_id);

        let result = client.get_pet(&pet_id);
        // Must be None — never a profile containing "Error" strings
        assert!(result.is_none());
    }

    /// A non-existent pet must still return None (regression guard).
    #[test]
    fn test_nonexistent_pet_returns_none() {
        let (env, client) = setup();
        assert!(client.get_pet(&9999u64).is_none());
    }
}
