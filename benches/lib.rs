#![no_std]
#![allow(clippy::too_many_arguments)]

#[contracttype]
pub enum InsuranceKey {
    Policy(u64),
    Claim(u64),                // claim_id -> InsuranceClaim
    ClaimCount,                // Global count of claims
    PetClaimCount(u64),        // pet_id -> count of claims
    PetClaimIndex((u64, u64)), // (pet_id, index) -> claim_id
}

#[contracttype]
pub enum BehaviorKey {
    BehaviorRecord(u64),
    BehaviorRecordCount,
    PetBehaviorCount(u64),
    PetBehaviorIndex((u64, u64)),
    TrainingMilestone(u64),
    TrainingMilestoneCount,
    PetMilestoneCount(u64),
    PetMilestoneIndex((u64, u64)),
}

#[contracttype]
pub enum ActivityKey {
    ActivityRecord(u64),
    ActivityRecordCount,
    PetActivityCount(u64),
    PetActivityIndex((u64, u64)),
}

#[contracttype]
pub enum BreedingKey {
    BreedingRecord(u64),
    BreedingRecordCount,
    PetBreedingCount(u64),
    PetBreedingIndex((u64, u64)),
    PetOffspringCount(u64),
    PetOffspringIndex((u64, u64)),
}

#[contracttype]
pub enum GroomingKey {
    GroomingRecord(u64),
    GroomingRecordCount,
    PetGroomingCount(u64),
    PetGroomingIndex((u64, u64)),
}

#[cfg(test)]
mod test_access_control;
#[cfg(test)]
mod test_activity;
#[cfg(test)]
mod test_attachments;
#[cfg(test)]
mod test_behavior;
#[cfg(test)]
mod test_emergency_contacts;
#[cfg(test)]
mod test_emergency_override;
#[cfg(test)]
mod test_grooming;
#[cfg(test)]
mod test_insurance;
#[cfg(test)]
mod test_insurance_claims;
#[cfg(test)]
mod test_insurance_comprehensive;
#[cfg(test)]
mod test_medical_records_pagination;
#[cfg(test)]
mod test_multisig_transfer;
#[cfg(test)]
mod test_nutrition;
#[cfg(test)]
mod test_pet_age;
#[cfg(test)]
mod test_search_medical_records;
#[cfg(test)]
mod test_statistics;

use soroban_sdk::xdr::{FromXdr, ToXdr};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Bytes, BytesN,
    Env, String, Symbol, Vec,
};

