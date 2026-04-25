use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

// Helper function to setup test environment
fn setup_test_env() -> (
    Env,
    PetChainContractClient<'static>,
    Address,
    Address,
    u64,
    u64,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    // Register owner and pet
    let owner = Address::generate(&env);
    let vet = Address::generate(&env);

    // Register vet
    client.register_vet(
        &vet,
        &String::from_str(&env, "Dr. Smith"),
        &String::from_str(&env, "VET12345"),
        &String::from_str(&env, "General Practice"),
    );

    // Initialize admin and verify vet
    let admin = Address::generate(&env);
    client.init_admin(&admin);
    client.verify_vet(&admin, &vet);

    // Register pet
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
        &PrivacyLevel::Public,
    );

    // Add a medical record
    let record_id = client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "Annual checkup"),
        &String::from_str(&env, "Healthy"),
        &soroban_sdk::Vec::new(&env),
        &String::from_str(&env, "All vitals normal"),
    );

    (env, client, owner, vet, pet_id, record_id)
}

// Helper function to create test attachment metadata
fn create_test_metadata(
    env: &Env,
    filename: &str,
    file_type: &str,
    size: u64,
) -> AttachmentMetadata {
    AttachmentMetadata {
        filename: String::from_str(env, filename),
        file_type: String::from_str(env, file_type),
        size,
        uploaded_date: env.ledger().timestamp(),
    }
}

#[test]
fn test_add_attachment_success() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "xray_001.jpg", "image/jpeg", 1024000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    let result = client.add_attachment(&record_id, &ipfs_hash, &metadata);
    assert!(result);

    // Verify attachment was added
    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments.get(0).unwrap().ipfs_hash, ipfs_hash);
    assert_eq!(
        attachments.get(0).unwrap().metadata.filename,
        String::from_str(&env, "xray_001.jpg")
    );
}

#[test]
fn test_add_multiple_attachments() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Add first attachment
    let metadata1 = create_test_metadata(&env, "xray_001.jpg", "image/jpeg", 1024000);
    let ipfs_hash1 = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.add_attachment(&record_id, &ipfs_hash1, &metadata1);

    // Add second attachment
    let metadata2 = create_test_metadata(&env, "blood_test.pdf", "application/pdf", 512000);
    let ipfs_hash2 = String::from_str(&env, "QmT5NvUtoM5nWFfrQdVrFtvGfKFmG7AHE8P34isapyhCxX");
    client.add_attachment(&record_id, &ipfs_hash2, &metadata2);

    // Add third attachment
    let metadata3 = create_test_metadata(&env, "ultrasound.png", "image/png", 2048000);
    let ipfs_hash3 = String::from_str(&env, "QmPK1s3pNYLi9ERiq3BDxKa4XosgWwFRQUydHUtz4YgpqB");
    client.add_attachment(&record_id, &ipfs_hash3, &metadata3);

    // Verify all attachments were added
    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.len(), 3);
    assert_eq!(
        attachments.get(0).unwrap().metadata.filename,
        String::from_str(&env, "xray_001.jpg")
    );
    assert_eq!(
        attachments.get(1).unwrap().metadata.filename,
        String::from_str(&env, "blood_test.pdf")
    );
    assert_eq!(
        attachments.get(2).unwrap().metadata.filename,
        String::from_str(&env, "ultrasound.png")
    );
}

#[test]
fn test_get_attachments_empty() {
    let (_env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.len(), 0);
}

#[test]
fn test_get_attachments_nonexistent_record() {
    let (_env, client, _owner, _vet, _pet_id, _record_id) = setup_test_env();

    let attachments = client.get_attachments(&999u64);
    assert_eq!(attachments.len(), 0);
}

#[test]
fn test_attachment_metadata_storage() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let timestamp = env.ledger().timestamp();
    let metadata = AttachmentMetadata {
        filename: String::from_str(&env, "detailed_report.pdf"),
        file_type: String::from_str(&env, "application/pdf"),
        size: 3145728, // 3MB
        uploaded_date: timestamp,
    };
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    client.add_attachment(&record_id, &ipfs_hash, &metadata);

    let attachments = client.get_attachments(&record_id);
    let stored_attachment = attachments.get(0).unwrap();

    assert_eq!(
        stored_attachment.metadata.filename,
        String::from_str(&env, "detailed_report.pdf")
    );
    assert_eq!(
        stored_attachment.metadata.file_type,
        String::from_str(&env, "application/pdf")
    );
    assert_eq!(stored_attachment.metadata.size, 3145728);
    assert_eq!(stored_attachment.metadata.uploaded_date, timestamp);
}

#[test]
#[should_panic]

fn test_add_attachment_invalid_ipfs_hash_too_short() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "xray.jpg", "image/jpeg", 1024000);
    let invalid_hash = String::from_str(&env, "short"); // Too short

    client.add_attachment(&record_id, &invalid_hash, &metadata);
}

#[test]
#[should_panic]

