// ============================================================
// get_pet ACCESS CONTROL TESTS
// ============================================================
//
// Covers the three PrivacyLevel variants crossed with the three
// AccessLevel tiers (None / Basic / Full) and explicit grants.

#[cfg(test)]
mod test_get_pet_access_control {
    use crate::{
        AccessLevel, Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species,
    };
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    // ---- helpers ----

    fn setup() -> (Env, PetChainContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &id);
        (env, client)
    }

    fn register(
        client: &PetChainContractClient,
        env: &Env,
        owner: &Address,
        privacy: PrivacyLevel,
    ) -> u64 {
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
            &privacy,
        )
    }

    // ================================================================
    // PUBLIC pets
    // ================================================================

    #[test]
    fn test_public_pet_owner_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Public);

        let result = client.get_pet(&pet_id, &owner);
        assert!(result.is_some(), "owner must always read their own public pet");
    }

    #[test]
    fn test_public_pet_stranger_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Public);

        // Public pets are readable by any authenticated address.
        let result = client.get_pet(&pet_id, &stranger);
        assert!(
            result.is_some(),
            "any viewer can read a Public pet without an explicit grant"
        );
    }

    #[test]
    fn test_public_pet_grantee_with_full_access_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Public);

        client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);

        let result = client.get_pet(&pet_id, &grantee);
        assert!(result.is_some());
    }

    // ================================================================
    // RESTRICTED pets
    // ================================================================

    #[test]
    fn test_restricted_pet_owner_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Restricted);

        let result = client.get_pet(&pet_id, &owner);
        assert!(result.is_some(), "owner must always read their own restricted pet");
    }

    #[test]
    fn test_restricted_pet_stranger_cannot_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Restricted);

        let result = client.get_pet(&pet_id, &stranger);
        assert!(
            result.is_none(),
            "stranger must not read a Restricted pet without a grant"
        );
    }

    #[test]
    fn test_restricted_pet_grantee_with_basic_access_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Restricted);

        client.grant_access(&pet_id, &grantee, &AccessLevel::Basic, &None);

        let result = client.get_pet(&pet_id, &grantee);
        assert!(result.is_some(), "Basic grant on Restricted pet must allow read");
    }

    #[test]
    fn test_restricted_pet_grantee_with_full_access_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Restricted);

        client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);

        let result = client.get_pet(&pet_id, &grantee);
        assert!(result.is_some(), "Full grant on Restricted pet must allow read");
    }

    #[test]
    fn test_restricted_pet_expired_grant_cannot_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Restricted);

        // Grant that expired in the past (timestamp 1 = far in the past).
        client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &Some(1u64));

        let result = client.get_pet(&pet_id, &grantee);
        assert!(
            result.is_none(),
            "expired grant must not allow access to a Restricted pet"
        );
    }

    // ================================================================
    // PRIVATE pets
    // ================================================================

    #[test]
    fn test_private_pet_owner_can_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Private);

        let result = client.get_pet(&pet_id, &owner);
        assert!(result.is_some(), "owner must always read their own private pet");
    }

    #[test]
    fn test_private_pet_stranger_cannot_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Private);

        let result = client.get_pet(&pet_id, &stranger);
        assert!(
            result.is_none(),
            "stranger must not read a Private pet"
        );
    }

    #[test]
    fn test_private_pet_grantee_with_full_access_cannot_read() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let pet_id = register(&client, &env, &owner, PrivacyLevel::Private);

        // Even an explicit Full grant cannot override Private level.
        client.grant_access(&pet_id, &grantee, &AccessLevel::Full, &None);

        let result = client.get_pet(&pet_id, &grantee);
        assert!(
            result.is_none(),
            "Private pets must only be readable by the owner, even with a Full grant"
        );
    }

    // ================================================================
    // Non-existent pet
    // ================================================================

    #[test]
    fn test_nonexistent_pet_returns_none() {
        let (env, client) = setup();
        let viewer = Address::generate(&env);
        assert!(client.get_pet(&9999u64, &viewer).is_none());
    }
}