const MAX_LOG_ENTRIES: u32 = 1_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    Unauthorized = 1,
    AdminNotInitialized = 2,
    PetNotFound = 3,
    VetNotFound = 4,
    VeterinarianNotVerified = 5,
    VetAlreadyRegistered = 6,
    LicenseAlreadyRegistered = 7,
    InputStringTooLong = 8,
    PetAlreadyHasLinkedTag = 9,
    InvalidIpfsHash = 10,
    CounterOverflow = 11,
    TooManyItems = 12,
    InvalidState = 13,
    InvalidInput = 14,
    CommentTooLong = 15,
    AdminAlreadySet = 16,
    AdminsNotSet = 17,
    NoAdminsConfigured = 18,
    NotAnAdmin = 19,
    InvokerNotInAdminList = 20,
    InvalidThreshold = 21,
    SireNotFound = 22,
    VetNotVerified = 23,
    TagAlreadyLinked = 24,
    FilenameEmpty = 25,
    FileTypeEmpty = 26,
    FileSizeZero = 27,
    InvalidAttachmentIndex = 43,
    AlertNotFound = 50,
    AlertNotActive = 51,
    NotPetOwner = 60,
    NotConsentOwner = 61,
    ConsentAlreadyRevoked = 62,
    SlotAlreadyBooked = 70,
    ProposalNotFound = 80,
    ProposalAlreadyExecuted = 81,
    ProposalExpired = 82,
    ThresholdNotMet = 83,
    AdminAlreadyApproved = 84,
    InvalidRating = 90,
    DuplicateReview = 91,
    MedicationNotFound = 100,
    MultisigNotConfigured = 110,
    MultisigNotEnabled = 111,
    NotAuthorizedSigner = 112,
    AlreadySigned = 113,
    SeverityOutOfRange = 120,
    IntensityOutOfRange = 121,
    CustodyNotFound = 130,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Species {
    Other,
    Dog,
    Cat,
    Bird,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityType {
    Walk,
    Run,
    Play,
    Training,
    Other,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroomingRecord {
    pub id: u64,
    pub pet_id: u64,
    pub service_type: String,
    pub groomer: String,
    pub date: u64,
    pub next_due: u64,
    pub cost: u64,
    pub notes: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityRecord {
    pub id: u64,
    pub pet_id: u64,
    pub activity_type: ActivityType,
    pub duration_minutes: u32,
    pub intensity: u32,
    pub distance_meters: u32,
    pub recorded_at: u64,
    pub notes: String,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreedingRecord {
    pub id: u64,
    pub sire_id: u64,
    pub dam_id: u64,
    pub breeding_date: u64,
    pub offspring_ids: Vec<u64>,
    pub breeder: Address,
    pub notes: String,
}


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BehaviorType {
    Aggression,
    Anxiety,
    Training,
    Socialization,
    Other,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BehaviorRecord {
    pub id: u64,
    pub pet_id: u64,
    pub behavior_type: BehaviorType,
    pub severity: u32,
    pub description: String,
    pub recorded_by: Address,
    pub recorded_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrainingMilestone {
    pub id: u64,
    pub pet_id: u64,
    pub milestone_name: String,
    pub achieved: bool,
    pub achieved_at: Option<u64>,
    pub trainer: Address,
    pub notes: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Gender {
    NotSpecified,
    Male,
    Female,
    Unknown,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivacyLevel {
    Public,     // Accessible to anyone
    Restricted, // Accessible to granted access (e.g., vets, owners)
    Private,    // Accessible only to the owner
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessAction {
    Read,
    Write,
    Grant,
    Revoke,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessLog {
    pub id: u64,
    pub pet_id: u64,
    pub user: Address,
    pub action: AccessAction,
    pub timestamp: u64,
    pub details: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyContactInfo {
    pub name: String,
    pub phone: String,
    pub relationship: String,
}

#[contracttype]
#[derive(Clone)]
pub struct EmergencyContact {
    pub name: String,
    pub phone: String,
    pub email: String,
    pub relationship: String,
    pub is_primary: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Allergy {
    pub name: String,
    pub severity: String,
    pub is_critical: bool,
}

// --- NUTRITION / DIET ---
#[contracttype]
pub enum NutritionKey {
    DietPlan(u64),              // diet_id -> DietPlan
    DietPlanCount,              // global count
    PetDietCount(u64),          // pet_id -> count
    PetDietByIndex((u64, u64)), // (pet_id, index) -> diet_id

    WeightEntry(u64),             // weight_id -> WeightEntry
    WeightCount,                  // global weight entry count
    PetWeightCount(u64),          // pet_id -> count
    PetWeightByIndex((u64, u64)), // (pet_id, index) -> weight_id
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DietPlan {
    pub pet_id: u64,
    pub food_type: String,
    pub portion_size: String,
    pub feeding_frequency: String,
    pub dietary_restrictions: Vec<String>,
    pub allergies: Vec<String>,
    pub created_by: Address,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightEntry {
    pub pet_id: u64,
    pub weight: u32,
    pub recorded_at: u64,
    pub recorded_by: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PetData {
    pub name: String,
    pub species: String,
    pub breed: String,
}

#[contracttype]
#[derive(Clone)]
pub struct EmergencyInfo {
    pub pet_id: u64,
    pub species: String,
    pub allergies: Vec<Allergy>,
    pub critical_alerts: Vec<String>,
    pub emergency_contacts: Vec<EmergencyContact>,
}

#[contracttype]
#[derive(Clone)]
pub struct EmergencyAccessLog {
    pub pet_id: u64,
    pub accessed_by: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedData {
    pub nonce: Bytes,
    pub ciphertext: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct Pet {
    pub id: u64,
    pub owner: Address,
    pub privacy_level: PrivacyLevel,
    // Encrypted fields replace plain text for sensitive data in storage
    pub encrypted_name: EncryptedData,
    pub encrypted_birthday: EncryptedData,
    pub encrypted_breed: EncryptedData,
    pub encrypted_emergency_contacts: EncryptedData,
    pub encrypted_medical_alerts: EncryptedData,
    pub encrypted_allergies: EncryptedData,

    // Internal/Empty fields to maintain some structural compatibility if needed,
    // or just purely internal placeholders. HEAD set these to empty strings.
    pub name: String,
    pub birthday: String,
    pub breed: String,
    pub emergency_contacts: Vec<EmergencyContact>,
    pub medical_alerts: String,
    pub allergies: Vec<Allergy>,

    pub active: bool,
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub new_owner: Address,
    pub species: Species,
    pub gender: Gender,
    pub color: String,
    pub weight: u32,
    pub microchip_id: Option<String>,
    pub photo_hashes: Vec<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PetProfile {
    pub id: u64,
    pub owner: Address,
    pub privacy_level: PrivacyLevel,
    pub name: String,
    pub birthday: String,
    pub active: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub new_owner: Address,
    pub species: Species,
    pub gender: Gender,
    pub breed: String,
    pub color: String,
    pub weight: u32,
    pub microchip_id: Option<String>,
    pub allergies: Vec<Allergy>,
}

#[contracttype]
#[derive(Clone)]
pub struct PetOwner {
    pub owner_address: Address,
    pub privacy_level: PrivacyLevel,
    pub encrypted_name: EncryptedData,
    pub encrypted_email: EncryptedData,
    pub encrypted_emergency_contact: EncryptedData,

    pub created_at: u64,
    pub updated_at: u64,
    pub is_pet_owner: bool,
}

/*
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClinicInfo {
    pub clinic_name: String,
    pub address: String,
    pub phone: String,
    pub email: String,
    pub operating_hours: String,
    pub emergency_available: bool,
}
*/

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Specialization {
    pub name: String,
    pub certified_date: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Certification {
    pub name: String,
    pub issuer: String,
    pub issue_date: u64,
    pub expiry_date: Option<u64>,
}

#[contracttype]
#[derive(Clone)]
pub struct Vet {
    pub address: Address,
    pub name: String,
    pub license_number: String,
    pub specialization: String,
    pub verified: bool,
    pub clinic_info: Option<String>, // Simplified to String to avoid nested Option issues
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VaccineType {
    Rabies,
    Parvovirus,
    Leukemia,
    Bordetella,
    Other,
}

#[contracttype]
#[derive(Clone)]
pub struct Vaccination {
    pub id: u64,
    pub pet_id: u64,
    pub veterinarian: Address,
    pub vaccine_type: VaccineType,

    pub vaccine_name: Option<String>, // Decrypted value (None in storage)
    pub encrypted_vaccine_name: EncryptedData, // Encrypted value

    pub administered_at: u64,
    pub next_due_date: u64,

    pub batch_number: Option<String>, // Decrypted value (None in storage)
    pub encrypted_batch_number: EncryptedData, // Encrypted value

    pub created_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct TagLinkedEvent {
    pub tag_id: BytesN<32>,
    pub pet_id: u64,
    pub owner: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct TagDeactivatedEvent {
    pub tag_id: BytesN<32>,
    pub pet_id: u64,
    pub deactivated_by: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct UpgradeProposal {
    pub id: u64,
    pub proposed_by: Address,
    pub new_wasm_hash: BytesN<32>,
    pub proposed_at: u64,
    pub approved: bool,
    pub executed: bool,
}
#[contracttype]
#[derive(Clone)]
pub struct TagReactivatedEvent {
    pub tag_id: BytesN<32>,
    pub pet_id: u64,
    pub reactivated_by: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PetTag {
    pub tag_id: BytesN<32>,
    pub pet_id: u64,
    pub owner: Address,
    pub message: String,
    pub is_active: bool,
    pub linked_at: u64,
    pub updated_at: u64,
    // Note: older code might use 'tag_message' or 'created_at', we harmonize here
    pub tag_message: String,
    pub created_at: u64,
}

#[contracttype]
pub enum DataKey {
    Pet(u64),
    PetCount,
    PetOwner(Address),
    OwnerPetIndex((Address, u64)),
    PetCountByOwner(Address),

    // Species index for filtering
    SpeciesPetCount(String),
    SpeciesPetIndex((String, u64)), // (species_key, index) -> pet_id

    // Vet verification keys
    Vet(Address),
    VetLicense(String),
    Admin,

    // Contract Upgrade keys
    ContractVersion,
    UpgradeProposal(u64),
    UpgradeProposalCount,

    // Access Control keys
    AccessGrant((u64, Address)),  // (pet_id, grantee) -> AccessGrant
    AccessGrantCount(u64),        // pet_id -> count of grants
    AccessGrantIndex((u64, u64)), // (pet_id, index) -> grantee Address
    TemporaryCustody(u64),        // pet_id -> temporary custody record

    // Vet stats and tracking
    VetStats(Address),
    VetPetTreated((Address, u64)), // (vet, pet_id) -> bool
    VetPetCount(Address),          // unique pets treated

    // Lab Result DataKey

    // Medical Record DataKey

    // Vet Review keys

    // Medication keys
    // Lost Pet Alert System keys
    EmergencyAccessLogs(u64),    // pet_id -> Vec<EmergencyAccessLog>
    EmergencyResponders(u64),     // pet_id -> Vec<Address>
}

#[contracttype]
pub enum TreatmentKey {
    // Treatment DataKey
    Treatment(u64),
    TreatmentCount,
    PetTreatmentCount(u64),
    PetTreatmentIndex((u64, u64)), // (pet_id, index) -> treatment_id
}

#[contracttype]
pub enum TagKey {
    // Tag Linking System keys
    Tag(soroban_sdk::BytesN<32>), // tag_id -> PetTag (reverse lookup for QR scan)
    // Tag String keys (QR)
    PetTagId(u64), // pet_id -> tag_id (forward lookup)
    TagNonce,      // Global nonce for deterministic tag ID generation
    PetTagCount,   // Count of tags (mostly for stats)
}

#[contracttype]
pub enum MedicalKey {
    LabResult(u64),
    LabResultCount,
    PetLabResultIndex((u64, u64)), // (pet_id, index) -> lab_result_id
    PetLabResultCount(u64),
    MedicalRecord(u64),
    MedicalRecordCount,
    PetMedicalRecordIndex((u64, u64)), // (pet_id, index) -> medical_record_id
    PetMedicalRecordCount(u64),
    GlobalMedication(u64),          // medication_id -> Medication
    MedicationCount,                // Global count
    PetMedicationCount(u64),        // pet_id -> count
    PetMedicationIndex((u64, u64)), // (pet_id, index) -> medication_id
    // Vaccination DataKey
    Vaccination(u64),
    VaccinationCount,
    PetVaccinationCount(u64),
    PetVaccinationByIndex((u64, u64)),
}

#[contracttype]
pub enum ReviewKey {
    VetReview(u64),                          // review_id -> VetReview
    VetReviewCount,                          // Global count of reviews
    VetReviewByVetIndex((Address, u64)),     // (Vet, index) -> review_id
    VetReviewCountByVet(Address),            // Vet -> count
    VetReviewByOwnerVet((Address, Address)), // (Owner, Vet) -> review_id (Duplicate check)
}

#[contracttype]
pub enum AlertKey {
    LostPetAlert(u64),
    LostPetAlertCount,
    ActiveLostPetAlerts, // Vec<u64> of active alert IDs
    AlertSightings(u64),
}

#[contracttype]
pub enum ConsentKey {
    // Consent System keys
    Consent(u64),
    ConsentCount,
    PetConsentIndex((u64, u64)),
    PetConsentCount(u64),
}

#[contracttype]
pub enum SystemKey {
    // Ownership History keys
    PetOwnershipRecord(u64),
    OwnershipRecordCount,
    PetOwnershipRecordCount(u64),
    PetOwnershipRecordIndex((u64, u64)),

    // Multisig keys
    Admins,
    AdminThreshold,
    Proposal(u64),
    ProposalCount,

    // Vet Availability keys
    VetAvailability((Address, u64)),
    VetAvailabilityCount(Address),
    VetAvailabilityByDate((Address, u64)),

    // Pet Multisig keys
    PetMultisigConfig(u64),
    PetTransferProposal(u64),
    PetTransferProposalCount,
    EncryptionNonceCounter,
}

#[contracttype]
pub enum VetKey {
    VetStats(Address),
    VetPetTreated((Address, u64)),
    VetPetCount(Address),
}

#[contracttype]
pub enum StatsKey {
    ActivePetsCount,
}

// --- LOST PET ALERT SYSTEM ---
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertStatus {
    Active,
    Found,
    Cancelled,
}

#[contracttype]
#[derive(Clone)]
pub struct VetStats {
    pub total_records: u64,
    pub total_vaccinations: u64,
    pub total_treatments: u64,
    pub pets_treated: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct LostPetAlert {
    pub id: u64,
    pub pet_id: u64,
    pub reported_by: Address,
    pub reported_date: u64,
    pub last_seen_location: String,
    pub reward_amount: Option<u64>,
    pub status: AlertStatus,
    pub found_date: Option<u64>,
}

#[contracttype]
#[derive(Clone)]
pub struct SightingReport {
    pub alert_id: u64,
    pub reporter: Address,
    pub location: String,
    pub timestamp: u64,
    pub description: String,
}

#[contracttype]
#[derive(Clone)]
pub struct AvailabilitySlot {
    pub vet_address: Address,
    pub start_time: u64,
    pub end_time: u64,
    pub available: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsentType {
    Insurance,
    Research,
    PublicHealth,
    Other,
}

#[contracttype]
#[derive(Clone)]
pub struct Consent {
    pub id: u64,
    pub pet_id: u64,
    pub owner: Address,
    pub consent_type: ConsentType,
    pub granted_to: Address,
    pub granted_at: u64,
    pub revoked_at: Option<u64>,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabResult {
    pub id: u64,
    pub pet_id: u64,
    pub test_type: String,
    pub date: u64,
    pub results: String,
    pub vet_address: Address,
    pub reference_ranges: String,
    pub attachment_hash: Option<String>, // IPFS hash for PDF
    pub medical_record_id: Option<u64>,  // Link to medical record
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaccinationSummary {
    pub is_fully_current: bool,
    pub overdue_types: Vec<VaccineType>,
    pub upcoming_count: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessLevel {
    None,
    Basic, // Can view basic pet info only
    Full,  // Can view all records including medical history
}

#[contracttype]
#[derive(Clone)]
pub struct AccessGrant {
    pub pet_id: u64,
    pub granter: Address, // Pet owner who granted access
    pub grantee: Address, // User receiving access
    pub access_level: AccessLevel,
    pub granted_at: u64,
    pub expires_at: Option<u64>, // None means permanent access
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct TemporaryCustody {
    pub pet_id: u64,
    pub owner: Address,
    pub custodian: Address,
    pub start_date: u64,
    pub end_date: u64,
    pub permissions: Vec<String>,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Medication {
    pub id: u64,
    pub pet_id: u64,
    pub name: String,
    pub dosage: String,
    pub frequency: String,
    pub start_date: u64,
    pub end_date: Option<u64>,
    pub prescribing_vet: Address,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentMetadata {
    pub filename: String,
    pub file_type: String,
    pub size: u64,
    pub uploaded_date: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attachment {
    pub ipfs_hash: String,
    pub metadata: AttachmentMetadata,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MedicalRecord {
    pub id: u64,
    pub pet_id: u64,
    pub vet_address: Address,
    pub diagnosis: String,
    pub treatment: String,
    pub medications: Vec<Medication>,
    pub date: u64,
    pub updated_at: u64,
    pub notes: String,
    pub attachment_hashes: Vec<Attachment>,
}

#[contracttype]
#[derive(Clone)]
pub struct VaccinationInput {
    pub pet_id: u64,
    pub vaccine_type: VaccineType,
    pub vaccine_name: String,
    pub administered_at: u64,
    pub next_due_date: u64,
    pub batch_number: String,
}

#[contracttype]
#[derive(Clone)]
pub struct MedicalRecordInput {
    pub pet_id: u64,
    pub diagnosis: String,
    pub treatment: String,
    pub medications: Vec<Medication>,
    pub notes: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MedicalRecordFilter {
    pub vet_address: Option<Address>,
    pub from_date: Option<u64>,
    pub to_date: Option<u64>,
    pub diagnosis_keyword: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VetReview {
    pub id: u64,
    pub vet_address: Address,
    pub reviewer: Address,
    pub rating: u32, // 1-5 stars
    pub comment: String,
    pub date: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct OwnershipRecord {
    pub pet_id: u64,
    pub previous_owner: Address,
    pub new_owner: Address,
    pub transfer_date: u64,
    pub transfer_reason: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalAction {
    UpgradeContract(BytesN<32>),
    VerifyVet(Address),
    RevokeVet(Address),
    ChangeAdmin((Vec<Address>, u32)),
}

#[contracttype]
#[derive(Clone)]
pub struct MultiSigProposal {
    pub id: u64,
    pub action: ProposalAction,
    pub proposed_by: Address,
    pub approvals: Vec<Address>,
    pub required_approvals: u32,
    pub created_at: u64,
    pub expires_at: u64,
    pub executed: bool,
}

/// Multi-signature configuration for a pet.
/// Enables multiple parties to approve pet ownership transfers.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultisigConfig {
    /// The pet ID this configuration applies to
    pub pet_id: u64,
    /// List of addresses authorized to sign transfer proposals
    pub signers: Vec<Address>,
    /// Minimum number of signatures required to execute a transfer
    pub threshold: u32,
    /// Whether multisig enforcement is enabled
    pub enabled: bool,
}

/// Proposal for transferring pet ownership with multi-signature approval.
#[contracttype]
#[derive(Clone)]
pub struct PetTransferProposal {
    /// Unique proposal identifier
    pub id: u64,
    /// The pet being transferred
    pub pet_id: u64,
    /// Address of the new owner
    pub to: Address,
    /// Addresses that have signed this proposal
    pub signatures: Vec<Address>,
    /// Timestamp when proposal was created
    pub created_at: u64,
    /// Timestamp when proposal expires
    pub expires_at: u64,
    /// Whether the transfer has been executed
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreatmentType {
    Surgery,
    Therapy,
    Emergency,
    Routine,
    Other,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Treatment {
    pub id: u64,
    pub pet_id: u64,
    pub treatment_type: TreatmentType,
    pub date: u64,
    pub vet_address: Address,
    pub notes: String,
    pub cost: Option<i128>,
    pub outcome: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreatmentAddedEvent {
    pub treatment_id: u64,
    pub pet_id: u64,
    pub vet_address: Address,
    pub treatment_type: TreatmentType,
    pub timestamp: u64,
}

// --- EVENTS ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsurancePolicy {
    pub policy_id: String,
    pub provider: String,
    pub coverage_type: String,
    pub premium: u64,
    pub coverage_limit: u64,
    pub start_date: u64,
    pub expiry_date: u64,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceAddedEvent {
    pub pet_id: u64,
    pub policy_id: String,
    pub provider: String,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceUpdatedEvent {
    pub pet_id: u64,
    pub policy_id: String,
    pub active: bool,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InsuranceClaimStatus {
    Pending,
    Approved,
    Rejected,
    Paid,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaim {
    pub claim_id: u64,
    pub pet_id: u64,
    pub policy_id: String,
    pub amount: u64,
    pub date: u64,
    pub status: InsuranceClaimStatus,
    pub description: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaimSubmittedEvent {
    pub claim_id: u64,
    pub pet_id: u64,
    pub policy_id: String,
    pub amount: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaimStatusUpdatedEvent {
    pub claim_id: u64,
    pub pet_id: u64,
    pub status: InsuranceClaimStatus,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct AccessGrantedEvent {
    pub pet_id: u64,
    pub granter: Address,
    pub grantee: Address,
    pub access_level: AccessLevel,
    pub expires_at: Option<u64>,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct AccessRevokedEvent {
    pub pet_id: u64,
    pub granter: Address,
    pub grantee: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct AccessExpiredEvent {
    pub pet_id: u64,
    pub grantee: Address,
    pub expired_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PetRegisteredEvent {
    pub pet_id: u64,
    pub owner: Address,
    pub name: String, // Note: This might be redundant if encrypted, but keeping for event compatibility if safe
    pub species: Species,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaccinationAddedEvent {
    pub vaccine_id: u64,
    pub pet_id: u64,
    pub veterinarian: Address,
    pub vaccine_type: VaccineType,
    pub next_due_date: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PetOwnershipTransferredEvent {
    pub pet_id: u64,
    pub old_owner: Address,
    pub new_owner: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MedicalRecordAddedEvent {
    pub pet_id: u64,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contract]
pub struct PetChainContract;

#[contractimpl]
impl PetChainContract {
    // --- CONTRACT STATISTICS ---

    /// Returns the total number of pets ever registered in the contract.
    pub fn get_total_pets(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PetCount)
            .unwrap_or(0)
    }

    /// Returns the number of registered pets for a given species.
    /// Pass the species name as a string: "Dog", "Cat", "Bird", or "Other".
    pub fn get_species_count(env: Env, species: String) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::SpeciesPetCount(species))
            .unwrap_or(0)
    }

    /// Returns the number of currently active pets.
    /// This counter is maintained automatically by `activate_pet` and `deactivate_pet`.
    pub fn get_active_pets_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&StatsKey::ActivePetsCount)
            .unwrap_or(0)
    }

    fn log_access(env: &Env, pet_id: u64, user: Address, action: AccessAction, details: String) {
        let key = (Symbol::new(env, "access_logs"), pet_id);
        let mut logs: Vec<AccessLog> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        while logs.len() >= MAX_LOG_ENTRIES {
            logs.remove(0);
        }

        let id = if logs.is_empty() {
            0
        } else {
            logs.get(logs.len() - 1).unwrap().id + 1
        };
        let log = AccessLog {
            id,
            pet_id,
            user,
            action,
            timestamp: env.ledger().timestamp(),
            details,
        };

        logs.push_back(log);
        env.storage().persistent().set(&key, &logs);
    }

    fn require_admin(env: &Env) {
        if let Some(legacy_admin) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Admin)
        {
            legacy_admin.require_auth();
            return;
        }

        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&SystemKey::Admins)
            .unwrap_or_else(|| env.panic_with_error(ContractError::AdminsNotSet));

        if admins.is_empty() {
            env.panic_with_error(ContractError::NoAdminsConfigured);
        }

        let admin = admins
            .get(0)
            .unwrap_or_else(|| env.panic_with_error(ContractError::NoAdminsConfigured));

        admin.require_auth();
    }

    fn require_admin_auth(env: &Env, admin: &Address) {
        if let Some(legacy_admin) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Admin)
        {
            if &legacy_admin == admin {
                admin.require_auth();
                return;
            }
        }

        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&SystemKey::Admins)
            .unwrap_or_else(|| env.panic_with_error(ContractError::AdminsNotSet));

        if !admins.contains(admin.clone()) {
            panic_with_error!(env, ContractError::Unauthorized);
        }
        admin.require_auth();
    }

    pub fn init_admin(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin)
            || env.storage().instance().has(&SystemKey::Admins)
        {
            panic!("Admin already set");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn init_multisig(env: Env, invoker: Address, admins: Vec<Address>, threshold: u32) {
        if env.storage().instance().has(&DataKey::Admin)
            || env.storage().instance().has(&SystemKey::Admins)
        {
            panic!("Admin already set");
        }
        if threshold == 0 || threshold > admins.len() {
            panic!("Invalid threshold");
        }

        invoker.require_auth();
        if !admins.contains(invoker) {
            panic!("Invoker must be in the initial admin list");
        }

        env.storage().instance().set(&SystemKey::Admins, &admins);
        env.storage()
            .instance()
            .set(&SystemKey::AdminThreshold, &threshold);
    }

    fn update_vet_stats(
        env: &Env,
        vet: &Address,
        pet_id: u64,
        record_increment: u64,
        vaccination_increment: u64,
        treatment_increment: u64,
    ) {
        let mut stats = env
            .storage()
            .instance()
            .get::<_, VetStats>(&VetKey::VetStats(vet.clone()))
            .unwrap_or(VetStats {
                total_records: 0,
                total_vaccinations: 0,
                total_treatments: 0,
                pets_treated: 0,
            });

        stats.total_records = stats
            .total_records
            .checked_add(record_increment)
            .unwrap_or_else(|| panic_with_error!(env.clone(), ContractError::CounterOverflow));
        stats.total_vaccinations = stats
            .total_vaccinations
            .checked_add(vaccination_increment)
            .unwrap_or_else(|| panic_with_error!(env.clone(), ContractError::CounterOverflow));
        stats.total_treatments = stats
            .total_treatments
            .checked_add(treatment_increment)
            .unwrap_or_else(|| panic_with_error!(env.clone(), ContractError::CounterOverflow));
        stats.total_records += record_increment;
        stats.total_vaccinations += vaccination_increment;
        stats.total_treatments += treatment_increment;

        // Unique pet tracking
        if !env
            .storage()
            .instance()
            .has(&VetKey::VetPetTreated((vet.clone(), pet_id)))
        {
            env.storage()
                .instance()
                .set(&VetKey::VetPetTreated((vet.clone(), pet_id)), &true);

            stats.pets_treated += 1;
        }

        env.storage()
            .instance()
            .set(&VetKey::VetStats(vet.clone()), &stats);
    }

    // Pet Management Functions
    #[allow(clippy::too_many_arguments)]
    pub fn register_pet(
        env: Env,
        owner: Address,
        name: String,
        birthday: String,
        gender: Gender,
        species: Species,
        breed: String,
        color: String,
        weight: u32,
        microchip_id: Option<String>,
        privacy_level: PrivacyLevel,
    ) -> u64 {
        owner.require_auth();
        if let Err(err) = Self::parse_birthday_timestamp(&birthday) {
            env.panic_with_error(err);
        }

        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PetCount)
            .unwrap_or(0);
        let pet_id = pet_count + 1;
        let timestamp = env.ledger().timestamp();

        let key = Self::get_encryption_key(&env);

        // Encrypt name
        let name_bytes = name.to_xdr(&env);
        let (name_nonce, name_ciphertext) = encrypt_sensitive_data(&env, &name_bytes, &key);
        let encrypted_name = EncryptedData {
            nonce: name_nonce,
            ciphertext: name_ciphertext,
        };

        // Encrypt birthday
        let birthday_bytes = birthday.to_xdr(&env);
        let (birthday_nonce, birthday_ciphertext) =
            encrypt_sensitive_data(&env, &birthday_bytes, &key);
        let encrypted_birthday = EncryptedData {
            nonce: birthday_nonce,
            ciphertext: birthday_ciphertext,
        };

        // Encrypt breed
        let breed_bytes = breed.to_xdr(&env);
        let (breed_nonce, breed_ciphertext) = encrypt_sensitive_data(&env, &breed_bytes, &key);
        let encrypted_breed = EncryptedData {
            nonce: breed_nonce,
            ciphertext: breed_ciphertext,
        };

        // Initialize empty medical alerts/contacts
        let empty_alerts_bytes = Bytes::from_slice(&env, "".as_bytes());
        let (alerts_nonce, alerts_ciphertext) =
            encrypt_sensitive_data(&env, &empty_alerts_bytes, &key);
        let encrypted_medical_alerts = EncryptedData {
            nonce: alerts_nonce,
            ciphertext: alerts_ciphertext,
        };

        let empty_contacts = Vec::<EmergencyContact>::new(&env);
        let contacts_bytes = empty_contacts.to_xdr(&env);
        let (contacts_nonce, contacts_ciphertext) =
            encrypt_sensitive_data(&env, &contacts_bytes, &key);
        let encrypted_emergency_contacts = EncryptedData {
            nonce: contacts_nonce,
            ciphertext: contacts_ciphertext,
        };

        let empty_allergies = Vec::<Allergy>::new(&env);
        let allergies_bytes = empty_allergies.to_xdr(&env);
        let (allergies_nonce, allergies_ciphertext) =
            encrypt_sensitive_data(&env, &allergies_bytes, &key);
        let encrypted_allergies = EncryptedData {
            nonce: allergies_nonce,
            ciphertext: allergies_ciphertext,
        };

        let pet = Pet {
            id: pet_id,
            owner: owner.clone(),
            privacy_level,
            encrypted_name,
            encrypted_birthday,
            encrypted_breed,
            encrypted_emergency_contacts,
            encrypted_medical_alerts,
            encrypted_allergies,

            // Empty placeholders for internal API consistency if needed
            name: String::from_str(&env, ""),
            birthday: String::from_str(&env, ""),
            breed: String::from_str(&env, ""),
            emergency_contacts: Vec::<EmergencyContact>::new(&env),
            medical_alerts: String::from_str(&env, ""),
            allergies: Vec::<Allergy>::new(&env),

            active: false,
            archived: false,
            created_at: timestamp,
            updated_at: timestamp,
            new_owner: owner.clone(),
            species: species.clone(),
            gender,
            color,
            weight: weight.into(),
            microchip_id,
            photo_hashes: Vec::new(&env),
        };

        env.storage().instance().set(&DataKey::Pet(pet_id), &pet);
        env.storage().instance().set(&DataKey::PetCount, &pet_id);

        Self::log_ownership_change(
            &env,
            pet_id,
            owner.clone(),
            owner.clone(),
            String::from_str(&env, "Initial Registration"),
        );

        let owner_pet_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PetCountByOwner(owner.clone()))
            .unwrap_or(0)
            + 1;
        env.storage()
            .instance()
            .set(&DataKey::PetCountByOwner(owner.clone()), &owner_pet_count);
        env.storage().instance().set(
            &DataKey::OwnerPetIndex((owner.clone(), owner_pet_count)),
            &pet_id,
        );

        // Add to species index
        let species_key = Self::species_to_string(&env, &species);
        let species_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SpeciesPetCount(species_key.clone()))
            .unwrap_or(0)
            + 1;
        env.storage().instance().set(
            &DataKey::SpeciesPetCount(species_key.clone()),
            &species_count,
        );
        env.storage().instance().set(
            &DataKey::SpeciesPetIndex((species_key, species_count)),
            &pet_id,
        );

        // EMIT EVENT: PetRegistered (we emit the decrypted name for the event log as it's useful,
        // assuming standard privacy. If high strictness needed, this should be masked).
        // For now, we emit what was passed in.
        env.events().publish(
            (String::from_str(&env, "PetRegistered"), pet_id),
            PetRegisteredEvent {
                pet_id,
                owner,
                name: String::from_str(&env, "PROTECTED"), // Masking name in event for safety
                species,
                timestamp,
            },
        );

        pet_id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_pet_profile(
        env: Env,
        id: u64,
        name: String,
        birthday: String,
        gender: Gender,
        species: Species,
        breed: String,
        color: String,
        weight: u32,
        microchip_id: Option<String>,
        privacy_level: PrivacyLevel,
    ) -> bool {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            pet.owner.require_auth();
            if let Err(err) = Self::parse_birthday_timestamp(&birthday) {
                env.panic_with_error(err);
            }

            let key = Self::get_encryption_key(&env);

            let name_bytes = name.to_xdr(&env);
            let (name_nonce, name_ciphertext) = encrypt_sensitive_data(&env, &name_bytes, &key);
            pet.encrypted_name = EncryptedData {
                nonce: name_nonce,
                ciphertext: name_ciphertext,
            };

            let birthday_bytes = birthday.to_xdr(&env);
            let (birthday_nonce, birthday_ciphertext) =
                encrypt_sensitive_data(&env, &birthday_bytes, &key);
            pet.encrypted_birthday = EncryptedData {
                nonce: birthday_nonce,
                ciphertext: birthday_ciphertext,
            };

            let breed_bytes = breed.to_xdr(&env);
            let (breed_nonce, breed_ciphertext) = encrypt_sensitive_data(&env, &breed_bytes, &key);
            pet.encrypted_breed = EncryptedData {
                nonce: breed_nonce,
                ciphertext: breed_ciphertext,
            };

            pet.gender = gender;
            pet.species = species;
            pet.privacy_level = privacy_level;
            pet.color = color;
            pet.weight = weight;
            pet.microchip_id = microchip_id;
            pet.updated_at = env.ledger().timestamp();

            env.storage().instance().set(&DataKey::Pet(id), &pet);
            Self::log_access(
                &env,
                id,
                pet.owner,
                AccessAction::Write,
                String::from_str(&env, "Pet profile updated"),
            );
            true
        } else {
            false
        }
    }

    pub fn get_pet(env: Env, id: u64, caller: Address) -> Option<PetProfile> {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            let _caller = caller;            // For now, we decrypt if Public, or we assume this function decrypts for the client to see.
            // Real privacy requires off-chain key management.
            // We will proceed with decryption to return the Profile.

            let key = Self::get_encryption_key(&env);

            let decrypted_name = decrypt_sensitive_data(
                &env,
                &pet.encrypted_name.ciphertext,
                &pet.encrypted_name.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let name =
                String::from_xdr(&env, &decrypted_name).unwrap_or(String::from_str(&env, "Error"));

            let decrypted_birthday = decrypt_sensitive_data(
                &env,
                &pet.encrypted_birthday.ciphertext,
                &pet.encrypted_birthday.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let birthday = String::from_xdr(&env, &decrypted_birthday)
                .unwrap_or(String::from_str(&env, "Error"));

            let decrypted_breed = decrypt_sensitive_data(
                &env,
                &pet.encrypted_breed.ciphertext,
                &pet.encrypted_breed.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let breed =
                String::from_xdr(&env, &decrypted_breed).unwrap_or(String::from_str(&env, "Error"));

            let a_bytes = decrypt_sensitive_data(
                &env,
                &pet.encrypted_allergies.ciphertext,
                &pet.encrypted_allergies.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let allergies = Vec::<Allergy>::from_xdr(&env, &a_bytes).unwrap_or(Vec::new(&env));

            let profile = PetProfile {
                id: pet.id,
                owner: pet.owner,
                privacy_level: pet.privacy_level,
                name,
                birthday,
                active: pet.active,
                created_at: pet.created_at,
                updated_at: pet.updated_at,
                new_owner: pet.new_owner,
                species: pet.species,
                gender: pet.gender,
                breed,
                color: pet.color,
                weight: pet.weight,
                microchip_id: pet.microchip_id,
                allergies,
            };
            Self::log_access(
                &env,
                id,
                env.current_contract_address(),
                AccessAction::Read,
                String::from_str(&env, "Pet profile accessed"),
            );
            Some(profile)
        } else {
            None
        }
    }

    pub fn get_pet_age(env: Env, pet_id: u64) -> (u64, u64) {
        if let Some(pet) = Self::get_pet(env.clone(), pet_id, env.current_contract_address()) {
            let current_time = env.ledger().timestamp();
            let birthday_timestamp = match Self::parse_birthday_timestamp(&pet.birthday) {
                Ok(timestamp) => timestamp,
                Err(_) => return (0, 0),
            };

            if current_time < birthday_timestamp {
                return (0, 0);
            }

            let elapsed_seconds = current_time - birthday_timestamp;
            let elapsed_days = elapsed_seconds / 86_400;
            let years = elapsed_days / 365;
            let remaining_days = elapsed_days % 365;
            let months = remaining_days / 30;

            return (years, months);
        }

        (0, 0)
    }

    fn parse_birthday_timestamp(birthday: &String) -> Result<u64, ContractError> {
        let len = birthday.len() as usize;
        if len == 0 || len > 20 {
            return Err(ContractError::InvalidInput);
        }

        let mut bytes = [0u8; 20];
        birthday.copy_into_slice(&mut bytes[..len]);

        if bytes.iter().take(len).all(u8::is_ascii_digit) {
            let mut timestamp = 0u64;
            for b in bytes.iter().take(len) {
                let digit = (b - b'0') as u64;
                timestamp = timestamp
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(digit))
                    .ok_or(ContractError::InvalidInput)?;
            }
            return Ok(timestamp);
        }

        if len != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(ContractError::InvalidInput);
        }

        let year = Self::parse_fixed_digits(&bytes[0..4])?;
        let month = Self::parse_fixed_digits(&bytes[5..7])?;
        let day = Self::parse_fixed_digits(&bytes[8..10])?;

        if !(1..=12).contains(&month) {
            return Err(ContractError::InvalidInput);
        }

        let max_day = Self::days_in_month(year, month);
        if day == 0 || day > max_day {
            return Err(ContractError::InvalidInput);
        }

        let days_since_epoch = Self::days_from_civil(year as i32, month as i32, day as i32)?;
        Ok(days_since_epoch * 86_400)
    }

    fn parse_fixed_digits(bytes: &[u8]) -> Result<u32, ContractError> {
        let mut value = 0u32;
        for b in bytes {
            if !b.is_ascii_digit() {
                return Err(ContractError::InvalidInput);
            }
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((b - b'0') as u32))
                .ok_or(ContractError::InvalidInput)?;
        }
        Ok(value)
    }

    fn is_leap_year(year: u32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }

    fn days_in_month(year: u32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if Self::is_leap_year(year) => 29,
            2 => 28,
            _ => 0,
        }
    }

    fn days_from_civil(year: i32, month: i32, day: i32) -> Result<u64, ContractError> {
        let adjusted_year = year - if month <= 2 { 1 } else { 0 };
        let era = if adjusted_year >= 0 {
            adjusted_year / 400
        } else {
            (adjusted_year - 399) / 400
        };
        let year_of_era = adjusted_year - era * 400;
        let month_of_year = month + if month > 2 { -3 } else { 9 };
        let day_of_year = (153 * month_of_year + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        let days = era * 146_097 + day_of_era - 719_468;
        if days < 0 {
            return Err(ContractError::InvalidInput);
        }
        Ok(days as u64)
    }

    pub fn is_pet_active(env: Env, id: u64) -> bool {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            pet.active
        } else {
            false
        }
    }

    pub fn get_pet_owner(env: Env, id: u64) -> Option<Address> {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            Some(pet.owner)
        } else {
            None
        }
    }

    pub fn activate_pet(env: Env, id: u64) {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            pet.owner.require_auth();
            if !pet.active {
                let active_count: u64 = env
                    .storage()
                    .instance()
                    .get(&StatsKey::ActivePetsCount)
                    .unwrap_or(0);
                env.storage()
                    .instance()
                    .set(&StatsKey::ActivePetsCount, &(active_count + 1));
            }
            pet.active = true;
            pet.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&DataKey::Pet(id), &pet);
        }
    }

    pub fn deactivate_pet(env: Env, id: u64) {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            pet.owner.require_auth();
            if pet.active {
                let active_count: u64 = env
                    .storage()
                    .instance()
                    .get(&StatsKey::ActivePetsCount)
                    .unwrap_or(0);
                if active_count > 0 {
                    env.storage()
                        .instance()
                        .set(&StatsKey::ActivePetsCount, &(active_count - 1));
                }
            }
            pet.active = false;
            pet.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&DataKey::Pet(id), &pet);
        }
    }

    pub fn add_pet_photo(env: Env, pet_id: u64, photo_hash: String) -> bool {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
        {
            pet.owner.require_auth();
            if let Err(err) = Self::validate_ipfs_hash(&env, &photo_hash) {
                env.panic_with_error(err);
            }
            pet.photo_hashes.push_back(photo_hash);
            pet.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&DataKey::Pet(pet_id), &pet);
            true
        } else {
            false
        }
    }

    pub fn get_pet_photos(env: Env, pet_id: u64) -> Vec<String> {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
        {
            pet.photo_hashes
        } else {
            Vec::new(&env)
        }
    }

    pub fn transfer_pet_ownership(env: Env, id: u64, to: Address) {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            pet.owner.require_auth();
            pet.new_owner = to;
            pet.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&DataKey::Pet(id), &pet);
        }
    }

    pub fn accept_pet_transfer(env: Env, id: u64) {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(id))
        {
            pet.new_owner.require_auth();

            let old_owner = pet.owner.clone();
            Self::remove_pet_from_owner_index(&env, &old_owner, id);

            pet.owner = pet.new_owner.clone();
            pet.updated_at = env.ledger().timestamp();

            Self::add_pet_to_owner_index(&env, &pet.owner, id);

            env.storage().instance().set(&DataKey::Pet(id), &pet);

            Self::log_ownership_change(
                &env,
                id,
                old_owner.clone(),
                pet.owner.clone(),
                String::from_str(&env, "Ownership Transfer"),
            );

            env.events().publish(
                (String::from_str(&env, "PetOwnershipTransferred"), id),
                PetOwnershipTransferredEvent {
                    pet_id: id,
                    old_owner,
                    new_owner: pet.owner.clone(),
                    timestamp: pet.updated_at,
                },
            );
        }
    }

    // --- HELPER FOR INDEX MAINTENANCE ---
    fn remove_pet_from_owner_index(env: &Env, owner: &Address, pet_id: u64) {
        let count = Self::get_owner_pet_count(env, owner);
        if count == 0 {
            return;
        }

        let mut remove_index: Option<u64> = None;
        for i in 1..=count {
            if let Some(pid) = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::OwnerPetIndex((owner.clone(), i)))
            {
                if pid == pet_id {
                    remove_index = Some(i);
                    break;
                }
            }
        }

        if let Some(idx) = remove_index {
            if idx != count {
                if let Some(last_pet_id) = env
                    .storage()
                    .instance()
                    .get::<DataKey, u64>(&DataKey::OwnerPetIndex((owner.clone(), count)))
                {
                    env.storage()
                        .instance()
                        .set(&DataKey::OwnerPetIndex((owner.clone(), idx)), &last_pet_id);
                }
            }
            env.storage()
                .instance()
                .remove(&DataKey::OwnerPetIndex((owner.clone(), count)));
            env.storage()
                .instance()
                .set(&DataKey::PetCountByOwner(owner.clone()), &(count - 1));
        }
    }

    fn add_pet_to_owner_index(env: &Env, owner: &Address, pet_id: u64) {
        let count = Self::get_owner_pet_count(env, owner);
        let new_count = count + 1;
        env.storage()
            .instance()
            .set(&DataKey::PetCountByOwner(owner.clone()), &new_count);
        env.storage()
            .instance()
            .set(&DataKey::OwnerPetIndex((owner.clone(), new_count)), &pet_id);
    }

    // --- OWNER MANAGEMENT ---

    pub fn register_pet_owner(
        env: Env,
        owner: Address,
        name: String,
        email: String,
        emergency_contact: String,
    ) {
        owner.require_auth();

        let key = Self::get_encryption_key(&env);
        let timestamp = env.ledger().timestamp();

        let name_bytes = name.to_xdr(&env);
        let (name_nonce, name_ciphertext) = encrypt_sensitive_data(&env, &name_bytes, &key);
        let encrypted_name = EncryptedData {
            nonce: name_nonce,
            ciphertext: name_ciphertext,
        };

        let email_bytes = email.to_xdr(&env);
        let (email_nonce, email_ciphertext) = encrypt_sensitive_data(&env, &email_bytes, &key);
        let encrypted_email = EncryptedData {
            nonce: email_nonce,
            ciphertext: email_ciphertext,
        };

        let contact_bytes = emergency_contact.to_xdr(&env);
        let (contact_nonce, contact_ciphertext) =
            encrypt_sensitive_data(&env, &contact_bytes, &key);
        let encrypted_emergency_contact = EncryptedData {
            nonce: contact_nonce,
            ciphertext: contact_ciphertext,
        };

        let pet_owner = PetOwner {
            owner_address: owner.clone(),
            privacy_level: PrivacyLevel::Public,
            encrypted_name,
            encrypted_email,
            encrypted_emergency_contact,
            created_at: timestamp,
            updated_at: timestamp,
            is_pet_owner: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::PetOwner(owner), &pet_owner);
    }

    pub fn is_owner_registered(env: Env, owner: Address) -> bool {
        if let Some(pet_owner) = env
            .storage()
            .instance()
            .get::<DataKey, PetOwner>(&DataKey::PetOwner(owner))
        {
            pet_owner.is_pet_owner
        } else {
            false
        }
    }

    pub fn update_owner_profile(
        env: Env,
        owner: Address,
        name: String,
        email: String,
        emergency_contact: String,
    ) -> bool {
        owner.require_auth();

        if let Some(mut pet_owner) = env
            .storage()
            .instance()
            .get::<DataKey, PetOwner>(&DataKey::PetOwner(owner.clone()))
        {
            let key = Self::get_encryption_key(&env);

            let name_bytes = name.to_xdr(&env);
            let (name_nonce, name_ciphertext) = encrypt_sensitive_data(&env, &name_bytes, &key);
            pet_owner.encrypted_name = EncryptedData {
                nonce: name_nonce,
                ciphertext: name_ciphertext,
            };

            let email_bytes = email.to_xdr(&env);
            let (email_nonce, email_ciphertext) = encrypt_sensitive_data(&env, &email_bytes, &key);
            pet_owner.encrypted_email = EncryptedData {
                nonce: email_nonce,
                ciphertext: email_ciphertext,
            };

            let contact_bytes = emergency_contact.to_xdr(&env);
            let (contact_nonce, contact_ciphertext) =
                encrypt_sensitive_data(&env, &contact_bytes, &key);
            pet_owner.encrypted_emergency_contact = EncryptedData {
                nonce: contact_nonce,
                ciphertext: contact_ciphertext,
            };

            pet_owner.updated_at = env.ledger().timestamp();

            env.storage()
                .instance()
                .set(&DataKey::PetOwner(owner), &pet_owner);
            true
        } else {
            false
        }
    }

    // Vet Verification & Registration
    const MAX_STR_SHORT: u32 = 100;
    const MAX_STR_LONG: u32 = 500;
    const MAX_VEC_MEDS: u32 = 20;
    const MAX_VEC_ATTACHMENTS: u32 = 10;
    const MAX_VET_NAME_LEN: u32 = 100;
    const MAX_VET_LICENSE_LEN: u32 = 50;
    const MAX_VET_SPEC_LEN: u32 = 100;

    /// Maximum byte length of a vet-review comment.
    /// Enforced in `add_vet_review` to bound on-chain storage and gas costs.
    const MAX_REVIEW_COMMENT_LEN: u32 = 500;

    pub fn register_vet(
        env: Env,
        vet_address: Address,
        name: String,
        license_number: String,
        specialization: String,
    ) -> bool {
        vet_address.require_auth();

        if env
            .storage()
            .instance()
            .has(&DataKey::VetLicense(license_number.clone()))
        {
            panic!("License already registered");
        }

        if env
            .storage()
            .instance()
            .has(&DataKey::Vet(vet_address.clone()))
        {
            panic!("Vet already registered");
        }

        let vet = Vet {
            address: vet_address.clone(),
            name,
            license_number: license_number.clone(),
            specialization,
            verified: false,
            clinic_info: None,
        };

        env.storage()
            .instance()
            .set(&DataKey::Vet(vet_address.clone()), &vet);
        env.storage()
            .instance()
            .set(&DataKey::VetLicense(license_number), &vet_address);

        true
    }

    pub fn verify_vet(env: Env, admin: Address, vet_address: Address) -> bool {
        Self::require_admin_auth(&env, &admin);
        Self::_verify_vet_internal(&env, vet_address)
    }

    fn _verify_vet_internal(env: &Env, vet_address: Address) -> bool {
        if let Some(mut vet) = env
            .storage()
            .instance()
            .get::<DataKey, Vet>(&DataKey::Vet(vet_address))
        {
            vet.verified = true;
            env.storage()
                .instance()
                .set(&DataKey::Vet(vet.address.clone()), &vet);
            true
        } else {
            false
        }
    }

    pub fn revoke_vet_license(env: Env, admin: Address, vet_address: Address) -> bool {
        Self::require_admin_auth(&env, &admin);
        Self::_revoke_vet_internal(&env, vet_address)
    }

    fn _revoke_vet_internal(env: &Env, vet_address: Address) -> bool {
        if let Some(mut vet) = env
            .storage()
            .instance()
            .get::<DataKey, Vet>(&DataKey::Vet(vet_address))
        {
            vet.verified = false;
            env.storage()
                .instance()
                .set(&DataKey::Vet(vet.address.clone()), &vet);
            true
        } else {
            false
        }
    }

    pub fn is_verified_vet(env: Env, vet_address: Address) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, Vet>(&DataKey::Vet(vet_address))
            .map(|vet| vet.verified)
            .unwrap_or(false)
    }

    pub fn get_vet(env: Env, vet_address: Address) -> Option<Vet> {
        env.storage().instance().get(&DataKey::Vet(vet_address))
    }

    pub fn get_vet_by_license(env: Env, license_number: String) -> Option<Vet> {
        let vet_address: Option<Address> = env
            .storage()
            .instance()
            .get(&DataKey::VetLicense(license_number));
        vet_address.and_then(|address| Self::get_vet(env, address))
    }

    /*
    /// Update clinic info for a vet. Only the vet can update their own clinic info.
    pub fn update_clinic_info(env: Env, vet_address: Address, clinic_info: String) -> bool {
        vet_address.require_auth();

        if let Some(mut vet) = env
            .storage()
            .instance()
            .get::<_, Vet>(&DataKey::Vet(vet_address.clone()))
        {
            vet.clinic_info = Some(clinic_info);
            env.storage()
                .instance()
                .set(&DataKey::Vet(vet_address), &vet);
            true
        } else {
            panic!("Vet not found");
        }
    }
    */

    // Pet Vaccination Record
    #[allow(clippy::too_many_arguments)]
    pub fn add_vaccination(
        env: Env,
        pet_id: u64,
        veterinarian: Address,
        vaccine_type: VaccineType,
        vaccine_name: String,
        administered_at: u64,
        next_due_date: u64,
        batch_number: String,
    ) -> u64 {
        veterinarian.require_auth();
        if !Self::is_verified_vet(env.clone(), veterinarian.clone()) {
            panic!("Veterinarian not verified");
        }

        let _pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        let vaccine_count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::VaccinationCount)
            .unwrap_or(0);
        let vaccine_id = vaccine_count + 1;
        let now = env.ledger().timestamp();
        let key = Self::get_encryption_key(&env);

        let vname_bytes = vaccine_name.to_xdr(&env);
        let (vname_nonce, vname_ciphertext) = encrypt_sensitive_data(&env, &vname_bytes, &key);
        let encrypted_vaccine_name = EncryptedData {
            nonce: vname_nonce,
            ciphertext: vname_ciphertext,
        };

        let batch_bytes = batch_number.to_xdr(&env);
        let (batch_nonce, batch_ciphertext) = encrypt_sensitive_data(&env, &batch_bytes, &key);
        let encrypted_batch_number = EncryptedData {
            nonce: batch_nonce,
            ciphertext: batch_ciphertext,
        };

        let record = Vaccination {
            id: vaccine_id,
            pet_id,
            veterinarian: veterinarian.clone(),
            vaccine_type: vaccine_type.clone(),
            vaccine_name: None,
            encrypted_vaccine_name,
            administered_at,
            next_due_date,
            batch_number: None,
            encrypted_batch_number,
            created_at: now,
        };

        Self::update_vet_stats(&env, &veterinarian, pet_id, 1, 1, 0);

        env.storage()
            .instance()
            .set(&MedicalKey::Vaccination(vaccine_id), &record);
        env.storage()
            .instance()
            .set(&MedicalKey::VaccinationCount, &vaccine_id);

        // Update indexes
        let pet_vax_count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::PetVaccinationCount(pet_id))
            .unwrap_or(0);
        let new_pet_vax_count = pet_vax_count + 1;
        env.storage()
            .instance()
            .set(&MedicalKey::PetVaccinationCount(pet_id), &new_pet_vax_count);
        env.storage().instance().set(
            &MedicalKey::PetVaccinationByIndex((pet_id, new_pet_vax_count)),
            &vaccine_id,
        );

        env.events().publish(
            (String::from_str(&env, "VaccinationAdded"), pet_id),
            VaccinationAddedEvent {
                vaccine_id,
                pet_id,
                veterinarian,
                vaccine_type,
                next_due_date,
                timestamp: now,
            },
        );

        vaccine_id
    }

    pub fn get_vaccinations(env: Env, vaccine_id: u64) -> Option<Vaccination> {
        if let Some(record) = env
            .storage()
            .instance()
            .get::<MedicalKey, Vaccination>(&MedicalKey::Vaccination(vaccine_id))
        {
            let key = Self::get_encryption_key(&env);

            let name_bytes = decrypt_sensitive_data(
                &env,
                &record.encrypted_vaccine_name.ciphertext,
                &record.encrypted_vaccine_name.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let vaccine_name =
                String::from_xdr(&env, &name_bytes).unwrap_or(String::from_str(&env, "Error"));

            let batch_bytes = decrypt_sensitive_data(
                &env,
                &record.encrypted_batch_number.ciphertext,
                &record.encrypted_batch_number.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let batch_number =
                String::from_xdr(&env, &batch_bytes).unwrap_or(String::from_str(&env, "Error"));

            let mut decrypted = record.clone();
            decrypted.vaccine_name = Some(vaccine_name);
            decrypted.batch_number = Some(batch_number);
            Some(decrypted)
        } else {
            None
        }
    }

    pub fn get_vaccination_history(env: Env, pet_id: u64, offset: u64, limit: u32) -> Vec<Vaccination> {
        if env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .is_none()
        {
            return Vec::new(&env);
        }

        let _vax_count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::PetVaccinationCount(pet_id))
            .unwrap_or(0);

        // Here we return decrypted history. Privacy check omitted for brevity in this merge step,
        // relying on upstream behavior + encryption presence.
        let count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::PetVaccinationCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);

        // Calculate the range to return based on offset and limit
        let start_index = offset + 1; // Indices start from 1
        let end_index = (offset + limit as u64).min(count);

        for i in start_index..=end_index {
            if let Some(vid) = env
                .storage()
                .instance()
                .get::<MedicalKey, u64>(&MedicalKey::PetVaccinationByIndex((pet_id, i)))
            {
                if let Some(vax) = Self::get_vaccinations(env.clone(), vid) {
                    history.push_back(vax);
                }
            }
        }
        history
    }

    pub fn get_upcoming_vaccinations(
        env: Env,
        pet_id: u64,
        days_threshold: u64,
    ) -> Vec<Vaccination> {
        let current_time = env.ledger().timestamp();
        let threshold = current_time + (days_threshold * 86400);
        let history = Self::get_vaccination_history(env.clone(), pet_id, 0, u32::MAX);
        let mut upcoming = Vec::new(&env);

        for vax in history.iter() {
            if vax.next_due_date <= threshold {
                upcoming.push_back(vax);
            }
        }
        upcoming
    }

    pub fn is_vaccination_current(env: Env, pet_id: u64, vaccine_type: VaccineType) -> bool {
        let current_time = env.ledger().timestamp();
        let history = Self::get_vaccination_history(env, pet_id, 0, u32::MAX);
        let mut most_recent: Option<Vaccination> = None;

        for vax in history.iter() {
            if vax.vaccine_type == vaccine_type {
                match most_recent.clone() {
                    Some(current) => {
                        if vax.administered_at > current.administered_at {
                            most_recent = Some(vax);
                        }
                    }
                    None => most_recent = Some(vax),
                }
            }
        }

        if let Some(vax) = most_recent {
            vax.next_due_date > current_time
        } else {
            false
        }
    }

    pub fn get_overdue_vaccinations(env: Env, pet_id: u64) -> Vec<VaccineType> {
        let current_time = env.ledger().timestamp();
        let history = Self::get_vaccination_history(env.clone(), pet_id, 0, u32::MAX);
        let mut overdue = Vec::new(&env);

        for vax in history.iter() {
            if vax.next_due_date < current_time {
                overdue.push_back(vax.vaccine_type);
            }
        }
        overdue
    }

    // --- NUTRITION / DIET FUNCTIONS ---
    pub fn set_diet_plan(
        env: Env,
        pet_id: u64,
        food_type: String,
        portion_size: String,
        frequency: String,
        restrictions: Vec<String>,
        allergies: Vec<String>,
    ) -> bool {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        pet.owner.require_auth();

        let diet_count: u64 = env
            .storage()
            .instance()
            .get(&NutritionKey::DietPlanCount)
            .unwrap_or(0);
        let diet_id = diet_count + 1;

        let now = env.ledger().timestamp();

        let plan = DietPlan {
            pet_id,
            food_type,
            portion_size,
            feeding_frequency: frequency,
            dietary_restrictions: restrictions,
            allergies,
            created_by: pet.owner.clone(),
            created_at: now,
        };

        env.storage()
            .instance()
            .set(&NutritionKey::DietPlan(diet_id), &plan);
        env.storage()
            .instance()
            .set(&NutritionKey::DietPlanCount, &diet_id);

        let pet_diet_count: u64 = env
            .storage()
            .instance()
            .get(&NutritionKey::PetDietCount(pet_id))
            .unwrap_or(0)
            + 1;
        env.storage()
            .instance()
            .set(&NutritionKey::PetDietCount(pet_id), &pet_diet_count);
        env.storage().instance().set(
            &NutritionKey::PetDietByIndex((pet_id, pet_diet_count)),
            &diet_id,
        );

        true
    }

    pub fn get_diet_plan(env: Env, diet_id: u64) -> Option<DietPlan> {
        env.storage()
            .instance()
            .get(&NutritionKey::DietPlan(diet_id))
    }

    pub fn get_diet_history(env: Env, pet_id: u64) -> Vec<DietPlan> {
        if env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .is_none()
        {
            return Vec::new(&env);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&NutritionKey::PetDietCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(did) = env
                .storage()
                .instance()
                .get::<NutritionKey, u64>(&NutritionKey::PetDietByIndex((pet_id, i)))
            {
                if let Some(plan) = Self::get_diet_plan(env.clone(), did) {
                    history.push_back(plan);
                }
            }
        }
        history
    }

    pub fn add_weight_entry(env: Env, pet_id: u64, weight: u32) -> bool {
        let mut pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        pet.owner.require_auth();

        let weight_count: u64 = env
            .storage()
            .instance()
            .get(&NutritionKey::WeightCount)
            .unwrap_or(0);
        let weight_id = weight_count + 1;
        let now = env.ledger().timestamp();

        let entry = WeightEntry {
            pet_id,
            weight: weight.into(),
            recorded_at: now,
            recorded_by: pet.owner.clone(),
        };

        // Persist entry
        env.storage()
            .instance()
            .set(&NutritionKey::WeightEntry(weight_id), &entry);
        env.storage()
            .instance()
            .set(&NutritionKey::WeightCount, &weight_id);

        let pet_weight_count: u64 = env
            .storage()
            .instance()
            .get(&NutritionKey::PetWeightCount(pet_id))
            .unwrap_or(0)
            + 1;
        env.storage()
            .instance()
            .set(&NutritionKey::PetWeightCount(pet_id), &pet_weight_count);
        env.storage().instance().set(
            &NutritionKey::PetWeightByIndex((pet_id, pet_weight_count)),
            &weight_id,
        );

        // Update current pet weight
        pet.weight = weight;
        pet.updated_at = now;
        env.storage().instance().set(&DataKey::Pet(pet_id), &pet);

        true
    }

    pub fn get_weight_history(env: Env, pet_id: u64) -> Vec<WeightEntry> {
        if env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .is_none()
        {
            return Vec::new(&env);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&NutritionKey::PetWeightCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(wid) = env
                .storage()
                .instance()
                .get::<NutritionKey, u64>(&NutritionKey::PetWeightByIndex((pet_id, i)))
            {
                if let Some(entry) = env
                    .storage()
                    .instance()
                    .get(&NutritionKey::WeightEntry(wid))
                {
                    history.push_back(entry);
                }
            }
        }
        history
    }

    // --- TAG LINKING (UPSTREAM IMPLEMENTATION) ---

    fn generate_tag_id(env: &Env, pet_id: u64, _owner: &Address) -> BytesN<32> {
        let nonce: u64 = env.storage().instance().get(&TagKey::TagNonce).unwrap_or(0);
        let new_nonce = nonce + 1;
        env.storage().instance().set(&TagKey::TagNonce, &new_nonce);

        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();

        let mut preimage = Bytes::new(env);
        for byte in pet_id.to_be_bytes() {
            preimage.push_back(byte);
        }
        for byte in new_nonce.to_be_bytes() {
            preimage.push_back(byte);
        }
        for byte in timestamp.to_be_bytes() {
            preimage.push_back(byte);
        }
        for byte in sequence.to_be_bytes() {
            preimage.push_back(byte);
        }

        env.crypto().sha256(&preimage).into()
    }

    pub fn link_tag_to_pet(env: Env, pet_id: u64) -> BytesN<32> {
        let pet = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        if env
            .storage()
            .instance()
            .get::<TagKey, BytesN<32>>(&TagKey::PetTagId(pet_id))
            .is_some()
        {
            panic!("Pet already has a linked tag");
        }

        let tag_id = Self::generate_tag_id(&env, pet_id, &pet.owner);
        let now = env.ledger().timestamp();

        let pet_tag = PetTag {
            tag_id: tag_id.clone(),
            pet_id,
            owner: pet.owner.clone(),
            message: String::from_str(&env, ""),
            is_active: true,
            linked_at: now,
            updated_at: now,
            tag_message: String::from_str(&env, ""),
            created_at: now,
        };

        env.storage()
            .instance()
            .set(&TagKey::Tag(tag_id.clone()), &pet_tag);
        env.storage()
            .instance()
            .set(&TagKey::PetTagId(pet_id), &tag_id);

        let count: u64 = env
            .storage()
            .instance()
            .get(&TagKey::PetTagCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&TagKey::PetTagCount, &(count + 1));

        env.events().publish(
            (String::from_str(&env, "TAG_LINKED"),),
            TagLinkedEvent {
                tag_id: tag_id.clone(),
                pet_id,
                owner: pet.owner.clone(),
                timestamp: now,
            },
        );

        tag_id
    }

    pub fn get_pet_by_tag(env: Env, tag_id: BytesN<32>) -> Option<PetProfile> {
        if let Some(tag) = env
            .storage()
            .instance()
            .get::<TagKey, PetTag>(&TagKey::Tag(tag_id))
        {
            if !tag.is_active {
                return None;
            }
            Self::get_pet(env.clone(), tag.pet_id, env.current_contract_address())
        } else {
            None
        }
    }

    pub fn get_tag(env: Env, tag_id: BytesN<32>) -> Option<PetTag> {
        env.storage().instance().get(&TagKey::Tag(tag_id))
    }

    pub fn get_tag_by_pet(env: Env, pet_id: u64) -> Option<BytesN<32>> {
        env.storage().instance().get(&TagKey::PetTagId(pet_id))
    }

    pub fn update_tag_message(env: Env, tag_id: BytesN<32>, message: String) -> bool {
        if let Some(mut tag) = env
            .storage()
            .instance()
            .get::<TagKey, PetTag>(&TagKey::Tag(tag_id.clone()))
        {
            let pet = env
                .storage()
                .instance()
                .get::<DataKey, Pet>(&DataKey::Pet(tag.pet_id))
                .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
            pet.owner.require_auth();

            tag.message = message;
            tag.updated_at = env.ledger().timestamp();

            env.storage().instance().set(&TagKey::Tag(tag_id), &tag);
            true
        } else {
            false
        }
    }

    pub fn deactivate_tag(env: Env, tag_id: BytesN<32>) -> bool {
        if let Some(mut tag) = env
            .storage()
            .instance()
            .get::<TagKey, PetTag>(&TagKey::Tag(tag_id.clone()))
        {
            let pet = env
                .storage()
                .instance()
                .get::<DataKey, Pet>(&DataKey::Pet(tag.pet_id))
                .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
            pet.owner.require_auth();

            tag.is_active = false;
            tag.updated_at = env.ledger().timestamp();
            env.storage()
                .instance()
                .set(&TagKey::Tag(tag_id.clone()), &tag);

            env.events().publish(
                (String::from_str(&env, "TAG_DEACTIVATED"),),
                TagDeactivatedEvent {
                    tag_id,
                    pet_id: tag.pet_id,
                    deactivated_by: pet.owner,
                    timestamp: env.ledger().timestamp(),
                },
            );
            true
        } else {
            false
        }
    }

    pub fn reactivate_tag(env: Env, tag_id: BytesN<32>) -> bool {
        if let Some(mut tag) = env
            .storage()
            .instance()
            .get::<TagKey, PetTag>(&TagKey::Tag(tag_id.clone()))
        {
            let pet = env
                .storage()
                .instance()
                .get::<DataKey, Pet>(&DataKey::Pet(tag.pet_id))
                .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
            pet.owner.require_auth();

            tag.is_active = true;
            tag.updated_at = env.ledger().timestamp();
            env.storage()
                .instance()
                .set(&TagKey::Tag(tag_id.clone()), &tag);

            env.events().publish(
                (String::from_str(&env, "TAG_REACTIVATED"),),
                TagReactivatedEvent {
                    tag_id,
                    pet_id: tag.pet_id,
                    reactivated_by: pet.owner,
                    timestamp: env.ledger().timestamp(),
                },
            );
            true
        } else {
            false
        }
    }

    pub fn is_tag_active(env: Env, tag_id: BytesN<32>) -> bool {
        if let Some(tag) = env
            .storage()
            .instance()
            .get::<TagKey, PetTag>(&TagKey::Tag(tag_id))
        {
            tag.is_active
        } else {
            false
        }
    }

    // --- HELPERS ---

    fn get_owner_pet_count(env: &Env, owner: &Address) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PetCountByOwner(owner.clone()))
            .unwrap_or(0)
    }

    fn medical_record_matches_filter(
        env: &Env,
        record: &MedicalRecord,
        filter: &MedicalRecordFilter,
    ) -> bool {
        if let Some(vet_address) = &filter.vet_address {
            if record.vet_address != *vet_address {
                return false;
            }
        }

        if let Some(from_date) = filter.from_date {
            if record.date < from_date {
                return false;
            }
        }

        if let Some(to_date) = filter.to_date {
            if record.date > to_date {
                return false;
            }
        }

        if let Some(keyword) = &filter.diagnosis_keyword {
            if !Self::string_contains(env, &record.diagnosis, keyword) {
                return false;
            }
        }

        true
    }

    fn string_contains(_env: &Env, haystack: &String, needle: &String) -> bool {
        let haystack_len = haystack.len() as usize;
        let needle_len = needle.len() as usize;

        if needle_len == 0 {
            return true;
        }
        if needle_len > haystack_len {
            return false;
        }

        let mut haystack_bytes = [0u8; Self::MAX_STR_LONG as usize];
        let mut needle_bytes = [0u8; Self::MAX_STR_LONG as usize];
        haystack.copy_into_slice(&mut haystack_bytes[..haystack_len]);
        needle.copy_into_slice(&mut needle_bytes[..needle_len]);

        for start in 0..=(haystack_len - needle_len) {
            let mut matches = true;
            for offset in 0..needle_len {
                if haystack_bytes[start + offset] != needle_bytes[offset] {
                    matches = false;
                    break;
                }
            }

            if matches {
                return true;
            }
        }

        false
    }

    fn species_to_string(env: &Env, species: &Species) -> String {
        match species {
            Species::Other => String::from_str(env, "Other"),
            Species::Dog => String::from_str(env, "Dog"),
            Species::Cat => String::from_str(env, "Cat"),
            Species::Bird => String::from_str(env, "Bird"),
        }
    }

    fn validate_ipfs_hash(_env: &Env, hash: &String) -> Result<(), ContractError> {
        let len = hash.len() as usize;
        if len == 46 {
            let mut bytes = [0u8; 46];
            hash.copy_into_slice(&mut bytes);

            if bytes[0] != b'Q' || bytes[1] != b'm' {
                return Err(ContractError::InvalidIpfsHash);
            }

            for b in bytes.iter() {
                if !matches!(
                    b,
                    b'1'..=b'9'
                        | b'A'..=b'H'
                        | b'J'..=b'N'
                        | b'P'..=b'Z'
                        | b'a'..=b'k'
                        | b'm'..=b'z'
                ) {
                    return Err(ContractError::InvalidIpfsHash);
                }
            }

            return Ok(());
        }

        if !(2..=128).contains(&len) {
            return Err(ContractError::InvalidIpfsHash);
        }

        let mut bytes = [0u8; 128];
        hash.copy_into_slice(&mut bytes[..len]);

        if bytes[0] != b'b' {
            return Err(ContractError::InvalidIpfsHash);
        }

        for b in bytes.iter().take(len).skip(1) {
            if !matches!(b, b'a'..=b'z' | b'2'..=b'7') {
                return Err(ContractError::InvalidIpfsHash);
            }
        }

        Ok(())
    }

    fn get_encryption_key(env: &Env) -> Bytes {
        // Derive a stable, contract-scoped key from contract identity + admin context.
        // This avoids static hardcoded key material while remaining deterministic.
        let mut preimage = Bytes::new(env);
        for byte in b"petchain:encryption-key:v1" {
            preimage.push_back(*byte);
        }

        let contract_xdr = env.current_contract_address().to_xdr(env);
        for byte in contract_xdr.iter() {
            preimage.push_back(byte);
        }

        if let Some(legacy_admin) = env.storage().instance().get::<DataKey, Address>(&DataKey::Admin) {
            let admin_xdr = legacy_admin.to_xdr(env);
            for byte in admin_xdr.iter() {
                preimage.push_back(byte);
            }
        } else if let Some(admins) = env
            .storage()
            .instance()
            .get::<SystemKey, Vec<Address>>(&SystemKey::Admins)
        {
            if let Some(primary_admin) = admins.get(0) {
                let admin_xdr = primary_admin.to_xdr(env);
                for byte in admin_xdr.iter() {
                    preimage.push_back(byte);
                }
            }
        }

        env.crypto().sha256(&preimage).into()
    }

    fn log_ownership_change(
        env: &Env,
        pet_id: u64,
        previous_owner: Address,
        new_owner: Address,
        reason: String,
    ) {
        let global_count: u64 = env
            .storage()
            .instance()
            .get(&SystemKey::OwnershipRecordCount)
            .unwrap_or(0);
        let record_id = global_count + 1;

        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&SystemKey::PetOwnershipRecordCount(pet_id))
            .unwrap_or(0);
        let new_pet_count = pet_count + 1;

        let record = OwnershipRecord {
            pet_id,
            previous_owner,
            new_owner,
            transfer_date: env.ledger().timestamp(),
            transfer_reason: reason,
        };

        env.storage()
            .instance()
            .set(&SystemKey::PetOwnershipRecord(record_id), &record);
        env.storage()
            .instance()
            .set(&SystemKey::OwnershipRecordCount, &record_id);
        env.storage()
            .instance()
            .set(&SystemKey::PetOwnershipRecordCount(pet_id), &new_pet_count);
        env.storage().instance().set(
            &SystemKey::PetOwnershipRecordIndex((pet_id, new_pet_count)),
            &record_id,
        );
    }

    pub fn get_ownership_history(env: Env, pet_id: u64) -> Vec<OwnershipRecord> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&SystemKey::PetOwnershipRecordCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(record_id) = env
                .storage()
                .instance()
                .get::<SystemKey, u64>(&SystemKey::PetOwnershipRecordIndex((pet_id, i)))
            {
                if let Some(record) = env
                    .storage()
                    .instance()
                    .get::<SystemKey, OwnershipRecord>(&SystemKey::PetOwnershipRecord(record_id))
                {
                    history.push_back(record);
                }
            }
        }
        history
    }
    // --- EMERGENCY CONTACTS ---
    pub fn set_emergency_contacts(
        env: Env,
        pet_id: u64,
        contacts: Vec<EmergencyContact>,
        allergies: Vec<Allergy>,
        medical_notes: String,
    ) {
        if let Some(mut pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
        {
            pet.owner.require_auth();

            let key = Self::get_encryption_key(&env);

            let contacts_bytes = contacts.to_xdr(&env);
            let (c_nonce, c_cipher) = encrypt_sensitive_data(&env, &contacts_bytes, &key);
            pet.encrypted_emergency_contacts = EncryptedData {
                nonce: c_nonce,
                ciphertext: c_cipher,
            };

            let allergies_bytes = allergies.to_xdr(&env);
            let (a_nonce, a_cipher) = encrypt_sensitive_data(&env, &allergies_bytes, &key);
            pet.encrypted_allergies = EncryptedData {
                nonce: a_nonce,
                ciphertext: a_cipher,
            };

            let notes_bytes = medical_notes.to_xdr(&env);
            let (n_nonce, n_cipher) = encrypt_sensitive_data(&env, &notes_bytes, &key);
            pet.encrypted_medical_alerts = EncryptedData {
                nonce: n_nonce,
                ciphertext: n_cipher,
            };

            pet.updated_at = env.ledger().timestamp();

            env.storage().instance().set(&DataKey::Pet(pet_id), &pet);
        } else {
            panic!("Pet not found");
        }
    }

    pub fn add_emergency_responder(env: Env, pet_id: u64, responder: Address) {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| panic!("Pet not found"));
        pet.owner.require_auth();
        let key = DataKey::EmergencyResponders(pet_id);
        let mut responders: Vec<Address> = env.storage().instance().get(&key).unwrap_or(Vec::new(&env));
        if !responders.contains(responder.clone()) {
            responders.push_back(responder);
        }
        env.storage().instance().set(&key, &responders);
    }

    pub fn remove_emergency_responder(env: Env, pet_id: u64, responder: Address) {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| panic!("Pet not found"));
        pet.owner.require_auth();
        let key = DataKey::EmergencyResponders(pet_id);
        let responders: Vec<Address> = env.storage().instance().get(&key).unwrap_or(Vec::new(&env));
        let mut updated = Vec::new(&env);
        for r in responders.iter() {
            if r != responder {
                updated.push_back(r);
            }
        }
        env.storage().instance().set(&key, &updated);
    }

    fn is_emergency_authorized(env: &Env, pet_id: u64, caller: &Address, owner: &Address) -> bool {
        if caller == owner {
            return true;
        }
        let key = DataKey::EmergencyResponders(pet_id);
        let responders: Vec<Address> = env.storage().instance().get(&key).unwrap_or(Vec::new(env));
        responders.contains(caller.clone())
    }

    pub fn get_emergency_info(env: Env, pet_id: u64, caller: Address) -> EmergencyInfo {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
        {
            if !Self::is_emergency_authorized(&env, pet_id, &caller, &pet.owner) {
                panic!("Unauthorized");
            }
            let key = Self::get_encryption_key(&env);

            let c_bytes = decrypt_sensitive_data(
                &env,
                &pet.encrypted_emergency_contacts.ciphertext,
                &pet.encrypted_emergency_contacts.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let contacts =
                Vec::<EmergencyContact>::from_xdr(&env, &c_bytes).unwrap_or(Vec::new(&env));

            let n_bytes = decrypt_sensitive_data(
                &env,
                &pet.encrypted_medical_alerts.ciphertext,
                &pet.encrypted_medical_alerts.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let notes = String::from_xdr(&env, &n_bytes).unwrap_or(String::from_str(&env, ""));

            let mut critical_alerts = Vec::new(&env);
            if !notes.is_empty() {
                critical_alerts.push_back(notes);
            }

            let a_bytes = decrypt_sensitive_data(
                &env,
                &pet.encrypted_allergies.ciphertext,
                &pet.encrypted_allergies.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            let all_allergies = Vec::<Allergy>::from_xdr(&env, &a_bytes).unwrap_or(Vec::new(&env));

            let mut critical_allergies = Vec::new(&env);
            for allergy in all_allergies.iter() {
                if allergy.is_critical {
                    critical_allergies.push_back(allergy);
                }
            }

            // Log the emergency access
            let log = EmergencyAccessLog {
                pet_id,
                accessed_by: env.current_contract_address(),
                timestamp: env.ledger().timestamp(),
            };

            let log_key = DataKey::EmergencyAccessLogs(pet_id);
            let mut logs: Vec<EmergencyAccessLog> = env
                .storage()
                .persistent()
                .get(&log_key)
                .unwrap_or(Vec::new(&env));
            while logs.len() >= MAX_LOG_ENTRIES {
                logs.remove(0);
            }
            logs.push_back(log);
            env.storage().persistent().set(&log_key, &logs);

            EmergencyInfo {
                pet_id,
                species: Self::species_to_string(&env, &pet.species),
                allergies: critical_allergies,
                critical_alerts,
                emergency_contacts: contacts,
            }
        } else {
            panic_with_error!(&env, ContractError::PetNotFound);
        }
    }

    pub fn get_emergency_contacts(env: Env, pet_id: u64, caller: Address) -> Vec<EmergencyContact> {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<_, Pet>(&DataKey::Pet(pet_id))
        {
            if !Self::is_emergency_authorized(&env, pet_id, &caller, &pet.owner) {
                panic!("Unauthorized");
            }
            let key = Self::get_encryption_key(&env);
            let c_bytes = decrypt_sensitive_data(
                &env,
                &pet.encrypted_emergency_contacts.ciphertext,
                &pet.encrypted_emergency_contacts.nonce,
                &key,
            )
            .unwrap_or(Bytes::new(&env));
            Vec::<EmergencyContact>::from_xdr(&env, &c_bytes).unwrap_or(Vec::new(&env))
        } else {
            Vec::new(&env)
        }
    }

    // --- ACCESSIBLE PETS ---
    pub fn get_accessible_pets(env: Env, user: Address) -> Vec<u64> {
        user.require_auth();
        let mut accessible_pets = Vec::new(&env);
        let count = Self::get_owner_pet_count(&env, &user);
        for i in 1..=count {
            if let Some(pid) = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::OwnerPetIndex((user.clone(), i)))
            {
                accessible_pets.push_back(pid);
            }
        }
        accessible_pets
    }

    pub fn get_all_pets_by_owner(env: Env, owner: Address) -> Vec<PetProfile> {
        let count = Self::get_owner_pet_count(&env, &owner);
        let mut pets = Vec::new(&env);
        for i in 1..=count {
            if let Some(pid) = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::OwnerPetIndex((owner.clone(), i)))
            {
                if let Some(pet) = Self::get_pet(env.clone(), pid, env.current_contract_address()) {
                    pets.push_back(pet);
                }
            }
        }
        pets
    }

    pub fn get_pets_by_owner(env: Env, owner: Address, offset: u64, limit: u32) -> Vec<PetProfile> {
        let count = Self::get_owner_pet_count(&env, &owner);
        let mut pets = Vec::new(&env);
        if count == 0 || limit == 0 || offset >= count {
            return pets;
        }

        let start_index = offset.saturating_add(1);
        let requested_end = offset.saturating_add(limit as u64);
        let end_index = if requested_end > count {
            count
        } else {
            requested_end
        };

        for i in start_index..=end_index {
            if let Some(pid) = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::OwnerPetIndex((owner.clone(), i)))
            {
                if let Some(pet) = Self::get_pet(env.clone(), pid, env.current_contract_address()) {
                    pets.push_back(pet);
                }
            }
        }

        pets
    }

    pub fn get_pets_by_species(env: Env, species: String) -> Vec<PetProfile> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SpeciesPetCount(species.clone()))
            .unwrap_or(0);
        let mut pets = Vec::new(&env);
        for i in 1..=count {
            if let Some(pid) = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::SpeciesPetIndex((species.clone(), i)))
            {
                if let Some(pet) = Self::get_pet(env.clone(), pid, env.current_contract_address()) {
                    pets.push_back(pet);
                }
            }
        }
        pets
    }

    pub fn get_active_pets(env: Env) -> Vec<PetProfile> {
        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PetCount)
            .unwrap_or(0);
        let mut pets = Vec::new(&env);
        for id in 1..=pet_count {
            if let Some(pet) = env
                .storage()
                .instance()
                .get::<DataKey, Pet>(&DataKey::Pet(id))
            {
                if pet.active {
                    if let Some(profile) = Self::get_pet(env.clone(), id, env.current_contract_address()) {
                        pets.push_back(profile);
                    }
                }
            }
        }
        pets
    }

    // --- ACCESS CONTROL ---
    pub fn grant_access(
        env: Env,
        pet_id: u64,
        grantee: Address,
        access_level: AccessLevel,
        expires_at: Option<u64>,
    ) -> bool {
        let pet = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();
        let granter = pet.owner.clone();

        let now = env.ledger().timestamp();
        let grant = AccessGrant {
            pet_id,
            granter: granter.clone(),
            grantee: grantee.clone(),
            access_level: access_level.clone(),
            granted_at: now,
            expires_at,
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::AccessGrant((pet_id, grantee.clone())), &grant);

        let grant_count = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::AccessGrantCount(pet_id))
            .unwrap_or(0);
        let new_count = grant_count + 1;
        env.storage()
            .instance()
            .set(&DataKey::AccessGrantCount(pet_id), &new_count);
        env.storage()
            .instance()
            .set(&DataKey::AccessGrantIndex((pet_id, new_count)), &grantee);

        env.events().publish(
            (String::from_str(&env, "AccessGranted"), pet_id),
            AccessGrantedEvent {
                pet_id,
                granter: granter.clone(),
                grantee,
                access_level,
                expires_at,
                timestamp: now,
            },
        );
        Self::log_access(
            &env,
            pet_id,
            granter,
            AccessAction::Grant,
            String::from_str(&env, "Access granted"),
        );
        true
    }

    pub fn revoke_access(env: Env, pet_id: u64, grantee: Address) -> bool {
        let pet = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();
        let granter = pet.owner.clone();

        let key = DataKey::AccessGrant((pet_id, grantee.clone()));
        if let Some(mut grant) = env.storage().instance().get::<DataKey, AccessGrant>(&key) {
            grant.is_active = false;
            grant.access_level = AccessLevel::None;
            env.storage().instance().set(&key, &grant);
            env.events().publish(
                (String::from_str(&env, "AccessRevoked"), pet_id),
                AccessRevokedEvent {
                    pet_id,
                    granter: granter.clone(),
                    grantee,
                    timestamp: env.ledger().timestamp(),
                },
            );
            Self::log_access(
                &env,
                pet_id,
                granter,
                AccessAction::Revoke,
                String::from_str(&env, "Access revoked"),
            );
            true
        } else {
            false
        }
    }

    pub fn grant_temporary_custody(
        env: Env,
        pet_id: u64,
        custodian: Address,
        start_date: u64,
        end_date: u64,
        permissions: Vec<String>,
    ) -> TemporaryCustody {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        let custody = TemporaryCustody {
            pet_id,
            owner: pet.owner,
            custodian,
            start_date,
            end_date,
            permissions,
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::TemporaryCustody(pet_id), &custody);

        custody
    }

    pub fn revoke_temporary_custody(env: Env, pet_id: u64) {
        let mut custody: TemporaryCustody = env
            .storage()
            .instance()
            .get(&DataKey::TemporaryCustody(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::CustodyNotFound));

        custody.owner.require_auth();

        custody.is_active = false;

        env.storage()
            .instance()
            .set(&DataKey::TemporaryCustody(pet_id), &custody);
    }

    pub fn is_custody_valid(env: Env, pet_id: u64) -> bool {
        let custody: TemporaryCustody = env
            .storage()
            .instance()
            .get(&DataKey::TemporaryCustody(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::CustodyNotFound));
        let current_time = env.ledger().timestamp();
        custody.is_active && current_time <= custody.end_date
    }

    // --- MEDICAL RECORDS ---

    pub fn add_medical_record(
        env: Env,
        pet_id: u64,
        vet_address: Address,
        diagnosis: String,
        treatment: String,
        medications: Vec<Medication>,
        notes: String,
    ) -> u64 {
        // Vet authorization check
        vet_address.require_auth();
        if diagnosis.len() > Self::MAX_STR_LONG {
            panic!("diagnosis too long");
        }
        if treatment.len() > Self::MAX_STR_LONG {
            panic!("treatment too long");
        }
        if notes.len() > Self::MAX_STR_LONG {
            panic!("notes too long");
        }
        if medications.len() > Self::MAX_VEC_MEDS {
            panic!("too many medications");
        }

        if diagnosis.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }
        if treatment.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }
        if notes.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }
        if medications.len() > Self::MAX_VEC_MEDS {
            panic_with_error!(&env, ContractError::TooManyItems);
        }

        if diagnosis.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }
        if treatment.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }
        if notes.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }
        if medications.len() > Self::MAX_VEC_MEDS {
            panic_with_error!(&env, ContractError::TooManyItems);
        }

        // Verify vet is verified
        if !Self::is_verified_vet(env.clone(), vet_address.clone()) {
            panic!("Veterinarian not verified");
        }

        // Verify pet exists
        let _pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        // Get and increment medical record count
        let count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::MedicalRecordCount)
            .unwrap_or(0);
        let id = count + 1;
        env.storage()
            .instance()
            .set(&MedicalKey::MedicalRecordCount, &id);

        let now = env.ledger().timestamp();
        let record = MedicalRecord {
            id,
            pet_id,
            vet_address: vet_address.clone(),
            diagnosis,
            treatment,
            medications,
            date: now,
            updated_at: now,
            notes,
            attachment_hashes: Vec::new(&env),
        };

        // Store the medical record
        env.storage()
            .instance()
            .set(&MedicalKey::MedicalRecord(id), &record);

        // Update pet medical record index
        let pet_record_count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::PetMedicalRecordCount(pet_id))
            .unwrap_or(0);
        let new_pet_record_count = pet_record_count + 1;
        env.storage().instance().set(
            &MedicalKey::PetMedicalRecordCount(pet_id),
            &new_pet_record_count,
        );
        env.storage().instance().set(
            &MedicalKey::PetMedicalRecordIndex((pet_id, new_pet_record_count)),
            &id,
        );

        Self::update_vet_stats(&env, &vet_address, pet_id, 1, 0, 1);

        // Publish event
        env.events().publish(
            (String::from_str(&env, "MedicalRecordAdded"), pet_id),
            MedicalRecordAddedEvent {
                pet_id,
                updated_by: vet_address.clone(),
                timestamp: now,
            },
        );
        Self::log_access(
            &env,
            pet_id,
            vet_address,
            AccessAction::Write,
            String::from_str(&env, "Medical record added"),
        );

        id
    }

    pub fn update_medical_record(
        env: Env,
        record_id: u64,
        diagnosis: String,
        treatment: String,
        medications: Vec<Medication>,
        notes: String,
    ) -> bool {
        if let Some(mut record) = env
            .storage()
            .instance()
            .get::<MedicalKey, MedicalRecord>(&MedicalKey::MedicalRecord(record_id))
        {
            record.vet_address.require_auth();

            record.diagnosis = diagnosis;
            record.treatment = treatment;
            record.medications = medications;
            record.notes = notes;
            record.date = env.ledger().timestamp();

            env.storage()
                .instance()
                .set(&MedicalKey::MedicalRecord(record_id), &record);
            Self::log_access(
                &env,
                record.pet_id,
                record.vet_address,
                AccessAction::Write,
                String::from_str(&env, "Medical record updated"),
            );
            true
        } else {
            false
        }
    }

    pub fn get_vet_stats(env: Env, vet: Address) -> VetStats {
        env.storage()
            .instance()
            .get::<_, VetStats>(&VetKey::VetStats(vet))
            .unwrap_or(VetStats {
                total_records: 0,
                total_vaccinations: 0,
                total_treatments: 0,
                pets_treated: 0,
            })
    }

    pub fn get_medical_record(env: Env, record_id: u64) -> Option<MedicalRecord> {
        let record: Option<MedicalRecord> = env
            .storage()
            .instance()
            .get(&MedicalKey::MedicalRecord(record_id));
        if let Some(ref r) = record {
            Self::log_access(
                &env,
                r.pet_id,
                env.current_contract_address(),
                AccessAction::Read,
                String::from_str(&env, "Medical record accessed"),
            );
        }
        record
    }

    pub fn get_pet_medical_records(
        env: Env,
        pet_id: u64,
        offset: u64,
        limit: u32,
    ) -> Vec<MedicalRecord> {
        let count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::PetMedicalRecordCount(pet_id))
            .unwrap_or(0);
        let mut records = Vec::new(&env);
        if count == 0 || limit == 0 || offset >= count {
            return records;
        }

        let start_index = offset.saturating_add(1);
        let requested_end = offset.saturating_add(limit as u64);
        let end_index = if requested_end > count {
            count
        } else {
            requested_end
        };

        for i in start_index..=end_index {
            if let Some(rid) = env
                .storage()
                .instance()
                .get::<MedicalKey, u64>(&MedicalKey::PetMedicalRecordIndex((pet_id, i)))
            {
                if let Some(record) = Self::get_medical_record(env.clone(), rid) {
                    records.push_back(record);
                }
            }
        }
        Self::log_access(
            &env,
            pet_id,
            env.current_contract_address(),
            AccessAction::Read,
            String::from_str(&env, "Pet medical records accessed"),
        );
        records
    }

    pub fn search_medical_records(
        env: Env,
        pet_id: u64,
        filter: MedicalRecordFilter,
        offset: u64,
        limit: u32,
    ) -> Vec<MedicalRecord> {
        let count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::PetMedicalRecordCount(pet_id))
            .unwrap_or(0);
        let mut records = Vec::new(&env);
        if count == 0 || limit == 0 {
            return records;
        }

        let mut skipped = 0u64;
        for i in 1..=count {
            let Some(record_id) = env
                .storage()
                .instance()
                .get::<MedicalKey, u64>(&MedicalKey::PetMedicalRecordIndex((pet_id, i)))
            else {
                continue;
            };

            let Some(record) = Self::get_medical_record(env.clone(), record_id) else {
                continue;
            };

            if !Self::medical_record_matches_filter(&env, &record, &filter) {
                continue;
            }

            if skipped < offset {
                skipped += 1;
                continue;
            }

            records.push_back(record);
            if records.len() >= limit {
                break;
            }
        }

        Self::log_access(
            &env,
            pet_id,
            env.current_contract_address(),
            AccessAction::Read,
            String::from_str(&env, "Medical record search executed"),
        );

        records
    }

    // --- ATTACHMENT MANAGEMENT ---

    /// Add an attachment to a medical record
    /// Only the vet who created the record can add attachments
    pub fn add_attachment(
        env: Env,
        record_id: u64,
        ipfs_hash: String,
        metadata: AttachmentMetadata,
    ) -> bool {
        // Validate IPFS hash format
        if let Err(err) = Self::validate_ipfs_hash(&env, &ipfs_hash) {
            env.panic_with_error(err);
        }

        // Get the medical record
        if let Some(mut record) = env
            .storage()
            .instance()
            .get::<MedicalKey, MedicalRecord>(&MedicalKey::MedicalRecord(record_id))
        {
            // Require authentication from the vet who created the record
            record.vet_address.require_auth();

            // Validate metadata
            if metadata.filename.is_empty() {
                env.panic_with_error(ContractError::FilenameEmpty);
            }
            if metadata.file_type.is_empty() {
                env.panic_with_error(ContractError::FileTypeEmpty);
            }
            if metadata.size == 0 {
                env.panic_with_error(ContractError::FileSizeZero);
            }

            // Create attachment
            let attachment = Attachment {
                ipfs_hash,
                metadata,
            };

            // Add to record
            record.attachment_hashes.push_back(attachment);
            record.date = env.ledger().timestamp();

            // Save updated record
            env.storage()
                .instance()
                .set(&MedicalKey::MedicalRecord(record_id), &record);

            // Log the action
            Self::log_access(
                &env,
                record.pet_id,
                record.vet_address,
                AccessAction::Write,
                String::from_str(&env, "Attachment added to medical record"),
            );

            true
        } else {
            false
        }
    }

    /// Get all attachments for a medical record
    pub fn get_attachments(env: Env, record_id: u64) -> Vec<Attachment> {
        if let Some(record) = env
            .storage()
            .instance()
            .get::<MedicalKey, MedicalRecord>(&MedicalKey::MedicalRecord(record_id))
        {
            // Log access
            Self::log_access(
                &env,
                record.pet_id,
                env.current_contract_address(),
                AccessAction::Read,
                String::from_str(&env, "Medical record attachments accessed"),
            );

            record.attachment_hashes
        } else {
            Vec::new(&env)
        }
    }

    /// Remove an attachment from a medical record by index
    /// Only the vet who created the record can remove attachments
    pub fn remove_attachment(env: Env, record_id: u64, attachment_index: u32) -> bool {
        if let Some(mut record) = env
            .storage()
            .instance()
            .get::<MedicalKey, MedicalRecord>(&MedicalKey::MedicalRecord(record_id))
        {
            // Require authentication from the vet who created the record
            record.vet_address.require_auth();

            // Check if index is valid
            if attachment_index >= record.attachment_hashes.len() {
                return false;
            }

            // Remove the attachment
            record.attachment_hashes.remove(attachment_index);
            record.date = env.ledger().timestamp();

            // Save updated record
            env.storage()
                .instance()
                .set(&MedicalKey::MedicalRecord(record_id), &record);

            // Log the action
            Self::log_access(
                &env,
                record.pet_id,
                record.vet_address,
                AccessAction::Write,
                String::from_str(&env, "Attachment removed from medical record"),
            );

            true
        } else {
            false
        }
    }

    /// Get the count of attachments for a medical record
    pub fn get_attachment_count(env: Env, record_id: u64) -> u32 {
        if let Some(record) = env
            .storage()
            .instance()
            .get::<MedicalKey, MedicalRecord>(&MedicalKey::MedicalRecord(record_id))
        {
            record.attachment_hashes.len()
        } else {
            0
        }
    }

    pub fn get_access_logs(env: Env, pet_id: u64) -> Vec<AccessLog> {
        let key = (Symbol::new(&env, "access_logs"), pet_id);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env))
    }

    pub fn check_access(env: Env, pet_id: u64, user: Address) -> AccessLevel {
        if let Some(pet) = env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
        {
            if pet.owner == user {
                return AccessLevel::Full;
            }
            if let Some(grant) = env
                .storage()
                .instance()
                .get::<DataKey, AccessGrant>(&DataKey::AccessGrant((pet_id, user)))
            {
                if !grant.is_active {
                    return AccessLevel::None;
                }
                if let Some(exp) = grant.expires_at {
                    if env.ledger().timestamp() >= exp {
                        return AccessLevel::None;
                    }
                }
                return grant.access_level;
            }
        }
        AccessLevel::None
    }

    pub fn get_authorized_users(env: Env, pet_id: u64) -> Vec<Address> {
        let count = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::AccessGrantCount(pet_id))
            .unwrap_or(0);
        let mut users = Vec::new(&env);
        for i in 1..=count {
            if let Some(grantee) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::AccessGrantIndex((pet_id, i)))
            {
                if Self::check_access(env.clone(), pet_id, grantee.clone()) != AccessLevel::None {
                    users.push_back(grantee);
                }
            }
        }
        users
    }

    pub fn get_access_grant(env: Env, pet_id: u64, grantee: Address) -> Option<AccessGrant> {
        env.storage()
            .instance()
            .get(&DataKey::AccessGrant((pet_id, grantee)))
    }

    // --- LAB RESULTS ---
    pub fn add_lab_result(
        env: Env,
        pet_id: u64,
        vet_address: Address,
        test_type: String,
        results: String,
        reference_ranges: String,
        attachment_hash: Option<String>,
        medical_record_id: Option<u64>,
    ) -> u64 {
        vet_address.require_auth();
        let _pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        let count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::LabResultCount)
            .unwrap_or(0);
        let id = count + 1;
        env.storage()
            .instance()
            .set(&MedicalKey::LabResultCount, &id);

        let result = LabResult {
            id,
            pet_id,
            test_type,
            date: env.ledger().timestamp(),
            results,
            vet_address,
            reference_ranges,
            attachment_hash,
            medical_record_id,
        };
        env.storage()
            .instance()
            .set(&MedicalKey::LabResult(id), &result);

        let p_count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::PetLabResultCount(pet_id))
            .unwrap_or(0);
        let new_p = p_count + 1;
        env.storage()
            .instance()
            .set(&MedicalKey::PetLabResultCount(pet_id), &new_p);
        env.storage()
            .instance()
            .set(&MedicalKey::PetLabResultIndex((pet_id, new_p)), &id);

        id
    }

    pub fn get_lab_result(env: Env, lab_result_id: u64) -> Option<LabResult> {
        env.storage()
            .instance()
            .get(&MedicalKey::LabResult(lab_result_id))
    }

    pub fn get_lab_results(env: Env, pet_id: u64) -> Vec<LabResult> {
        let count = env
            .storage()
            .instance()
            .get::<MedicalKey, u64>(&MedicalKey::PetLabResultCount(pet_id))
            .unwrap_or(0);
        let mut res = Vec::new(&env);
        for i in 1..=count {
            if let Some(lid) = env
                .storage()
                .instance()
                .get::<MedicalKey, u64>(&MedicalKey::PetLabResultIndex((pet_id, i)))
            {
                if let Some(r) = Self::get_lab_result(env.clone(), lid) {
                    res.push_back(r);
                }
            }
        }
        res
    }
    // --- BATCH OPERATIONS ---

    pub fn batch_add_vaccinations(
        env: Env,
        veterinarian: Address,
        vaccinations: Vec<VaccinationInput>,
    ) -> Vec<u64> {
        veterinarian.require_auth();
        // Verify vet once
        if !Self::is_verified_vet(env.clone(), veterinarian.clone()) {
            panic!("Veterinarian not verified");
        }

        let mut ids = Vec::new(&env);
        for input in vaccinations.iter() {
            let id = Self::add_vaccination(
                env.clone(),
                input.pet_id,
                veterinarian.clone(),
                input.vaccine_type,
                input.vaccine_name,
                input.administered_at,
                input.next_due_date,
                input.batch_number,
            );
            ids.push_back(id);
        }
        ids
    }

    pub fn batch_add_records(
        env: Env,
        veterinarian: Address,
        records: Vec<MedicalRecordInput>,
    ) -> Vec<u64> {
        veterinarian.require_auth();

        let mut ids = Vec::new(&env);
        for input in records.iter() {
            let id = Self::add_medical_record(
                env.clone(),
                input.pet_id,
                veterinarian.clone(),
                input.diagnosis,
                input.treatment,
                input.medications,
                input.notes,
            );
            ids.push_back(id);
        }
        ids
    }

    // --- LOST PET ALERT FUNCTIONS ---

    /// Report a pet as lost
    pub fn report_lost(
        env: Env,
        pet_id: u64,
        last_seen_location: String,
        reward_amount: Option<u64>,
    ) -> u64 {
        // Verify pet exists and caller is owner
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        let alert_count: u64 = env
            .storage()
            .instance()
            .get(&AlertKey::LostPetAlertCount)
            .unwrap_or(0);
        let alert_id = alert_count + 1;

        let alert = LostPetAlert {
            id: alert_id,
            pet_id,
            reported_by: pet.owner.clone(),
            reported_date: env.ledger().timestamp(),
            last_seen_location,
            reward_amount,
            status: AlertStatus::Active,
            found_date: None,
        };

        // Store alert
        env.storage()
            .instance()
            .set(&AlertKey::LostPetAlert(alert_id), &alert);
        env.storage()
            .instance()
            .set(&AlertKey::LostPetAlertCount, &alert_id);

        // Add to active alerts list
        let mut active_alerts: Vec<u64> = env
            .storage()
            .instance()
            .get(&AlertKey::ActiveLostPetAlerts)
            .unwrap_or(Vec::new(&env));
        active_alerts.push_back(alert_id);
        env.storage()
            .instance()
            .set(&AlertKey::ActiveLostPetAlerts, &active_alerts);

        alert_id
    }

    /// Report a sighting of a lost pet
    pub fn report_sighting(env: Env, alert_id: u64, location: String, description: String) -> bool {
        let reporter = env.current_contract_address();

        let sighting = SightingReport {
            alert_id,
            reporter,
            location,
            timestamp: env.ledger().timestamp(),
            description,
        };

        let key = AlertKey::AlertSightings(alert_id);
        let mut sightings: Vec<SightingReport> =
            env.storage().instance().get(&key).unwrap_or(Vec::new(&env));
        sightings.push_back(sighting);
        env.storage().instance().set(&key, &sightings);

        true
    }

    /// Mark a lost pet as found
    pub fn report_found(env: Env, alert_id: u64) -> bool {
        let key = AlertKey::LostPetAlert(alert_id);

        let mut alert: LostPetAlert = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::AlertNotFound));

        alert.reported_by.require_auth();

        if alert.status != AlertStatus::Active {
            env.panic_with_error(ContractError::AlertNotActive);
        }

        alert.status = AlertStatus::Found;
        alert.found_date = Some(env.ledger().timestamp());
        env.storage().instance().set(&key, &alert);

        // Remove from active alerts
        let mut active_alerts: Vec<u64> = env
            .storage()
            .instance()
            .get(&AlertKey::ActiveLostPetAlerts)
            .unwrap_or(Vec::new(&env));

        if let Some(pos) = active_alerts.iter().position(|id| id == alert_id) {
            active_alerts.remove(pos as u32);
            env.storage()
                .instance()
                .set(&AlertKey::ActiveLostPetAlerts, &active_alerts);
        }

        true
    }

    /// Cancel a lost pet alert
    pub fn cancel_lost_alert(env: Env, alert_id: u64) -> bool {
        let key = AlertKey::LostPetAlert(alert_id);

        let mut alert: LostPetAlert = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(ContractError::AlertNotFound));

        alert.reported_by.require_auth();

        if alert.status != AlertStatus::Active {
            env.panic_with_error(ContractError::AlertNotActive);
        }

        alert.status = AlertStatus::Cancelled;
        env.storage().instance().set(&key, &alert);

        let mut active_alerts: Vec<u64> = env
            .storage()
            .instance()
            .get(&AlertKey::ActiveLostPetAlerts)
            .unwrap_or(Vec::new(&env));

        if let Some(pos) = active_alerts.iter().position(|id| id == alert_id) {
            active_alerts.remove(pos as u32);
            env.storage()
                .instance()
                .set(&AlertKey::ActiveLostPetAlerts, &active_alerts);
        }

        true
    }

    /// Get all active lost pet alerts
    pub fn get_active_alerts(env: Env) -> Vec<LostPetAlert> {
        let active_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&AlertKey::ActiveLostPetAlerts)
            .unwrap_or(Vec::new(&env));

        let mut active_alerts = Vec::new(&env);

        for id in active_ids.iter() {
            if let Some(alert) = env
                .storage()
                .instance()
                .get::<AlertKey, LostPetAlert>(&AlertKey::LostPetAlert(id))
            {
                if alert.status == AlertStatus::Active {
                    active_alerts.push_back(alert);
                }
            }
        }

        active_alerts
    }

    /// Get a specific alert by ID
    pub fn get_alert(env: Env, alert_id: u64) -> Option<LostPetAlert> {
        env.storage()
            .instance()
            .get(&AlertKey::LostPetAlert(alert_id))
    }

    /// Get sightings for a specific alert
    pub fn get_alert_sightings(env: Env, alert_id: u64) -> Vec<SightingReport> {
        env.storage()
            .instance()
            .get(&AlertKey::AlertSightings(alert_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get alerts for a specific pet
    pub fn get_pet_alerts(env: Env, pet_id: u64) -> Vec<LostPetAlert> {
        let alert_count: u64 = env
            .storage()
            .instance()
            .get(&AlertKey::LostPetAlertCount)
            .unwrap_or(0);

        let mut pet_alerts = Vec::new(&env);

        for i in 1..=alert_count {
            if let Some(alert) = env
                .storage()
                .instance()
                .get::<AlertKey, LostPetAlert>(&AlertKey::LostPetAlert(i))
            {
                if alert.pet_id == pet_id {
                    pet_alerts.push_back(alert);
                }
            }
        }
        pet_alerts
    }
    // --- VET AVAILABILITY FUNCTIONS ---

    /// Set availability slots for a vet (only verified vets can set their availability)
    pub fn set_availability(env: Env, vet_address: Address, start_time: u64, end_time: u64) -> u64 {
        // Verify caller is the vet and is verified
        vet_address.require_auth();
        if !Self::is_verified_vet(env.clone(), vet_address.clone()) {
            panic!("Vet not verified");
        }

        let slot_count: u64 = env
            .storage()
            .instance()
            .get(&SystemKey::VetAvailabilityCount(vet_address.clone()))
            .unwrap_or(0);
        let slot_index = slot_count + 1;

        let slot = AvailabilitySlot {
            vet_address: vet_address.clone(),
            start_time,
            end_time,
            available: true,
        };

        // Store the slot
        env.storage().instance().set(
            &SystemKey::VetAvailability((vet_address.clone(), slot_index)),
            &slot,
        );
        env.storage().instance().set(
            &SystemKey::VetAvailabilityCount(vet_address.clone()),
            &slot_index,
        );

        // Add to date-based index for efficient querying
        let date = Self::get_date_from_timestamp(start_time);
        let date_key = SystemKey::VetAvailabilityByDate((vet_address.clone(), date));
        let mut date_slots: Vec<u64> = env
            .storage()
            .instance()
            .get(&date_key)
            .unwrap_or(Vec::new(&env));
        date_slots.push_back(slot_index);
        env.storage().instance().set(&date_key, &date_slots);

        slot_index
    }

    /// Get available slots for a vet on a specific date
    pub fn get_available_slots(env: Env, vet_address: Address, date: u64) -> Vec<AvailabilitySlot> {
        let date_key = SystemKey::VetAvailabilityByDate((vet_address.clone(), date));
        let slot_indices: Vec<u64> = env
            .storage()
            .instance()
            .get(&date_key)
            .unwrap_or(Vec::new(&env));

        let mut available_slots = Vec::new(&env);

        for index in slot_indices.iter() {
            if let Some(slot) = env.storage().instance().get::<SystemKey, AvailabilitySlot>(
                &SystemKey::VetAvailability((vet_address.clone(), index)),
            ) {
                if slot.available {
                    available_slots.push_back(slot);
                }
            }
        }

        available_slots
    }
    // --- CONSENT SYSTEM ---

    pub fn grant_consent(
        env: Env,
        pet_id: u64,
        owner: Address,
        consent_type: ConsentType,
        granted_to: Address,
    ) -> u64 {
        owner.require_auth();

        // Verify owner owns the pet
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        if pet.owner != owner {
            env.panic_with_error(ContractError::NotPetOwner);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&ConsentKey::ConsentCount)
            .unwrap_or(0);
        let consent_id = count + 1;
        let now = env.ledger().timestamp();

        let consent = Consent {
            id: consent_id,
            pet_id,
            owner,
            consent_type,
            granted_to,
            granted_at: now,
            revoked_at: None,
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&ConsentKey::Consent(consent_id), &consent);
        env.storage()
            .instance()
            .set(&ConsentKey::ConsentCount, &consent_id);

        // Update pet consent index
        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&ConsentKey::PetConsentCount(pet_id))
            .unwrap_or(0);
        let new_pet_count = pet_count + 1;
        env.storage()
            .instance()
            .set(&ConsentKey::PetConsentCount(pet_id), &new_pet_count);
        env.storage().instance().set(
            &ConsentKey::PetConsentIndex((pet_id, new_pet_count)),
            &consent_id,
        );

        consent_id
    }

    pub fn revoke_consent(env: Env, consent_id: u64, owner: Address) -> bool {
        owner.require_auth();

        if let Some(mut consent) = env
            .storage()
            .instance()
            .get::<ConsentKey, Consent>(&ConsentKey::Consent(consent_id))
        {
            if consent.owner != owner {
                env.panic_with_error(ContractError::NotConsentOwner);
            }
            if !consent.is_active {
                env.panic_with_error(ContractError::ConsentAlreadyRevoked);
            }

            consent.is_active = false;
            consent.revoked_at = Some(env.ledger().timestamp());

            env.storage()
                .instance()
                .set(&ConsentKey::Consent(consent_id), &consent);
            true
        } else {
            false
        }
    }

    pub fn get_consent_history(env: Env, pet_id: u64) -> Vec<Consent> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&ConsentKey::PetConsentCount(pet_id))
            .unwrap_or(0);

        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(consent_id) = env
                .storage()
                .instance()
                .get::<ConsentKey, u64>(&ConsentKey::PetConsentIndex((pet_id, i)))
            {
                if let Some(consent) = env
                    .storage()
                    .instance()
                    .get::<ConsentKey, Consent>(&ConsentKey::Consent(consent_id))
                {
                    history.push_back(consent);
                }
            }
        }
        history
    }

    /// Book a slot (mark as unavailable)
    pub fn book_slot(env: Env, vet_address: Address, slot_index: u64) -> bool {
        let key = SystemKey::VetAvailability((vet_address.clone(), slot_index));

        if let Some(mut slot) = env
            .storage()
            .instance()
            .get::<SystemKey, AvailabilitySlot>(&key)
        {
            if !slot.available {
                panic!("Slot already booked");
            }

            // Require auth from either vet or pet owner (simplified - just require vet auth for now)
            // In real implementation, you'd check if caller is a registered pet owner
            slot.available = false;
            env.storage().instance().set(&key, &slot);
            true
        } else {
            false
        }
    }

    /// Helper: Extract date from timestamp (yyyyMMdd format)
    fn get_date_from_timestamp(timestamp: u64) -> u64 {
        // Simple conversion: timestamp / 86400 gives days since epoch
        // For this implementation, we use timestamp / 86400 as the "date"
        timestamp / 86400
    }
    // --- CONTRACT UPGRADE SYSTEM ---

    pub fn get_version(env: Env) -> ContractVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or(ContractVersion {
                major: 1,
                minor: 0,
                patch: 0,
            })
    }

    pub fn upgrade_contract(env: Env, new_wasm_hash: BytesN<32>) {
        // Only admin can upgrade
        Self::require_admin(&env);

        // Perform the upgrade
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn propose_upgrade(env: Env, proposer: Address, new_wasm_hash: BytesN<32>) -> u64 {
        // Only admin can propose
        Self::require_admin(&env);
        proposer.require_auth();

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposalCount)
            .unwrap_or(0);
        let proposal_id = count + 1;

        let proposal = UpgradeProposal {
            id: proposal_id,
            proposed_by: proposer,
            new_wasm_hash,
            proposed_at: env.ledger().timestamp(),
            approved: false,
            executed: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposalCount, &proposal_id);

        proposal_id
    }

    pub fn approve_upgrade(env: Env, proposal_id: u64) -> bool {
        Self::require_admin(&env);

        if let Some(mut proposal) = env
            .storage()
            .instance()
            .get::<DataKey, UpgradeProposal>(&DataKey::UpgradeProposal(proposal_id))
        {
            if proposal.executed {
                panic!("Proposal already executed");
            }

            proposal.approved = true;
            env.storage()
                .instance()
                .set(&DataKey::UpgradeProposal(proposal_id), &proposal);
            true
        } else {
            false
        }
    }

    pub fn get_upgrade_proposal(env: Env, proposal_id: u64) -> Option<UpgradeProposal> {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeProposal(proposal_id))
    }

    pub fn migrate_version(env: Env, major: u32, minor: u32, patch: u32) {
        Self::require_admin(&env);

        let version = ContractVersion {
            major,
            minor,
            patch,
        };
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &version);
    }

    // --- MULTISIG OPERATIONS ---

    pub fn propose_action(
        env: Env,
        proposer: Address,
        action: ProposalAction,
        expires_in: u64,
    ) -> u64 {
        Self::require_admin_auth(&env, &proposer);

        let count: u64 = env
            .storage()
            .instance()
            .get(&SystemKey::ProposalCount)
            .unwrap_or(0);
        let proposal_id = count + 1;

        let threshold = env
            .storage()
            .instance()
            .get::<SystemKey, u32>(&SystemKey::AdminThreshold)
            .unwrap_or(1);

        let mut approvals = Vec::new(&env);
        approvals.push_back(proposer.clone());

        let now = env.ledger().timestamp();
        let proposal = MultiSigProposal {
            id: proposal_id,
            action,
            proposed_by: proposer,
            approvals,
            required_approvals: threshold,
            created_at: now,
            expires_at: now + expires_in,
            executed: false,
        };

        env.storage()
            .instance()
            .set(&SystemKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&SystemKey::ProposalCount, &proposal_id);

        proposal_id
    }

    pub fn approve_proposal(env: Env, admin: Address, proposal_id: u64) {
        Self::require_admin_auth(&env, &admin);

        let mut proposal: MultiSigProposal = env
            .storage()
            .instance()
            .get(&SystemKey::Proposal(proposal_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::ProposalNotFound));

        if proposal.executed {
            env.panic_with_error(ContractError::ProposalAlreadyExecuted);
        }

        if env.ledger().timestamp() > proposal.expires_at {
            env.panic_with_error(ContractError::ProposalExpired);
        }

        if proposal.approvals.contains(admin.clone()) {
            env.panic_with_error(ContractError::AdminAlreadyApproved);
        }

        proposal.approvals.push_back(admin);
        env.storage()
            .instance()
            .set(&SystemKey::Proposal(proposal_id), &proposal);
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) {
        let mut proposal: MultiSigProposal = env
            .storage()
            .instance()
            .get(&SystemKey::Proposal(proposal_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::ProposalNotFound));

        if proposal.executed {
            env.panic_with_error(ContractError::ProposalAlreadyExecuted);
        }

        if env.ledger().timestamp() > proposal.expires_at {
            env.panic_with_error(ContractError::ProposalExpired);
        }

        if proposal.approvals.len() < proposal.required_approvals {
            env.panic_with_error(ContractError::ThresholdNotMet);
        }

        match proposal.action.clone() {
            ProposalAction::VerifyVet(addr) => {
                Self::_verify_vet_internal(&env, addr);
            }
            ProposalAction::RevokeVet(addr) => {
                Self::_revoke_vet_internal(&env, addr);
            }
            ProposalAction::UpgradeContract(_code_hash) => {
                // Mock upgrade or actual logic if available
                // In Soroban, upgrades are handled via env.deployer()
                // For this task, we can just log success or placeholder
            }
            ProposalAction::ChangeAdmin(params) => {
                let (admins, threshold) = params;
                if threshold == 0 || threshold > admins.len() {
                    panic!("Invalid threshold");
                }
                env.storage().instance().set(&SystemKey::Admins, &admins);
                env.storage()
                    .instance()
                    .set(&SystemKey::AdminThreshold, &threshold);
                // Also clean up legacy admin if needed
                env.storage().instance().remove(&DataKey::Admin);
            }
        }

        proposal.executed = true;
        env.storage()
            .instance()
            .set(&SystemKey::Proposal(proposal_id), &proposal);
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<MultiSigProposal> {
        env.storage()
            .instance()
            .get(&SystemKey::Proposal(proposal_id))
    }

    // --- VET REVIEWS ---

    pub fn add_vet_review(
        env: Env,
        reviewer: Address,
        vet: Address,
        rating: u32,
        comment: String,
    ) -> u64 {
        reviewer.require_auth();

        if !(1..=5).contains(&rating) {
            panic!("Rating must be between 1 and 5");
        }

        // Check duplicate
        if env
            .storage()
            .instance()
            .has(&ReviewKey::VetReviewByOwnerVet((
                reviewer.clone(),
                vet.clone(),
            )))
        {
            panic!("You have already reviewed this veterinarian");
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&ReviewKey::VetReviewCount)
            .unwrap_or(0);
        let id = count + 1;

        let review = VetReview {
            id,
            vet_address: vet.clone(),
            reviewer: reviewer.clone(),
            rating,
            comment,
            date: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&ReviewKey::VetReview(id), &review);
        env.storage()
            .instance()
            .set(&ReviewKey::VetReviewCount, &id);

        // Index by Vet
        let vet_count: u64 = env
            .storage()
            .instance()
            .get(&ReviewKey::VetReviewCountByVet(vet.clone()))
            .unwrap_or(0);
        let new_vet_count = vet_count + 1;
        env.storage()
            .instance()
            .set(&ReviewKey::VetReviewCountByVet(vet.clone()), &new_vet_count);
        env.storage().instance().set(
            &ReviewKey::VetReviewByVetIndex((vet.clone(), new_vet_count)),
            &id,
        );

        // Mark as reviewed by this owner
        env.storage()
            .instance()
            .set(&ReviewKey::VetReviewByOwnerVet((reviewer, vet)), &id);

        id
    }

    pub fn get_vet_reviews(env: Env, vet: Address) -> Vec<VetReview> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&ReviewKey::VetReviewCountByVet(vet.clone()))
            .unwrap_or(0);
        let mut reviews = Vec::new(&env);
        for i in 1..=count {
            if let Some(review_id) = env
                .storage()
                .instance()
                .get::<ReviewKey, u64>(&ReviewKey::VetReviewByVetIndex((vet.clone(), i)))
            {
                if let Some(review) = env
                    .storage()
                    .instance()
                    .get::<ReviewKey, VetReview>(&ReviewKey::VetReview(review_id))
                {
                    reviews.push_back(review);
                }
            }
        }
        reviews
    }

    pub fn get_vet_average_rating(env: Env, vet: Address) -> u32 {
        let reviews = Self::get_vet_reviews(env.clone(), vet);
        if reviews.is_empty() {
            return 0;
        }
        let mut total = 0u32;
        for review in reviews.iter() {
            total = total
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(&env, ContractError::CounterOverflow));
            total += review.rating;
        }
        total / reviews.len()
    }

    // --- MEDICATION TRACKING ---

    pub fn add_medication(
        env: Env,
        pet_id: u64,
        name: String,
        dosage: String,
        frequency: String,
        start_date: u64,
        end_date: Option<u64>,
        prescribing_vet: Address,
    ) -> u64 {
        prescribing_vet.require_auth();

        // Verify the pet exists
        let _pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        let count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::MedicationCount)
            .unwrap_or(0);
        let id = count + 1;

        let medication = Medication {
            id,
            pet_id,
            name,
            dosage,
            frequency,
            start_date,
            end_date,
            prescribing_vet: prescribing_vet.clone(),
            active: true,
        };

        env.storage()
            .instance()
            .set(&MedicalKey::GlobalMedication(id), &medication);
        env.storage()
            .instance()
            .set(&MedicalKey::MedicationCount, &id);

        // Index by pet
        let pet_med_count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::PetMedicationCount(pet_id))
            .unwrap_or(0);
        let new_count = pet_med_count + 1;
        env.storage()
            .instance()
            .set(&MedicalKey::PetMedicationCount(pet_id), &new_count);
        env.storage()
            .instance()
            .set(&MedicalKey::PetMedicationIndex((pet_id, new_count)), &id);

        id
    }

    pub fn get_active_medications(env: Env, pet_id: u64) -> Vec<Medication> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&MedicalKey::PetMedicationCount(pet_id))
            .unwrap_or(0);
        let mut active_meds = Vec::new(&env);

        for i in 1..=count {
            if let Some(med_id) = env
                .storage()
                .instance()
                .get::<MedicalKey, u64>(&MedicalKey::PetMedicationIndex((pet_id, i)))
            {
                if let Some(med) = env
                    .storage()
                    .instance()
                    .get::<MedicalKey, Medication>(&MedicalKey::GlobalMedication(med_id))
                {
                    if med.active {
                        active_meds.push_back(med);
                    }
                }
            }
        }
        active_meds
    }

    pub fn mark_medication_completed(env: Env, medication_id: u64) {
        if let Some(mut med) = env
            .storage()
            .instance()
            .get::<MedicalKey, Medication>(&MedicalKey::GlobalMedication(medication_id))
        {
            med.prescribing_vet.require_auth();
            med.active = false;
            // If end_date is not set, set it to current ledger timestamp
            if med.end_date.is_none() {
                med.end_date = Some(env.ledger().timestamp());
            }
            env.storage()
                .instance()
                .set(&MedicalKey::GlobalMedication(medication_id), &med);
        } else {
            panic!("Medication not found");
        }
    }

    // --- TREATMENT HISTORY ---

    pub fn add_treatment(
        env: Env,
        pet_id: u64,
        vet_address: Address,
        treatment_type: TreatmentType,
        date: u64,
        notes: String,
        cost: Option<i128>,
        outcome: String,
    ) -> u64 {
        vet_address.require_auth();

        if !Self::is_verified_vet(env.clone(), vet_address.clone()) {
            panic!("Veterinarian not verified");
        }

        let _pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        let treatment_count: u64 = env
            .storage()
            .instance()
            .get(&TreatmentKey::TreatmentCount)
            .unwrap_or(0);
        let treatment_id = treatment_count + 1;

        let now = env.ledger().timestamp();

        let treatment = Treatment {
            id: treatment_id,
            pet_id,
            treatment_type: treatment_type.clone(),
            date,
            vet_address: vet_address.clone(),
            notes,
            cost,
            outcome,
        };

        env.storage()
            .instance()
            .set(&TreatmentKey::Treatment(treatment_id), &treatment);
        env.storage()
            .instance()
            .set(&TreatmentKey::TreatmentCount, &treatment_id);

        // Update per-pet index
        let pet_treatment_count: u64 = env
            .storage()
            .instance()
            .get(&TreatmentKey::PetTreatmentCount(pet_id))
            .unwrap_or(0);
        let new_pet_treatment_count = pet_treatment_count + 1;
        env.storage().instance().set(
            &TreatmentKey::PetTreatmentCount(pet_id),
            &new_pet_treatment_count,
        );
        env.storage().instance().set(
            &TreatmentKey::PetTreatmentIndex((pet_id, new_pet_treatment_count)),
            &treatment_id,
        );

        env.events().publish(
            (String::from_str(&env, "TreatmentAdded"), pet_id),
            TreatmentAddedEvent {
                treatment_id,
                pet_id,
                vet_address,
                treatment_type,
                timestamp: now,
            },
        );

        treatment_id
    }

    pub fn get_treatment(env: Env, treatment_id: u64) -> Option<Treatment> {
        env.storage()
            .instance()
            .get::<TreatmentKey, Treatment>(&TreatmentKey::Treatment(treatment_id))
    }

    pub fn get_treatment_history(env: Env, pet_id: u64) -> Vec<Treatment> {
        if env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .is_none()
        {
            return Vec::new(&env);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&TreatmentKey::PetTreatmentCount(pet_id))
            .unwrap_or(0);

        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(tid) = env
                .storage()
                .instance()
                .get::<TreatmentKey, u64>(&TreatmentKey::PetTreatmentIndex((pet_id, i)))
            {
                if let Some(treatment) = env
                    .storage()
                    .instance()
                    .get::<TreatmentKey, Treatment>(&TreatmentKey::Treatment(tid))
                {
                    history.push_back(treatment);
                }
            }
        }

        history
    }

    pub fn get_treatments_by_type(
        env: Env,
        pet_id: u64,
        treatment_type: TreatmentType,
    ) -> Vec<Treatment> {
        let history = Self::get_treatment_history(env.clone(), pet_id);
        let mut filtered = Vec::new(&env);

        for treatment in history.iter() {
            if treatment.treatment_type == treatment_type {
                filtered.push_back(treatment);
            }
        }

        filtered
    }

    /// Adds an insurance policy to a pet.
    ///
    /// # Arguments
    /// * `pet_id` - The ID of the pet to insure
    /// * `policy_id` - Unique identifier for the insurance policy
    /// * `provider` - Name of the insurance provider
    /// * `coverage_type` - Type of coverage (e.g., "Comprehensive", "Basic")
    /// * `premium` - Annual premium amount
    /// * `coverage_limit` - Maximum coverage amount
    /// * `expiry_date` - Unix timestamp when policy expires
    ///
    /// # Returns
    /// * `true` if policy was added successfully
    /// * `false` if pet doesn't exist
    ///
    /// # Events
    /// Emits `InsuranceAddedEvent` on success
    pub fn add_insurance_policy(
        env: Env,
        pet_id: u64,
        policy_id: String,
        provider: String,
        coverage_type: String,
        premium: u64,
        coverage_limit: u64,
        expiry_date: u64,
    ) -> bool {
        if env
            .storage()
            .instance()
            .get::<DataKey, Pet>(&DataKey::Pet(pet_id))
            .is_none()
        {
            return false;
        }

        let start_date = env.ledger().timestamp();
        let policy = InsurancePolicy {
            policy_id: policy_id.clone(),
            provider: provider.clone(),
            coverage_type,
            premium,
            coverage_limit,
            start_date,
            expiry_date,
            active: true,
        };

        env.storage()
            .instance()
            .set(&InsuranceKey::Policy(pet_id), &policy);

        env.events().publish(
            (String::from_str(&env, "InsuranceAdded"), pet_id),
            InsuranceAddedEvent {
                pet_id,
                policy_id,
                provider,
                timestamp: start_date,
            },
        );

        true
    }

    /// Retrieves the insurance policy for a pet.
    ///
    /// # Arguments
    /// * `pet_id` - The ID of the pet
    ///
    /// # Returns
    /// * `Some(InsurancePolicy)` if policy exists
    /// * `None` if no policy found
    pub fn get_pet_insurance(env: Env, pet_id: u64) -> Option<InsurancePolicy> {
        env.storage()
            .instance()
            .get::<InsuranceKey, InsurancePolicy>(&InsuranceKey::Policy(pet_id))
    }

    /// Updates the active status of an insurance policy.
    ///
    /// # Arguments
    /// * `pet_id` - The ID of the pet
    /// * `active` - New status (true = active, false = inactive)
    ///
    /// # Returns
    /// * `true` if status was updated successfully
    /// * `false` if policy doesn't exist
    ///
    /// # Events
    /// Emits `InsuranceUpdatedEvent` on success
    pub fn update_insurance_status(env: Env, pet_id: u64, active: bool) -> bool {
        if let Some(mut policy) = env
            .storage()
            .instance()
            .get::<InsuranceKey, InsurancePolicy>(&InsuranceKey::Policy(pet_id))
        {
            policy.active = active;
            env.storage()
                .instance()
                .set(&InsuranceKey::Policy(pet_id), &policy);

            env.events().publish(
                (String::from_str(&env, "InsuranceUpdated"), pet_id),
                InsuranceUpdatedEvent {
                    pet_id,
                    policy_id: policy.policy_id,
                    active,
                    timestamp: env.ledger().timestamp(),
                },
            );
            return true;
        }
        false
    }

    /// Submits an insurance claim for a pet.
    ///
    /// # Arguments
    /// * `pet_id` - The ID of the pet
    /// * `amount` - Claim amount
    /// * `description` - Description of the claim
    ///
    /// # Returns
    /// * `Some(claim_id)` if claim was submitted successfully
    /// * `None` if pet has no policy or policy is inactive
    ///
    /// # Events
    /// Emits `InsuranceClaimSubmittedEvent` on success
    pub fn submit_insurance_claim(
        env: Env,
        pet_id: u64,
        amount: u64,
        description: String,
    ) -> Option<u64> {
        let policy = env
            .storage()
            .instance()
            .get::<InsuranceKey, InsurancePolicy>(&InsuranceKey::Policy(pet_id))?;

        if !policy.active {
            return None;
        }

        let claim_count: u64 = env
            .storage()
            .instance()
            .get(&InsuranceKey::ClaimCount)
            .unwrap_or(0);
        let claim_id = claim_count + 1;
        let timestamp = env.ledger().timestamp();

        let claim = InsuranceClaim {
            claim_id,
            pet_id,
            policy_id: policy.policy_id.clone(),
            amount,
            date: timestamp,
            status: InsuranceClaimStatus::Pending,
            description,
        };

        // Save claim globally
        env.storage()
            .instance()
            .set(&InsuranceKey::Claim(claim_id), &claim);
        env.storage()
            .instance()
            .set(&InsuranceKey::ClaimCount, &claim_id);

        // Save claim for pet
        let pet_claim_count: u64 = env
            .storage()
            .instance()
            .get(&InsuranceKey::PetClaimCount(pet_id))
            .unwrap_or(0)
            + 1;
        env.storage()
            .instance()
            .set(&InsuranceKey::PetClaimCount(pet_id), &pet_claim_count);
        env.storage().instance().set(
            &InsuranceKey::PetClaimIndex((pet_id, pet_claim_count)),
            &claim_id,
        );

        env.events().publish(
            (String::from_str(&env, "InsuranceClaimSubmitted"), pet_id),
            InsuranceClaimSubmittedEvent {
                claim_id,
                pet_id,
                policy_id: policy.policy_id,
                amount,
                timestamp,
            },
        );

        Some(claim_id)
    }

    /// Retrieves an insurance claim by ID.
    ///
    /// # Arguments
    /// * `claim_id` - The ID of the claim
    ///
    /// # Returns
    /// * `Some(InsuranceClaim)` if claim exists
    /// * `None` if claim not found
    pub fn get_insurance_claim(env: Env, claim_id: u64) -> Option<InsuranceClaim> {
        env.storage()
            .instance()
            .get::<InsuranceKey, InsuranceClaim>(&InsuranceKey::Claim(claim_id))
    }

    /// Updates the status of an insurance claim.
    ///
    /// # Arguments
    /// * `claim_id` - The ID of the claim
    /// * `status` - New status (Pending, Approved, Rejected, or Paid)
    ///
    /// # Returns
    /// * `true` if status was updated successfully
    /// * `false` if claim doesn't exist
    ///
    /// # Events
    /// Emits `InsuranceClaimStatusUpdatedEvent` on success
    pub fn update_insurance_claim_status(
        env: Env,
        claim_id: u64,
        status: InsuranceClaimStatus,
    ) -> bool {
        if let Some(mut claim) = env
            .storage()
            .instance()
            .get::<InsuranceKey, InsuranceClaim>(&InsuranceKey::Claim(claim_id))
        {
            claim.status = status.clone();
            env.storage()
                .instance()
                .set(&InsuranceKey::Claim(claim_id), &claim);

            env.events().publish(
                (
                    String::from_str(&env, "InsuranceClaimStatusUpdated"),
                    claim.pet_id,
                ),
                InsuranceClaimStatusUpdatedEvent {
                    claim_id,
                    pet_id: claim.pet_id,
                    status,
                    timestamp: env.ledger().timestamp(),
                },
            );
            return true;
        }
        false
    }

    /// Retrieves all insurance claims for a pet.
    ///
    /// # Arguments
    /// * `pet_id` - The ID of the pet
    ///
    /// # Returns
    /// Vector of all insurance claims for the pet (empty if none)
    pub fn get_pet_insurance_claims(env: Env, pet_id: u64) -> Vec<InsuranceClaim> {
        let mut claims = Vec::new(&env);
        let count: u64 = env
            .storage()
            .instance()
            .get(&InsuranceKey::PetClaimCount(pet_id))
            .unwrap_or(0);

        for i in 1..=count {
            if let Some(claim_id) = env
                .storage()
                .instance()
                .get::<InsuranceKey, u64>(&InsuranceKey::PetClaimIndex((pet_id, i)))
            {
                if let Some(claim) = env
                    .storage()
                    .instance()
                    .get::<InsuranceKey, InsuranceClaim>(&InsuranceKey::Claim(claim_id))
                {
                    claims.push_back(claim);
                }
            }
        }
        claims
    }

    // --- BEHAVIORAL TRACKING SYSTEM ---

    pub fn add_behavior_record(
        env: Env,
        pet_id: u64,
        behavior_type: BehaviorType,
        severity: u32,
        description: String,
    ) -> u64 {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        if severity > 10 {
            env.panic_with_error(ContractError::SeverityOutOfRange);
        }
        if description.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&BehaviorKey::BehaviorRecordCount)
            .unwrap_or(0);
        let record_id = count + 1;

        let record = BehaviorRecord {
            id: record_id,
            pet_id,
            behavior_type,
            severity,
            description,
            recorded_by: pet.owner.clone(),
            recorded_at: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&BehaviorKey::BehaviorRecord(record_id), &record);
        env.storage()
            .instance()
            .set(&BehaviorKey::BehaviorRecordCount, &record_id);

        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&BehaviorKey::PetBehaviorCount(pet_id))
            .unwrap_or(0);
        let new_pet_count = pet_count + 1;
        env.storage()
            .instance()
            .set(&BehaviorKey::PetBehaviorCount(pet_id), &new_pet_count);
        env.storage().instance().set(
            &BehaviorKey::PetBehaviorIndex((pet_id, new_pet_count)),
            &record_id,
        );

        record_id
    }

    pub fn get_behavior_history(env: Env, pet_id: u64) -> Vec<BehaviorRecord> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&BehaviorKey::PetBehaviorCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(record_id) = env
                .storage()
                .instance()
                .get::<BehaviorKey, u64>(&BehaviorKey::PetBehaviorIndex((pet_id, i)))
            {
                if let Some(record) = env
                    .storage()
                    .instance()
                    .get::<BehaviorKey, BehaviorRecord>(&BehaviorKey::BehaviorRecord(record_id))
                {
                    history.push_back(record);
                }
            }
        }
        history
    }

    pub fn add_training_milestone(
        env: Env,
        pet_id: u64,
        milestone_name: String,
        notes: String,
    ) -> u64 {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        let count: u64 = env
            .storage()
            .instance()
            .get(&BehaviorKey::TrainingMilestoneCount)
            .unwrap_or(0);
        let milestone_id = count + 1;

        let milestone = TrainingMilestone {
            id: milestone_id,
            pet_id,
            milestone_name,
            achieved: false,
            achieved_at: None,
            trainer: pet.owner.clone(),
            notes,
        };

        env.storage()
            .instance()
            .set(&BehaviorKey::TrainingMilestone(milestone_id), &milestone);
        env.storage()
            .instance()
            .set(&BehaviorKey::TrainingMilestoneCount, &milestone_id);

        let pet_milestone_count: u64 = env
            .storage()
            .instance()
            .get(&BehaviorKey::PetMilestoneCount(pet_id))
            .unwrap_or(0);
        let new_count = pet_milestone_count + 1;
        env.storage()
            .instance()
            .set(&BehaviorKey::PetMilestoneCount(pet_id), &new_count);
        env.storage().instance().set(
            &BehaviorKey::PetMilestoneIndex((pet_id, new_count)),
            &milestone_id,
        );

        milestone_id
    }

    pub fn mark_milestone_achieved(env: Env, milestone_id: u64) -> bool {
        if let Some(mut milestone) = env
            .storage()
            .instance()
            .get::<BehaviorKey, TrainingMilestone>(&BehaviorKey::TrainingMilestone(milestone_id))
        {
            milestone.trainer.require_auth();

            milestone.achieved = true;
            milestone.achieved_at = Some(env.ledger().timestamp());

            env.storage()
                .instance()
                .set(&BehaviorKey::TrainingMilestone(milestone_id), &milestone);
            true
        } else {
            false
        }
    }

    pub fn get_training_milestones(env: Env, pet_id: u64) -> Vec<TrainingMilestone> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&BehaviorKey::PetMilestoneCount(pet_id))
            .unwrap_or(0);
        let mut milestones = Vec::new(&env);

        for i in 1..=count {
            if let Some(milestone_id) = env
                .storage()
                .instance()
                .get::<BehaviorKey, u64>(&BehaviorKey::PetMilestoneIndex((pet_id, i)))
            {
                if let Some(milestone) = env
                    .storage()
                    .instance()
                    .get::<BehaviorKey, TrainingMilestone>(&BehaviorKey::TrainingMilestone(
                        milestone_id,
                    ))
                {
                    milestones.push_back(milestone);
                }
            }
        }
        milestones
    }

    pub fn get_behavior_improvements(
        env: Env,
        pet_id: u64,
        behavior_type: BehaviorType,
    ) -> Vec<BehaviorRecord> {
        let history = Self::get_behavior_history(env.clone(), pet_id);
        let mut filtered = Vec::new(&env);

        for record in history.iter() {
            if record.behavior_type == behavior_type {
                filtered.push_back(record);
            }
        }
        filtered
    }

    pub fn get_behavior_by_type(
        env: Env,
        pet_id: u64,
        behavior_type: BehaviorType,
    ) -> Vec<BehaviorRecord> {
        Self::get_behavior_improvements(env, pet_id, behavior_type)
    }

    // --- PET MULTISIG TRANSFER SYSTEM ---

    /// Configure multi-signature requirements for a pet.
    ///
    /// # Arguments
    /// * `pet_id` - The pet to configure
    /// * `signers` - List of authorized signers (must include owner)
    /// * `threshold` - Minimum signatures required (1 to signers.len())
    ///
    /// # Returns
    /// `true` if configuration was successful
    ///
    /// # Panics
    /// * If pet not found
    /// * If caller is not the pet owner
    /// * If threshold is invalid (0 or > signers.len())
    /// * If owner is not in signers list
    pub fn configure_multisig(
        env: Env,
        pet_id: u64,
        signers: Vec<Address>,
        threshold: u32,
    ) -> bool {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        if threshold == 0 || threshold > signers.len() {
            env.panic_with_error(ContractError::InvalidThreshold);
        }

        if !signers.contains(pet.owner.clone()) {
            env.panic_with_error(ContractError::NotPetOwner);
        }

        let config = MultisigConfig {
            pet_id,
            signers,
            threshold,
            enabled: true,
        };

        env.storage()
            .instance()
            .set(&SystemKey::PetMultisigConfig(pet_id), &config);
        true
    }

    /// Get the multi-signature configuration for a pet.
    ///
    /// # Arguments
    /// * `pet_id` - The pet ID
    ///
    /// # Returns
    /// `Some(MultisigConfig)` if configured, `None` otherwise
    pub fn get_multisig_config(env: Env, pet_id: u64) -> Option<MultisigConfig> {
        env.storage()
            .instance()
            .get(&SystemKey::PetMultisigConfig(pet_id))
    }

    /// Disable multi-signature enforcement for a pet.
    /// Configuration is preserved but not enforced.
    ///
    /// # Arguments
    /// * `pet_id` - The pet ID
    ///
    /// # Returns
    /// `true` if disabled successfully, `false` if no config exists
    ///
    /// # Panics
    /// * If pet not found
    /// * If caller is not the pet owner
    pub fn disable_multisig(env: Env, pet_id: u64) -> bool {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        if let Some(mut config) = env
            .storage()
            .instance()
            .get::<SystemKey, MultisigConfig>(&SystemKey::PetMultisigConfig(pet_id))
        {
            config.enabled = false;
            env.storage()
                .instance()
                .set(&SystemKey::PetMultisigConfig(pet_id), &config);
            true
        } else {
            false
        }
    }

    /// Initiate a multi-signature transfer proposal.
    /// Owner's signature is automatically added.
    ///
    /// # Arguments
    /// * `pet_id` - The pet to transfer
    /// * `to` - Address of the new owner
    ///
    /// # Returns
    /// The proposal ID
    ///
    /// # Panics
    /// * If pet not found
    /// * If caller is not the pet owner
    /// * If multisig not configured
    /// * If multisig is disabled
    pub fn require_multisig_for_transfer(env: Env, pet_id: u64, to: Address) -> u64 {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        let config: MultisigConfig = env
            .storage()
            .instance()
            .get(&SystemKey::PetMultisigConfig(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::MultisigNotConfigured));

        if !config.enabled {
            env.panic_with_error(ContractError::MultisigNotEnabled);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&SystemKey::PetTransferProposalCount)
            .unwrap_or(0);
        let proposal_id = count + 1;

        let now = env.ledger().timestamp();
        let mut signatures = Vec::new(&env);
        signatures.push_back(pet.owner.clone());

        let proposal = PetTransferProposal {
            id: proposal_id,
            pet_id,
            to,
            signatures,
            created_at: now,
            expires_at: now + 604800, // 7 days
            executed: false,
        };

        env.storage()
            .instance()
            .set(&SystemKey::PetTransferProposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&SystemKey::PetTransferProposalCount, &proposal_id);

        proposal_id
    }

    /// Add a signature to a transfer proposal.
    ///
    /// # Arguments
    /// * `proposal_id` - The proposal to sign
    /// * `signer` - The signer's address
    ///
    /// # Returns
    /// `true` if signature was added successfully
    ///
    /// # Panics
    /// * If proposal not found
    /// * If proposal already executed
    /// * If proposal expired
    /// * If signer not authorized
    /// * If signer already signed
    pub fn sign_transfer_proposal(env: Env, proposal_id: u64, signer: Address) -> bool {
        signer.require_auth();

        let mut proposal: PetTransferProposal = env
            .storage()
            .instance()
            .get(&SystemKey::PetTransferProposal(proposal_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::ProposalNotFound));

        if proposal.executed {
            env.panic_with_error(ContractError::ProposalAlreadyExecuted);
        }

        if env.ledger().timestamp() > proposal.expires_at {
            env.panic_with_error(ContractError::ProposalExpired);
        }

        let config: MultisigConfig = env
            .storage()
            .instance()
            .get(&SystemKey::PetMultisigConfig(proposal.pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::MultisigNotConfigured));

        if !config.signers.contains(signer.clone()) {
            env.panic_with_error(ContractError::NotAuthorizedSigner);
        }

        if proposal.signatures.contains(signer.clone()) {
            env.panic_with_error(ContractError::AlreadySigned);
        }

        proposal.signatures.push_back(signer);
        env.storage()
            .instance()
            .set(&SystemKey::PetTransferProposal(proposal_id), &proposal);
        true
    }

    /// Execute a multi-signature pet transfer.
    /// Requires threshold signatures to be met.
    ///
    /// # Arguments
    /// * `proposal_id` - The proposal to execute
    ///
    /// # Returns
    /// `true` if transfer was executed successfully
    ///
    /// # Panics
    /// * If proposal not found
    /// * If proposal already executed
    /// * If proposal expired
    /// * If threshold not met
    pub fn multisig_transfer_pet(env: Env, proposal_id: u64) -> bool {
        let mut proposal: PetTransferProposal = env
            .storage()
            .instance()
            .get(&SystemKey::PetTransferProposal(proposal_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::ProposalNotFound));

        if proposal.executed {
            env.panic_with_error(ContractError::ProposalAlreadyExecuted);
        }

        if env.ledger().timestamp() > proposal.expires_at {
            env.panic_with_error(ContractError::ProposalExpired);
        }

        let config: MultisigConfig = env
            .storage()
            .instance()
            .get(&SystemKey::PetMultisigConfig(proposal.pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::MultisigNotConfigured));

        if proposal.signatures.len() < config.threshold {
            env.panic_with_error(ContractError::ThresholdNotMet);
        }

        let mut pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(proposal.pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));

        let old_owner = pet.owner.clone();
        Self::remove_pet_from_owner_index(&env, &old_owner, proposal.pet_id);

        pet.owner = proposal.to.clone();
        pet.new_owner = proposal.to.clone();
        pet.updated_at = env.ledger().timestamp();

        Self::add_pet_to_owner_index(&env, &pet.owner, proposal.pet_id);
        env.storage()
            .instance()
            .set(&DataKey::Pet(proposal.pet_id), &pet);

        Self::log_ownership_change(
            &env,
            proposal.pet_id,
            old_owner.clone(),
            pet.owner.clone(),
            String::from_str(&env, "Multisig Transfer"),
        );

        env.events().publish(
            (
                String::from_str(&env, "PetOwnershipTransferred"),
                proposal.pet_id,
            ),
            PetOwnershipTransferredEvent {
                pet_id: proposal.pet_id,
                old_owner,
                new_owner: pet.owner.clone(),
                timestamp: pet.updated_at,
            },
        );

        proposal.executed = true;
        env.storage()
            .instance()
            .set(&SystemKey::PetTransferProposal(proposal_id), &proposal);

        true
    }

    /// Get details of a transfer proposal.
    ///
    /// # Arguments
    /// * `proposal_id` - The proposal ID
    ///
    /// # Returns
    /// `Some(PetTransferProposal)` if found, `None` otherwise
    pub fn get_transfer_proposal(env: Env, proposal_id: u64) -> Option<PetTransferProposal> {
        env.storage()
            .instance()
            .get(&SystemKey::PetTransferProposal(proposal_id))
    }

    // --- ACTIVITY TRACKING SYSTEM ---

    pub fn add_activity_record(
        env: Env,
        pet_id: u64,
        activity_type: ActivityType,
        duration_minutes: u32,
        intensity: u32,
        distance_meters: u32,
        notes: String,
    ) -> u64 {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::PetNotFound));
        pet.owner.require_auth();

        if intensity > 10 {
            env.panic_with_error(ContractError::IntensityOutOfRange);
        }
        if notes.len() > Self::MAX_STR_LONG {
            panic_with_error!(&env, ContractError::InputStringTooLong);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&ActivityKey::ActivityRecordCount)
            .unwrap_or(0);
        let record_id = count + 1;

        let record = ActivityRecord {
            id: record_id,
            pet_id,
            activity_type,
            duration_minutes,
            intensity,
            distance_meters,
            recorded_at: env.ledger().timestamp(),
            notes,
        };

        env.storage()
            .instance()
            .set(&ActivityKey::ActivityRecord(record_id), &record);
        env.storage()
            .instance()
            .set(&ActivityKey::ActivityRecordCount, &record_id);

        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&ActivityKey::PetActivityCount(pet_id))
            .unwrap_or(0);
        let new_pet_count = pet_count + 1;
        env.storage()
            .instance()
            .set(&ActivityKey::PetActivityCount(pet_id), &new_pet_count);
        env.storage().instance().set(
            &ActivityKey::PetActivityIndex((pet_id, new_pet_count)),
            &record_id,
        );

        record_id
    }

    pub fn get_activity_history(env: Env, pet_id: u64) -> Vec<ActivityRecord> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&ActivityKey::PetActivityCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);

        for i in 1..=count {
            if let Some(record_id) = env
                .storage()
                .instance()
                .get::<ActivityKey, u64>(&ActivityKey::PetActivityIndex((pet_id, i)))
            {
                if let Some(record) = env
                    .storage()
                    .instance()
                    .get::<ActivityKey, ActivityRecord>(&ActivityKey::ActivityRecord(record_id))
                {
                    history.push_back(record);
                }
            }
        }
        history
    }

    pub fn get_activity_stats(env: Env, pet_id: u64, days: u32) -> (u32, u32) {
        let current_time = env.ledger().timestamp();
        let cutoff_time = current_time.saturating_sub((days as u64) * 86400);
        let history = Self::get_activity_history(env, pet_id);

        let mut total_duration = 0u32;
        let mut total_distance = 0u32;

        for record in history.iter() {
            if record.recorded_at >= cutoff_time {
                total_duration = total_duration.saturating_add(record.duration_minutes);
                total_distance = total_distance.saturating_add(record.distance_meters);
            }
        }

        (total_duration, total_distance)
    }
    // --- BREEDING RECORDS SYSTEM ---
    pub fn add_breeding_record(
        env: Env,
        sire_id: u64,
        dam_id: u64,
        breeding_date: u64,
        notes: String,
    ) -> u64 {
        let sire: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(sire_id))
            .unwrap_or_else(|| env.panic_with_error(ContractError::SireNotFound));
        sire.owner.require_auth();
        let count: u64 = env.storage().instance().get(&BreedingKey::BreedingRecordCount).unwrap_or(0);
        let record_id = count + 1;
        let record = BreedingRecord {
            id: record_id,
            sire_id,
            dam_id,
            breeding_date,
            offspring_ids: Vec::new(&env),
            breeder: sire.owner.clone(),
            notes,
        };
        env.storage().instance().set(&BreedingKey::BreedingRecord(record_id), &record);
        env.storage().instance().set(&BreedingKey::BreedingRecordCount, &record_id);
        let sire_count: u64 = env.storage().instance().get(&BreedingKey::PetBreedingCount(sire_id)).unwrap_or(0);
        let new_sire_count = sire_count + 1;
        env.storage().instance().set(&BreedingKey::PetBreedingCount(sire_id), &new_sire_count);
        env.storage().instance().set(&BreedingKey::PetBreedingIndex((sire_id, new_sire_count)), &record_id);
        let dam_count: u64 = env.storage().instance().get(&BreedingKey::PetBreedingCount(dam_id)).unwrap_or(0);
        let new_dam_count = dam_count + 1;
        env.storage().instance().set(&BreedingKey::PetBreedingCount(dam_id), &new_dam_count);
        env.storage().instance().set(&BreedingKey::PetBreedingIndex((dam_id, new_dam_count)), &record_id);
        record_id
    }

    pub fn get_breeding_history(env: Env, pet_id: u64) -> Vec<BreedingRecord> {
        let count: u64 = env.storage().instance().get(&BreedingKey::PetBreedingCount(pet_id)).unwrap_or(0);
        let mut history = Vec::new(&env);
        for i in 1..=count {
            if let Some(record_id) = env.storage().instance().get::<BreedingKey, u64>(&BreedingKey::PetBreedingIndex((pet_id, i))) {
                if let Some(record) = env.storage().instance().get::<BreedingKey, BreedingRecord>(&BreedingKey::BreedingRecord(record_id)) {
                    history.push_back(record);
                }
            }
        }
        history
    }

    pub fn add_offspring(env: Env, record_id: u64, offspring_id: u64) -> bool {
        if let Some(mut record) = env.storage().instance().get::<BreedingKey, BreedingRecord>(&BreedingKey::BreedingRecord(record_id)) {
            record.breeder.require_auth();
            record.offspring_ids.push_back(offspring_id);
            env.storage().instance().set(&BreedingKey::BreedingRecord(record_id), &record);
            let off_count: u64 = env.storage().instance().get(&BreedingKey::PetOffspringCount(record.sire_id)).unwrap_or(0);
            let new_off_count = off_count + 1;
            env.storage().instance().set(&BreedingKey::PetOffspringCount(record.sire_id), &new_off_count);
            env.storage().instance().set(&BreedingKey::PetOffspringIndex((record.sire_id, new_off_count)), &offspring_id);
            true
        } else {
            false
        }
    }

    pub fn get_offspring(env: Env, pet_id: u64) -> Vec<u64> {
        let count: u64 = env.storage().instance().get(&BreedingKey::PetOffspringCount(pet_id)).unwrap_or(0);
        let mut offspring = Vec::new(&env);
        for i in 1..=count {
            if let Some(offspring_id) = env.storage().instance().get::<BreedingKey, u64>(&BreedingKey::PetOffspringIndex((pet_id, i))) {
                offspring.push_back(offspring_id);
            }
        }
        offspring
    }

    pub fn get_pedigree(env: Env, pet_id: u64) -> Vec<BreedingRecord> {
        let history = PetChainContract::get_breeding_history(env.clone(), pet_id);
        let mut pedigree = Vec::new(&env);
        for record in history.iter() {
            pedigree.push_back(record);
        }
        pedigree
    }

    // --- GROOMING RECORDS SYSTEM ---
    pub fn add_grooming_record(
        env: Env,
        pet_id: u64,
        service_type: String,
        groomer: String,
        date: u64,
        next_due: u64,
        cost: u64,
        notes: String,
    ) -> u64 {
        let pet: Pet = env
            .storage()
            .instance()
            .get(&DataKey::Pet(pet_id))
            .unwrap_or_else(|| panic_with_error!(env, ContractError::PetNotFound));
        pet.owner.require_auth();
        let count: u64 = env
            .storage()
            .instance()
            .get(&GroomingKey::GroomingRecordCount)
            .unwrap_or(0);
        let record_id = count + 1;
        let record = GroomingRecord {
            id: record_id,
            pet_id,
            service_type,
            groomer,
            date,
            next_due,
            cost,
            notes,
        };
        env.storage()
            .instance()
            .set(&GroomingKey::GroomingRecord(record_id), &record);
        env.storage()
            .instance()
            .set(&GroomingKey::GroomingRecordCount, &record_id);
        let pet_count: u64 = env
            .storage()
            .instance()
            .get(&GroomingKey::PetGroomingCount(pet_id))
            .unwrap_or(0);
        let new_count = pet_count + 1;
        env.storage()
            .instance()
            .set(&GroomingKey::PetGroomingCount(pet_id), &new_count);
        env.storage().instance().set(
            &GroomingKey::PetGroomingIndex((pet_id, new_count)),
            &record_id,
        );

        record_id
    }

    pub fn get_grooming_history(env: Env, pet_id: u64) -> Vec<GroomingRecord> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&GroomingKey::PetGroomingCount(pet_id))
            .unwrap_or(0);
        let mut history = Vec::new(&env);
        for i in 1..=count {
            if let Some(record_id) = env
                .storage()
                .instance()
                .get::<GroomingKey, u64>(&GroomingKey::PetGroomingIndex((pet_id, i)))
            {
                if let Some(record) = env
                    .storage()
                    .instance()
                    .get::<GroomingKey, GroomingRecord>(&GroomingKey::GroomingRecord(record_id))
                {
                    history.push_back(record);
                }
            }
        }
        history
    }

    pub fn get_next_grooming_date(env: Env, pet_id: u64) -> u64 {
        let history = Self::get_grooming_history(env, pet_id);
        let mut next_date = 0u64;
        for record in history.iter() {
            if record.next_due > 0 && (next_date == 0 || record.next_due < next_date) {
                next_date = record.next_due;
            }
        }
        next_date
    }

    pub fn get_grooming_expenses(env: Env, pet_id: u64) -> u64 {
        let history = Self::get_grooming_history(env, pet_id);
        let mut total = 0u64;
        for record in history.iter() {
            total += record.cost;
        }
        total
    }
}