fn test_add_attachment_invalid_ipfs_hash_too_long() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "xray.jpg", "image/jpeg", 1024000);
    // Create a string longer than 128 characters by repeating a pattern
    let mut _long_chars = soroban_sdk::String::from_str(&env, "");
    for _ in 0..130 {
        _long_chars = soroban_sdk::String::from_str(&env, "a");
    }
    let long_string = soroban_sdk::String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdGQmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdGQmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    client.add_attachment(&record_id, &long_string, &metadata);
}

#[test]
#[should_panic]
fn test_add_attachment_empty_filename() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "", "image/jpeg", 1024000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    client.add_attachment(&record_id, &ipfs_hash, &metadata);
}

#[test]
#[should_panic]
fn test_add_attachment_empty_file_type() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "xray.jpg", "", 1024000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    client.add_attachment(&record_id, &ipfs_hash, &metadata);
}

#[test]
#[should_panic]
fn test_add_attachment_zero_file_size() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "xray.jpg", "image/jpeg", 0);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    client.add_attachment(&record_id, &ipfs_hash, &metadata);
}

#[test]
fn test_add_attachment_nonexistent_record() {
    let (env, client, _owner, _vet, _pet_id, _record_id) = setup_test_env();

    let metadata = create_test_metadata(&env, "xray.jpg", "image/jpeg", 1024000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    let result = client.add_attachment(&999u64, &ipfs_hash, &metadata);
    assert!(!result);
}

#[test]
fn test_remove_attachment_success() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Add two attachments
    let metadata1 = create_test_metadata(&env, "xray_001.jpg", "image/jpeg", 1024000);
    let ipfs_hash1 = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.add_attachment(&record_id, &ipfs_hash1, &metadata1);

    let metadata2 = create_test_metadata(&env, "blood_test.pdf", "application/pdf", 512000);
    let ipfs_hash2 = String::from_str(&env, "QmT5NvUtoM5nWFfrQdVrFtvGfKFmG7AHE8P34isapyhCxX");
    client.add_attachment(&record_id, &ipfs_hash2, &metadata2);

    // Verify both attachments exist
    assert_eq!(client.get_attachments(&record_id).len(), 2);

    // Remove first attachment
    let result = client.remove_attachment(&record_id, &0u32);
    assert!(result);

    // Verify only one attachment remains
    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.len(), 1);
    assert_eq!(
        attachments.get(0).unwrap().metadata.filename,
        String::from_str(&env, "blood_test.pdf")
    );
}

#[test]
fn test_remove_attachment_invalid_index() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Add one attachment
    let metadata = create_test_metadata(&env, "xray.jpg", "image/jpeg", 1024000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.add_attachment(&record_id, &ipfs_hash, &metadata);

    // Try to remove with invalid index - should return false, not panic
    let result = client.remove_attachment(&record_id, &5u32);
    assert!(!result);
}

#[test]
fn test_remove_attachment_nonexistent_record() {
    let (_env, client, _owner, _vet, _pet_id, _record_id) = setup_test_env();

    let result = client.remove_attachment(&999u64, &0u32);
    assert!(!result);
}

#[test]
fn test_get_attachment_count() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Initially no attachments
    assert_eq!(client.get_attachment_count(&record_id), 0);

    // Add first attachment
    let metadata1 = create_test_metadata(&env, "xray.jpg", "image/jpeg", 1024000);
    let ipfs_hash1 = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.add_attachment(&record_id, &ipfs_hash1, &metadata1);
    assert_eq!(client.get_attachment_count(&record_id), 1);

    // Add second attachment
    let metadata2 = create_test_metadata(&env, "report.pdf", "application/pdf", 512000);
    let ipfs_hash2 = String::from_str(&env, "QmT5NvUtoM5nWFfrQdVrFtvGfKFmG7AHE8P34isapyhCxX");
    client.add_attachment(&record_id, &ipfs_hash2, &metadata2);
    assert_eq!(client.get_attachment_count(&record_id), 2);

    // Remove one attachment
    client.remove_attachment(&record_id, &0u32);
    assert_eq!(client.get_attachment_count(&record_id), 1);
}

#[test]
fn test_get_attachment_count_nonexistent_record() {
    let (_env, client, _owner, _vet, _pet_id, _record_id) = setup_test_env();

    assert_eq!(client.get_attachment_count(&999u64), 0);
}

#[test]
fn test_attachment_with_various_file_types() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Test various medical file types
    let file_types = [
        ("xray.jpg", "image/jpeg"),
        ("scan.png", "image/png"),
        ("report.pdf", "application/pdf"),
        ("results.xml", "application/xml"),
        ("data.json", "application/json"),
        ("image.dicom", "application/dicom"),
    ];

    for (i, (filename, file_type)) in file_types.iter().enumerate() {
        let metadata = create_test_metadata(&env, filename, file_type, 1024000);
        let hash_str = if i == 0 {
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"
        } else if i == 1 {
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdH"
        } else if i == 2 {
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdP"
        } else if i == 3 {
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdJ"
        } else if i == 4 {
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdK"
        } else {
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdL"
        };
        let ipfs_hash = String::from_str(&env, hash_str);
        client.add_attachment(&record_id, &ipfs_hash, &metadata);
    }

    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.len(), 6);
}

