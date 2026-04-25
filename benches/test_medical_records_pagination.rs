#[cfg(test)]
mod test_medical_records_pagination {
    use crate::{Gender, PetChainContract, PetChainContractClient, PrivacyLevel, Species};
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    fn setup() -> (
        Env,
        PetChainContractClient<'static>,
        Address,
        Address,
        Address,
        u64,
    ) {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.mock_all_auths();

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

    fn add_record(
        client: &PetChainContractClient,
        env: &Env,
        pet_id: u64,
        vet: &Address,
        diagnosis: &str,
    ) {
        client.add_medical_record(
            &pet_id,
            vet,
            &String::from_str(env, diagnosis),
            &String::from_str(env, "Treatment"),
            &soroban_sdk::Vec::new(env),
            &String::from_str(env, "Notes"),
        );
    }

    #[test]
    fn returns_first_page_with_limit() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();
        add_record(&client, &env, pet_id, &vet, "A");
        add_record(&client, &env, pet_id, &vet, "B");
        add_record(&client, &env, pet_id, &vet, "C");

        let page = client.get_pet_medical_records(&pet_id, &0u64, &2u32);
        assert_eq!(page.len(), 2);
        assert_eq!(page.get(0).unwrap().diagnosis, String::from_str(&env, "A"));
        assert_eq!(page.get(1).unwrap().diagnosis, String::from_str(&env, "B"));
    }

    #[test]
    fn returns_middle_page_with_offset() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();
        add_record(&client, &env, pet_id, &vet, "A");
        add_record(&client, &env, pet_id, &vet, "B");
        add_record(&client, &env, pet_id, &vet, "C");
        add_record(&client, &env, pet_id, &vet, "D");

        let page = client.get_pet_medical_records(&pet_id, &1u64, &2u32);
        assert_eq!(page.len(), 2);
        assert_eq!(page.get(0).unwrap().diagnosis, String::from_str(&env, "B"));
        assert_eq!(page.get(1).unwrap().diagnosis, String::from_str(&env, "C"));
    }

    #[test]
    fn returns_empty_when_offset_out_of_range_or_limit_zero() {
        let (env, client, _admin, _owner, vet, pet_id) = setup();
        add_record(&client, &env, pet_id, &vet, "Only");

        let out_of_range = client.get_pet_medical_records(&pet_id, &5u64, &2u32);
        assert_eq!(out_of_range.len(), 0);

        let zero_limit = client.get_pet_medical_records(&pet_id, &0u64, &0u32);
        assert_eq!(zero_limit.len(), 0);
    }
}