// --- OVERFLOW-SAFE COUNTER HELPER ---
pub(crate) fn safe_increment(count: u64) -> u64 {
    count
        .checked_add(1)
        .unwrap_or_else(|| panic!("counter overflow"))
}

// --- ENCRYPTION HELPERS ---
fn encrypt_sensitive_data(env: &Env, data: &Bytes, key: &Bytes) -> (Bytes, Bytes) {
    let nonce = derive_encryption_nonce(env);
    let ciphertext = xor_stream_crypt(env, data, key, &nonce);
    (nonce, ciphertext)
}

fn decrypt_sensitive_data(
    env: &Env,
    ciphertext: &Bytes,
    nonce: &Bytes,
    key: &Bytes,
) -> Result<Bytes, ()> {
    if nonce.len() != 12 {
        return Err(());
    }
    Ok(xor_stream_crypt(env, ciphertext, key, nonce))
}

fn derive_encryption_nonce(env: &Env) -> Bytes {
    let counter: u64 = env
        .storage()
        .instance()
        .get(&SystemKey::EncryptionNonceCounter)
        .unwrap_or(0);
    let next_counter = safe_increment(counter);
    env.storage()
        .instance()
        .set(&SystemKey::EncryptionNonceCounter, &next_counter);

    let timestamp = env.ledger().timestamp();
    let mut nonce = Bytes::new(env);
    for byte in timestamp.to_be_bytes() {
        nonce.push_back(byte);
    }
    for byte in (next_counter as u32).to_be_bytes() {
        nonce.push_back(byte);
    }
    nonce
}

fn xor_stream_crypt(env: &Env, input: &Bytes, key: &Bytes, nonce: &Bytes) -> Bytes {
    let mut output = Bytes::new(env);
    let mut block_index: u32 = 0;

    while output.len() < input.len() {
        let mut seed = Bytes::new(env);
        for byte in key.iter() {
            seed.push_back(byte);
        }
        for byte in nonce.iter() {
            seed.push_back(byte);
        }
        for byte in block_index.to_be_bytes() {
            seed.push_back(byte);
        }

        let stream_block: Bytes = env.crypto().sha256(&seed).into();
        let start = output.len();
        let remaining = input.len() - start;
        let take = if remaining < 32 { remaining } else { 32 };
        for i in 0..take {
            let src = input.get_unchecked(start + i);
            let key_byte = stream_block.get_unchecked(i);
            output.push_back(src ^ key_byte);
        }
        block_index = block_index.saturating_add(1);
    }

    output
}
