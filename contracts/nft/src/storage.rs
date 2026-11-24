use crate::NftKey;
use soroban_sdk::{Env, String};

/// Base URI applied to every token that has no dedicated metadata URI.
pub fn get_base_uri(env: &Env) -> Option<String> {
    env.storage().instance().get(&NftKey::BaseUri)
}

pub fn set_base_uri(env: &Env, base_uri: &String) {
    env.storage().instance().set(&NftKey::BaseUri, base_uri);
}

/// Per-token metadata URI; overrides the base URI for that token.
pub fn get_metadata_uri(env: &Env, token_id: u64) -> Option<String> {
    env.storage()
        .persistent()
        .get(&NftKey::MetadataUri(token_id))
}

pub fn set_metadata_uri(env: &Env, token_id: u64, uri: &String) {
    env.storage()
        .persistent()
        .set(&NftKey::MetadataUri(token_id), uri);
}
