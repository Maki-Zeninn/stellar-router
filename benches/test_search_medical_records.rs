// ============================================================
// MEDICAL RECORD SEARCH TESTS
// ============================================================

#[cfg(test)]
mod test_search_medical_records {
    use crate::{
        Gender, MedicalRecordFilter, PetChainContract, PetChainContractClient, PrivacyLevel,
        Species,
    };
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String,
    };

    fn setup() -> (
        Env,
        PetChainContractClient<'static>,
        Address,
        Address,
        Address,
        u64,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, PetChainContract);
        let client = PetChainContractClient::new(&env, &contract_id);
        client.init_admin(&admin);

        let owner = Address::generate(&env);
        let vet = Address::generate(&env);
        let pet_id = client.register_pet(
            &owner,
            &String::from_str(&env, "Buddy"),
            &String::from_str(&env, "2020-01-01"),
            &Gender::Male,
            &Species::Dog,
            &String::from_str(&env, "Labrador"),
            &String::from_str(&env, "Brown"),
            &25u32,
            &None,
            &PrivacyLevel::Public,
        );

        client.register_vet(
            &vet,
            &String::from_str(&env, "Dr. Smith"),
            &String::from_str(&env, "LIC-001"),
            &String::from_str(&env, "General"),
        );
        client.verify_vet(&admin, &vet);

        (env, client, admin, owner, vet, pet_id)
    }

    fn add_record_at(
        client: &PetChainContractClient,
        env: &Env,
        pet_id: u64,
        vet: &Address,
        diagnosis: &str,
        timestamp: u64,
    ) -> u64 {
        env.ledger().with_mut(|ledger| ledger.timestamp = timestamp);
        client.add_medical_record(
            &pet_id,
            vet,
            &String::from_str(env, diagnosis),
            &String::from_str(env, "Treatment"),
            &soroban_sdk::Vec::new(env),
            &String::from_str(env, "Notes"),
        )
    }

    fn empty_filter() -> MedicalRecordFilter {
        MedicalRecordFilter {
            vet_address: None,
            from_date: None,
            to_date: None,
            diagnosis_keyword: None,
        }
    }

    #[test]
    fn test_search_medical_records_filters_by_diagnosis_keyword() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();

        add_record_at(&client, &env, pet_id, &vet, "Canine Flu", 10);
        add_record_at(&client, &env, pet_id, &vet, "Skin Allergy", 20);
        add_record_at(&client, &env, pet_id, &vet, "Flu Booster Follow-up", 30);

        let results = client.search_medical_records(
            &pet_id,
            &MedicalRecordFilter {
                diagnosis_keyword: Some(String::from_str(&env, "Flu")),
                ..empty_filter()
            },
            &0u64,
            &10u32,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(
            results.get(0).unwrap().diagnosis,
            String::from_str(&env, "Canine Flu")
        );
        assert_eq!(
            results.get(1).unwrap().diagnosis,
            String::from_str(&env, "Flu Booster Follow-up")
        );
    }

    #[test]
    fn test_search_medical_records_filters_by_vet_and_date_range() {
        let (env, client, admin, _owner, vet1, pet_id) = setup();
        let vet2 = Address::generate(&env);
        client.register_vet(
            &vet2,
            &String::from_str(&env, "Dr. Jones"),
            &String::from_str(&env, "LIC-002"),
            &String::from_str(&env, "Cardiology"),
        );
        client.verify_vet(&admin, &vet2);

        add_record_at(&client, &env, pet_id, &vet1, "Flu", 10);
        add_record_at(&client, &env, pet_id, &vet2, "Flu", 20);
        add_record_at(&client, &env, pet_id, &vet1, "Flu Recheck", 30);

        let results = client.search_medical_records(
            &pet_id,
            &MedicalRecordFilter {
                vet_address: Some(vet1.clone()),
                from_date: Some(15),
                to_date: Some(35),
                diagnosis_keyword: Some(String::from_str(&env, "Flu")),
            },
            &0u64,
            &10u32,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0).unwrap().vet_address, vet1);
        assert_eq!(
            results.get(0).unwrap().diagnosis,
            String::from_str(&env, "Flu Recheck")
        );
        assert_eq!(results.get(0).unwrap().date, 30);
    }

    #[test]
    fn test_search_medical_records_date_range_is_inclusive() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();

        add_record_at(&client, &env, pet_id, &vet, "Checkup", 50);

        let results = client.search_medical_records(
            &pet_id,
            &MedicalRecordFilter {
                from_date: Some(50),
                to_date: Some(50),
                ..empty_filter()
            },
            &0u64,
            &10u32,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results.get(0).unwrap().date, 50);
    }

    #[test]
    fn test_search_medical_records_paginates_filtered_results() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();

        add_record_at(&client, &env, pet_id, &vet, "Flu A", 10);
        add_record_at(&client, &env, pet_id, &vet, "Allergy", 20);
        add_record_at(&client, &env, pet_id, &vet, "Flu B", 30);
        add_record_at(&client, &env, pet_id, &vet, "Flu C", 40);

        let page = client.search_medical_records(
            &pet_id,
            &MedicalRecordFilter {
                diagnosis_keyword: Some(String::from_str(&env, "Flu")),
                ..empty_filter()
            },
            &1u64,
            &2u32,
        );

        assert_eq!(page.len(), 2);
        assert_eq!(page.get(0).unwrap().diagnosis, String::from_str(&env, "Flu B"));
        assert_eq!(page.get(1).unwrap().diagnosis, String::from_str(&env, "Flu C"));
    }

    #[test]
    fn test_search_medical_records_returns_empty_when_no_match() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();

        add_record_at(&client, &env, pet_id, &vet, "Allergy", 10);

        let results = client.search_medical_records(
            &pet_id,
            &MedicalRecordFilter {
                diagnosis_keyword: Some(String::from_str(&env, "Flu")),
                ..empty_filter()
            },
            &0u64,
            &10u32,
        );

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_medical_records_returns_empty_for_zero_limit_or_large_offset() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();

        add_record_at(&client, &env, pet_id, &vet, "Flu", 10);

        let zero_limit = client.search_medical_records(&pet_id, &empty_filter(), &0u64, &0u32);
        let large_offset = client.search_medical_records(&pet_id, &empty_filter(), &5u64, &2u32);

        assert_eq!(zero_limit.len(), 0);
        assert_eq!(large_offset.len(), 0);
    }
}
