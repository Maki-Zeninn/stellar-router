use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

// ── helpers ──────────────────────────────────────────────────────────────────

fn repeat(env: &Env, byte: u8, n: usize) -> String {
    let mut buf = [0u8; 1024];
    for i in 0..n {
        buf[i] = byte;
    }
    String::from_bytes(env, &buf[..n])
}

fn setup(env: &Env) -> (PetChainContractClient, Address, Address, u64) {
    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let owner = Address::generate(env);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(env, "Buddy"),
        &String::from_str(env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(env, "Labrador"),
        &String::from_str(env, "Brown"),
        &25u32,
        &None,
        &PrivacyLevel::Public,
    );

    (client, admin, owner, pet_id)
}

fn setup_with_vet(env: &Env) -> (PetChainContractClient, Address, Address, Address, u64) {
    let (client, admin, owner, pet_id) = setup(env);
    let vet = Address::generate(env);
    client.register_vet(
        &vet,
        &String::from_str(env, "Dr. Test"),
        &String::from_str(env, "LIC-TEST-001"),
        &String::from_str(env, "General"),
    );
    client.verify_vet(&admin, &vet);
    (client, admin, owner, vet, pet_id)
}

// ── add_behavior_record ───────────────────────────────────────────────────────

#[test]
fn test_behavior_description_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    let id = client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &5,
        &repeat(&env, b'a', 1000),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_behavior_description_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &5,
        &repeat(&env, b'a', 1001),
    );
}

// ── add_training_milestone ────────────────────────────────────────────────────

#[test]
fn test_milestone_name_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    let id = client.add_training_milestone(
        &pet_id,
        &repeat(&env, b'm', 100),
        &String::from_str(&env, "notes"),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_milestone_name_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    client.add_training_milestone(
        &pet_id,
        &repeat(&env, b'm', 101),
        &String::from_str(&env, "notes"),
    );
}

#[test]
fn test_milestone_notes_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    let id = client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Sit"),
        &repeat(&env, b'n', 1000),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_milestone_notes_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Sit"),
        &repeat(&env, b'n', 1001),
    );
}

// ── add_activity_record ───────────────────────────────────────────────────────

#[test]
fn test_activity_notes_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    let id = client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &5,
        &1000,
        &repeat(&env, b'n', 1000),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_activity_notes_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, pet_id) = setup(&env);

    client.add_activity_record(
        &pet_id,
        &ActivityType::Walk,
        &30,
        &5,
        &1000,
        &repeat(&env, b'n', 1001),
    );
}

// ── add_treatment ─────────────────────────────────────────────────────────────

#[test]
fn test_treatment_notes_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let id = client.add_treatment(
        &pet_id,
        &vet,
        &TreatmentType::Routine,
        &0,
        &repeat(&env, b'n', 1000),
        &None,
        &String::from_str(&env, "ok"),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_treatment_notes_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_treatment(
        &pet_id,
        &vet,
        &TreatmentType::Routine,
        &0,
        &repeat(&env, b'n', 1001),
        &None,
        &String::from_str(&env, "ok"),
    );
}

#[test]
fn test_treatment_outcome_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let id = client.add_treatment(
        &pet_id,
        &vet,
        &TreatmentType::Routine,
        &0,
        &String::from_str(&env, "notes"),
        &None,
        &repeat(&env, b'o', 100),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_treatment_outcome_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_treatment(
        &pet_id,
        &vet,
        &TreatmentType::Routine,
        &0,
        &String::from_str(&env, "notes"),
        &None,
        &repeat(&env, b'o', 101),
    );
}

// ── add_lab_result ────────────────────────────────────────────────────────────

#[test]
fn test_lab_result_fields_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let id = client.add_lab_result(
        &pet_id,
        &vet,
        &repeat(&env, b't', 100),
        &repeat(&env, b'r', 1000),
        &repeat(&env, b'f', 1000),
        &None,
        &None,
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_lab_result_test_type_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_lab_result(
        &pet_id,
        &vet,
        &repeat(&env, b't', 101),
        &String::from_str(&env, "results"),
        &String::from_str(&env, "ranges"),
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_lab_result_results_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_lab_result(
        &pet_id,
        &vet,
        &String::from_str(&env, "CBC"),
        &repeat(&env, b'r', 1001),
        &String::from_str(&env, "ranges"),
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_lab_result_reference_ranges_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_lab_result(
        &pet_id,
        &vet,
        &String::from_str(&env, "CBC"),
        &String::from_str(&env, "results"),
        &repeat(&env, b'f', 1001),
        &None,
        &None,
    );
}

// ── add_medical_record ────────────────────────────────────────────────────────

#[test]
fn test_medical_record_fields_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let id = client.add_medical_record(
        &pet_id,
        &vet,
        &repeat(&env, b'd', 1000),
        &repeat(&env, b't', 1000),
        &Vec::new(&env),
        &repeat(&env, b'n', 1000),
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_medical_record_diagnosis_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_medical_record(
        &pet_id,
        &vet,
        &repeat(&env, b'd', 1001),
        &String::from_str(&env, "treatment"),
        &Vec::new(&env),
        &String::from_str(&env, "notes"),
    );
}

#[test]
#[should_panic]
fn test_medical_record_treatment_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "diagnosis"),
        &repeat(&env, b't', 1001),
        &Vec::new(&env),
        &String::from_str(&env, "notes"),
    );
}

