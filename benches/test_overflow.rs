// ============================================================
// OVERFLOW SAFETY TESTS
// Tests that safe_increment panics at u64::MAX and that normal
// counter increments work correctly up to the boundary.
// ============================================================

#[cfg(test)]
mod test_overflow {
    use crate::{
        ActivityKey, AlertKey, BehaviorKey, ConsentKey, DataKey, Gender, InsuranceKey, MedicalKey,
        NutritionKey, PetChainContract, PetChainContractClient, PrivacyLevel, Species, SystemKey,
        TreatmentKey,
    };
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    fn register_pet(client: &PetChainContractClient, env: &Env, owner: &Address) -> u64 {
        client.register_pet(
            owner,
            &String::from_str(env, "Buddy"),
            &String::from_str(env, "1609459200"),
            &Gender::Male,
            &Species::Dog,
            &String::from_str(env, "Labrador"),
            &String::from_str(env, "Brown"),
            &25u32,
            &None,
            &PrivacyLevel::Public,
        )
    }

    fn setup_verified_vet(client: &PetChainContractClient, env: &Env) -> (Address, Address) {
        let admin = Address::generate(env);
        let vet = Address::generate(env);
        let mut admins = soroban_sdk::Vec::new(env);
        admins.push_back(admin.clone());
        client.init_multisig(&admin, &admins, &1u32);
        client.register_vet(
            &vet,
            &String::from_str(env, "Dr. Test"),
            &String::from_str(env, "LIC-OVF-001"),
            &String::from_str(env, "General"),
        );
        client.verify_vet(&admin, &vet);
        (admin, vet)
    }

    // --- safe_increment unit tests ---

    #[test]
    fn test_safe_increment_normal() {
        assert_eq!(crate::safe_increment(0), 1);
        assert_eq!(crate::safe_increment(100), 101);
        assert_eq!(crate::safe_increment(u64::MAX - 1), u64::MAX);
    }

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_safe_increment_at_max_panics() {
        crate::safe_increment(u64::MAX);
    }

    // --- PetCount overflow ---

    #[test]
    fn test_pet_count_increments_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        assert_eq!(register_pet(&client, &env, &owner), 1);
        assert_eq!(register_pet(&client, &env, &owner), 2);
        assert_eq!(register_pet(&client, &env, &owner), 3);
        assert_eq!(client.get_total_pets(), 3);
    }

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_pet_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage().instance().set(&DataKey::PetCount, &u64::MAX);
        });

        register_pet(&client, &env, &owner);
    }

    // --- VaccinationCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_vaccination_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let (_, vet) = setup_verified_vet(&client, &env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&MedicalKey::VaccinationCount, &u64::MAX);
        });

        client.add_vaccination(
            &pet_id,
            &vet,
            &crate::VaccineType::Rabies,
            &String::from_str(&env, "RabiesVax"),
            &1000u64,
            &2000u64,
            &String::from_str(&env, "BATCH-001"),
        );
    }

    // --- MedicalRecordCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_medical_record_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let (_, vet) = setup_verified_vet(&client, &env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&MedicalKey::MedicalRecordCount, &u64::MAX);
        });

        client.add_medical_record(
            &pet_id,
            &vet,
            &String::from_str(&env, "Diagnosis"),
            &String::from_str(&env, "Treatment"),
            &soroban_sdk::Vec::new(&env),
            &String::from_str(&env, "Notes"),
        );
    }

    // --- LostPetAlertCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_alert_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&AlertKey::LostPetAlertCount, &u64::MAX);
        });

        client.report_lost(&pet_id, &String::from_str(&env, "Park"), &None);
    }

    // --- BehaviorRecordCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_behavior_record_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&BehaviorKey::BehaviorRecordCount, &u64::MAX);
        });

        client.add_behavior_record(
            &pet_id,
            &crate::BehaviorType::Training,
            &5u32,
            &String::from_str(&env, "Good behavior"),
        );
    }

    // --- ActivityRecordCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_activity_record_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&ActivityKey::ActivityRecordCount, &u64::MAX);
        });

        client.add_activity_record(
            &pet_id,
            &crate::ActivityType::Walk,
            &30u32,
            &5u32,
            &1000u32,
            &String::from_str(&env, "Morning walk"),
        );
    }

    // --- InsuranceClaimCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_insurance_claim_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        client.add_insurance_policy(
            &pet_id,
            &String::from_str(&env, "POL-001"),
            &String::from_str(&env, "PetInsure"),
            &String::from_str(&env, "Comprehensive"),
            &100u64,
            &10000u64,
            &9999999999u64,
        );

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&InsuranceKey::ClaimCount, &u64::MAX);
        });

        client.submit_insurance_claim(&pet_id, &500u64, &String::from_str(&env, "Vet visit"));
    }

    // --- TreatmentCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_treatment_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let (_, vet) = setup_verified_vet(&client, &env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&TreatmentKey::TreatmentCount, &u64::MAX);
        });

        client.add_treatment(
            &pet_id,
            &vet,
            &crate::TreatmentType::Routine,
            &1000u64,
            &String::from_str(&env, "Checkup"),
            &None,
            &String::from_str(&env, "Good"),
        );
    }

    // --- ConsentCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_consent_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);
        let grantee = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&ConsentKey::ConsentCount, &u64::MAX);
        });

        client.grant_consent(&pet_id, &owner, &crate::ConsentType::Research, &grantee);
    }

    // --- DietPlanCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_diet_plan_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&NutritionKey::DietPlanCount, &u64::MAX);
        });

        client.set_diet_plan(
            &pet_id,
            &String::from_str(&env, "Dry food"),
            &String::from_str(&env, "200g"),
            &String::from_str(&env, "Twice daily"),
            &soroban_sdk::Vec::new(&env),
            &soroban_sdk::Vec::new(&env),
        );
    }

    // --- OwnershipRecordCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_ownership_record_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // register_pet calls log_ownership_change which increments OwnershipRecordCount
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&SystemKey::OwnershipRecordCount, &u64::MAX);
        });

        register_pet(&client, &env, &owner);
    }

    // --- PetTransferProposalCount overflow ---

    #[test]
    #[should_panic(expected = "counter overflow")]
    fn test_transfer_proposal_count_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let pet_id = register_pet(&client, &env, &owner);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(owner.clone());
        client.configure_multisig(&pet_id, &signers, &1u32);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&SystemKey::PetTransferProposalCount, &u64::MAX);
        });

        client.require_multisig_for_transfer(&pet_id, &new_owner);
    }
}
