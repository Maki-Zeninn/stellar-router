use crate::*;
use soroban_sdk::{Env, String};

#[test]
fn test_validate_ipfs_hash_v0_success() {
    let env = Env::default();
    let valid_v0 = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &valid_v0),
        Ok(())
    );
}

#[test]
fn test_validate_ipfs_hash_v1_success() {
    let env = Env::default();
    // CIDv1 base32
    let valid_v1 = String::from_str(
        &env,
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    );
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &valid_v1),
        Ok(())
    );
}

#[test]
fn test_validate_ipfs_hash_too_short() {
    let env = Env::default();
    let invalid = String::from_str(&env, "QmTooShort");
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &invalid),
        Err(ContractError::InvalidIpfsHash)
    );
}

#[test]
fn test_validate_ipfs_hash_v0_invalid_prefix() {
    let env = Env::default();
    // 46 chars but starts with Am
    let invalid = String::from_str(&env, "AmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &invalid),
        Err(ContractError::InvalidIpfsHash)
    );
}

#[test]
fn test_validate_ipfs_hash_v0_invalid_chars() {
    let env = Env::default();
    // 46 chars, starts with Qm, but contains '0' (invalid Base58)
    let invalid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbd0");
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &invalid),
        Err(ContractError::InvalidIpfsHash)
    );
}

#[test]
fn test_validate_ipfs_hash_v1_invalid_chars() {
    let env = Env::default();
    // Starts with b, but contains '1' (invalid Base32)
    let invalid = String::from_str(
        &env,
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzd1",
    );
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &invalid),
        Err(ContractError::InvalidIpfsHash)
    );
}

#[test]
fn test_validate_ipfs_hash_v1_too_short() {
    let env = Env::default();
    let invalid = String::from_str(&env, "b");
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &invalid),
        Err(ContractError::InvalidIpfsHash)
    );
}

#[test]
fn test_validate_ipfs_hash_v0_boundary_length() {
    let env = Env::default();
    let invalid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbd");
    assert_eq!(
        PetChainContract::validate_ipfs_hash(&env, &invalid),
        Err(ContractError::InvalidIpfsHash)
    );
}