#[test]
#[should_panic]
fn test_medical_record_notes_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "diagnosis"),
        &String::from_str(&env, "treatment"),
        &Vec::new(&env),
        &repeat(&env, b'n', 1001),
    );
}

#[test]
#[should_panic]
fn test_medical_record_too_many_medications_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, owner, vet, pet_id) = setup_with_vet(&env);

    let mut meds: Vec<Medication> = Vec::new(&env);
    for i in 0..51u64 {
        meds.push_back(Medication {
            id: i,
            pet_id,
            name: String::from_str(&env, "Med"),
            dosage: String::from_str(&env, "1mg"),
            frequency: String::from_str(&env, "daily"),
            start_date: 0,
            end_date: None,
            prescribing_vet: vet.clone(),
            active: true,
        });
    }

    client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "diagnosis"),
        &String::from_str(&env, "treatment"),
        &meds,
        &String::from_str(&env, "notes"),
    );
    let _ = owner;
}

// ── add_medication ────────────────────────────────────────────────────────────

#[test]
fn test_medication_fields_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let id = client.add_medication(
        &pet_id,
        &repeat(&env, b'n', 100),
        &repeat(&env, b'd', 100),
        &repeat(&env, b'f', 100),
        &0,
        &None,
        &vet,
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_medication_name_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_medication(
        &pet_id,
        &repeat(&env, b'n', 101),
        &String::from_str(&env, "1mg"),
        &String::from_str(&env, "daily"),
        &0,
        &None,
        &vet,
    );
}

#[test]
#[should_panic]
fn test_medication_dosage_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_medication(
        &pet_id,
        &String::from_str(&env, "Aspirin"),
        &repeat(&env, b'd', 101),
        &String::from_str(&env, "daily"),
        &0,
        &None,
        &vet,
    );
}

#[test]
#[should_panic]
fn test_medication_frequency_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    client.add_medication(
        &pet_id,
        &String::from_str(&env, "Aspirin"),
        &String::from_str(&env, "1mg"),
        &repeat(&env, b'f', 101),
        &0,
        &None,
        &vet,
    );
}

// ── add_attachment (vec limit) ────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_attachment_vec_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let record_id = client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "diagnosis"),
        &String::from_str(&env, "treatment"),
        &Vec::new(&env),
        &String::from_str(&env, "notes"),
    );

    for i in 0..20u32 {
        let hash = if i < 10 {
            String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG")
        } else {
            String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdH")
        };
        let meta = AttachmentMetadata {
            filename: String::from_str(&env, "file.pdf"),
            file_type: String::from_str(&env, "pdf"),
            size: 1024,
            uploaded_date: 0,
        };
        client.add_attachment(&record_id, &hash, &meta);
    }

    let meta = AttachmentMetadata {
        filename: String::from_str(&env, "extra.pdf"),
        file_type: String::from_str(&env, "pdf"),
        size: 512,
        uploaded_date: 0,
    };
    client.add_attachment(
        &record_id,
        &String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"),
        &meta,
    );
}

#[test]
fn test_attachment_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _owner, vet, pet_id) = setup_with_vet(&env);

    let record_id = client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "diagnosis"),
        &String::from_str(&env, "treatment"),
        &Vec::new(&env),
        &String::from_str(&env, "notes"),
    );

    for _i in 0..20u32 {
        let meta = AttachmentMetadata {
            filename: String::from_str(&env, "file.pdf"),
            file_type: String::from_str(&env, "pdf"),
            size: 1024,
            uploaded_date: 0,
        };
        let result = client.add_attachment(
            &record_id,
            &String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"),
            &meta,
        );
        assert!(result);
    }
    assert_eq!(client.get_attachment_count(&record_id), 20);
}

// ── add_vet_review (comment length) ──────────────────────────────────────────

#[test]
fn test_review_comment_at_limit_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, owner, _pet_id) = setup(&env);

    let vet = Address::generate(&env);
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Limit"),
        &String::from_str(&env, "LIC-LIMIT-001"),
        &String::from_str(&env, "General"),
    );
    client.verify_vet(&admin, &vet);

    let id = client.add_vet_review(&owner, &vet, &5, &repeat(&env, b'c', 500));
    assert!(id > 0);
}

#[test]
fn test_review_comment_over_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, owner, _pet_id) = setup(&env);

    let vet = Address::generate(&env);
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Over"),
        &String::from_str(&env, "LIC-OVER-001"),
        &String::from_str(&env, "General"),
    );
    client.verify_vet(&admin, &vet);

    let result = client.try_add_vet_review(&owner, &vet, &5, &repeat(&env, b'c', 501));
    assert!(result.is_err());
}

#[test]
fn test_review_empty_comment_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, owner, _pet_id) = setup(&env);

    let vet = Address::generate(&env);
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Empty"),
        &String::from_str(&env, "LIC-EMPTY-001"),
        &String::from_str(&env, "General"),
    );
    client.verify_vet(&admin, &vet);

    let id = client.add_vet_review(&owner, &vet, &3, &String::from_str(&env, ""));
    assert!(id > 0);
}