#[test]
fn test_attachment_with_large_file_size() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Test with a large file size (100MB)
    let metadata = create_test_metadata(&env, "large_scan.dicom", "application/dicom", 104857600);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");

    let result = client.add_attachment(&record_id, &ipfs_hash, &metadata);
    assert!(result);

    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.get(0).unwrap().metadata.size, 104857600);
}

#[test]
fn test_medical_record_with_attachments_integration() {
    let (env, client, _owner, vet, pet_id, _record_id) = setup_test_env();

    // Create a new medical record
    let record_id = client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "Fracture diagnosis"),
        &String::from_str(&env, "Cast applied"),
        &soroban_sdk::Vec::new(&env),
        &String::from_str(&env, "Follow-up in 6 weeks"),
    );

    // Add X-ray images
    let xray1_metadata = create_test_metadata(&env, "xray_front.jpg", "image/jpeg", 2048000);
    let xray1_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdM");
    client.add_attachment(&record_id, &xray1_hash, &xray1_metadata);

    let xray2_metadata = create_test_metadata(&env, "xray_side.jpg", "image/jpeg", 2048000);
    let xray2_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdN");
    client.add_attachment(&record_id, &xray2_hash, &xray2_metadata);

    // Add medical report
    let report_metadata =
        create_test_metadata(&env, "diagnosis_report.pdf", "application/pdf", 512000);
    let report_hash = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdQ");
    client.add_attachment(&record_id, &report_hash, &report_metadata);

    // Verify the complete medical record
    let record = client.get_medical_record(&record_id).unwrap();
    assert_eq!(
        record.diagnosis,
        String::from_str(&env, "Fracture diagnosis")
    );
    assert_eq!(record.attachment_hashes.len(), 3);

    // Verify attachments can be retrieved
    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.len(), 3);
    assert_eq!(
        attachments.get(0).unwrap().metadata.filename,
        String::from_str(&env, "xray_front.jpg")
    );
    assert_eq!(
        attachments.get(1).unwrap().metadata.filename,
        String::from_str(&env, "xray_side.jpg")
    );
    assert_eq!(
        attachments.get(2).unwrap().metadata.filename,
        String::from_str(&env, "diagnosis_report.pdf")
    );
}

#[test]
fn test_multiple_records_with_attachments() {
    let (env, client, _owner, vet, pet_id, _record_id) = setup_test_env();

    // Create first record with attachments
    let record1_id = client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "Checkup 1"),
        &String::from_str(&env, "Healthy"),
        &soroban_sdk::Vec::new(&env),
        &String::from_str(&env, "All good"),
    );
    let metadata1 = create_test_metadata(&env, "checkup1.pdf", "application/pdf", 512000);
    client.add_attachment(
        &record1_id,
        &String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdH"),
        &metadata1,
    );

    // Create second record with attachments
    let record2_id = client.add_medical_record(
        &pet_id,
        &vet,
        &String::from_str(&env, "Checkup 2"),
        &String::from_str(&env, "Healthy"),
        &soroban_sdk::Vec::new(&env),
        &String::from_str(&env, "All good"),
    );
    let metadata2 = create_test_metadata(&env, "checkup2.pdf", "application/pdf", 512000);
    client.add_attachment(
        &record2_id,
        &String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdP"),
        &metadata2,
    );

    // Verify each record has its own attachments
    assert_eq!(client.get_attachment_count(&record1_id), 1);
    assert_eq!(client.get_attachment_count(&record2_id), 1);

    let attachments1 = client.get_attachments(&record1_id);
    let attachments2 = client.get_attachments(&record2_id);

    assert_eq!(
        attachments1.get(0).unwrap().metadata.filename,
        String::from_str(&env, "checkup1.pdf")
    );
    assert_eq!(
        attachments2.get(0).unwrap().metadata.filename,
        String::from_str(&env, "checkup2.pdf")
    );
}

#[test]
fn test_attachment_timestamp_tracking() {
    let (env, client, _owner, _vet, _pet_id, record_id) = setup_test_env();

    // Set initial timestamp
    env.ledger().with_mut(|l| l.timestamp = 1000);

    let metadata1 = create_test_metadata(&env, "file1.jpg", "image/jpeg", 1024000);
    client.add_attachment(
        &record_id,
        &String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdH"),
        &metadata1,
    );

    // Advance time
    env.ledger().with_mut(|l| l.timestamp = 2000);

    let metadata2 = create_test_metadata(&env, "file2.jpg", "image/jpeg", 1024000);
    client.add_attachment(
        &record_id,
        &String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdP"),
        &metadata2,
    );

    // Verify timestamps are different
    let attachments = client.get_attachments(&record_id);
    assert_eq!(attachments.get(0).unwrap().metadata.uploaded_date, 1000);
    assert_eq!(attachments.get(1).unwrap().metadata.uploaded_date, 2000);
}
