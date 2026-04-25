use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_test_env() -> (Env, Address, Address, u64, soroban_sdk::Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);

    let contract_id = env.register_contract(None, PetChainContract);
    let client = PetChainContractClient::new(&env, &contract_id);

    client.init_admin(&admin);

    let pet_id = client.register_pet(
        &owner,
        &String::from_str(&env, "Buddy"),
        &String::from_str(&env, "2020-01-01"),
        &Gender::Male,
        &Species::Dog,
        &String::from_str(&env, "Golden Retriever"),
        &String::from_str(&env, "Golden"),
        &30,
        &None,
        &PrivacyLevel::Public,
    );

    (env, owner, admin, pet_id, contract_id)
}

#[test]
fn test_add_behavior_record() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    let record_id = client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &5,
        &String::from_str(&env, "Learning to sit on command"),
    );

    assert_eq!(record_id, 1);

    let history = client.get_behavior_history(&pet_id);
    assert_eq!(history.len(), 1);
    assert_eq!(
        history.get(0).unwrap().behavior_type,
        BehaviorType::Training
    );
    assert_eq!(history.get(0).unwrap().severity, 5);
}

#[test]
fn test_add_multiple_behavior_records() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Aggression,
        &7,
        &String::from_str(&env, "Barking at strangers"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Anxiety,
        &6,
        &String::from_str(&env, "Separation anxiety"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Socialization,
        &3,
        &String::from_str(&env, "Playing with other dogs"),
    );

    let history = client.get_behavior_history(&pet_id);
    assert_eq!(history.len(), 3);
}

#[test]
#[should_panic]
fn test_invalid_severity() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Aggression,
        &15,
        &String::from_str(&env, "Invalid severity"),
    );
}

#[test]
fn test_get_behavior_by_type() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &5,
        &String::from_str(&env, "Sit command"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Aggression,
        &7,
        &String::from_str(&env, "Barking"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &4,
        &String::from_str(&env, "Stay command"),
    );

    let training_records = client.get_behavior_by_type(&pet_id, &BehaviorType::Training);
    assert_eq!(training_records.len(), 2);

    let aggression_records = client.get_behavior_by_type(&pet_id, &BehaviorType::Aggression);
    assert_eq!(aggression_records.len(), 1);
}

#[test]
fn test_add_training_milestone() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    let milestone_id = client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Basic Obedience"),
        &String::from_str(&env, "Completed sit, stay, come commands"),
    );

    assert_eq!(milestone_id, 1);

    let milestones = client.get_training_milestones(&pet_id);
    assert_eq!(milestones.len(), 1);
    assert_eq!(milestones.get(0).unwrap().achieved, false);
}

#[test]
fn test_mark_milestone_achieved() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    let milestone_id = client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Potty Training"),
        &String::from_str(&env, "No accidents for 2 weeks"),
    );

    let result = client.mark_milestone_achieved(&milestone_id);
    assert!(result);

    let milestones = client.get_training_milestones(&pet_id);
    assert_eq!(milestones.get(0).unwrap().achieved, true);
    assert!(milestones.get(0).unwrap().achieved_at.is_some());
}

#[test]
fn test_multiple_training_milestones() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Basic Commands"),
        &String::from_str(&env, "Sit, stay, come"),
    );

    client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Advanced Commands"),
        &String::from_str(&env, "Heel, roll over"),
    );

    client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Socialization"),
        &String::from_str(&env, "Comfortable with other dogs"),
    );

    let milestones = client.get_training_milestones(&pet_id);
    assert_eq!(milestones.len(), 3);
}

#[test]
fn test_behavior_improvements_tracking() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    // Track improvement over time
    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Anxiety,
        &8,
        &String::from_str(&env, "High anxiety during storms"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Anxiety,
        &6,
        &String::from_str(&env, "Moderate anxiety, showing improvement"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Anxiety,
        &3,
        &String::from_str(&env, "Much calmer, training working"),
    );

    let improvements = client.get_behavior_improvements(&pet_id, &BehaviorType::Anxiety);
    assert_eq!(improvements.len(), 3);

    // Verify severity is decreasing (improvement)
    assert_eq!(improvements.get(0).unwrap().severity, 8);
    assert_eq!(improvements.get(1).unwrap().severity, 6);
    assert_eq!(improvements.get(2).unwrap().severity, 3);
}

#[test]
fn test_comprehensive_behavior_tracking() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    // Add various behavior records
    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Aggression,
        &5,
        &String::from_str(&env, "Initial assessment"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &7,
        &String::from_str(&env, "Started training program"),
    );

    // Add training milestones
    let milestone1 = client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Week 1 Goals"),
        &String::from_str(&env, "Basic commands"),
    );

    let _milestone2 = client.add_training_milestone(
        &pet_id,
        &String::from_str(&env, "Week 4 Goals"),
        &String::from_str(&env, "Advanced training"),
    );

    // Mark first milestone as achieved
    client.mark_milestone_achieved(&milestone1);

    // Add follow-up behavior record
    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Aggression,
        &2,
        &String::from_str(&env, "Significant improvement after training"),
    );

    // Verify all data
    let history = client.get_behavior_history(&pet_id);
    assert_eq!(history.len(), 3);

    let milestones = client.get_training_milestones(&pet_id);
    assert_eq!(milestones.len(), 2);
    assert_eq!(milestones.get(0).unwrap().achieved, true);
    assert_eq!(milestones.get(1).unwrap().achieved, false);

    let aggression_records = client.get_behavior_by_type(&pet_id, &BehaviorType::Aggression);
    assert_eq!(aggression_records.len(), 2);
}

#[test]
fn test_empty_behavior_history() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    let history = client.get_behavior_history(&pet_id);
    assert_eq!(history.len(), 0);
}

#[test]
fn test_empty_training_milestones() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    let milestones = client.get_training_milestones(&pet_id);
    assert_eq!(milestones.len(), 0);
}

#[test]
fn test_all_behavior_types() {
    let (env, _owner, _admin, pet_id, contract_id) = setup_test_env();
    let client = PetChainContractClient::new(&env, &contract_id);

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Aggression,
        &5,
        &String::from_str(&env, "Aggression test"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Anxiety,
        &6,
        &String::from_str(&env, "Anxiety test"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Training,
        &7,
        &String::from_str(&env, "Training test"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Socialization,
        &4,
        &String::from_str(&env, "Socialization test"),
    );

    client.add_behavior_record(
        &pet_id,
        &BehaviorType::Other,
        &3,
        &String::from_str(&env, "Other behavior"),
    );

    let history = client.get_behavior_history(&pet_id);
    assert_eq!(history.len(), 5);
}
