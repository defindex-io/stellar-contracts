use soroban_sdk::{contracttype, Address, BytesN, Env};

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub asset: Address,
    pub admin: Address,
    pub keeper: Address,
}

#[contracttype]
pub enum DataKey {
    Config,
    TotalShares,
    /// Shares held by one depositor (normally one vault). Persistent.
    VaultPos(Address),
    /// Allowlisted depositor by exact address. Absent = not allowed.
    Depositor(Address),
    /// Allowlisted depositor by contract wasm hash. Absent = not allowed.
    AllowedHash(BytesN<32>),
}

pub const ONE_DAY_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP: u32 = 30 * ONE_DAY_LEDGERS;
const INSTANCE_THRESHOLD: u32 = INSTANCE_BUMP - ONE_DAY_LEDGERS;
const PERSISTENT_BUMP: u32 = 120 * ONE_DAY_LEDGERS;
const PERSISTENT_THRESHOLD: u32 = PERSISTENT_BUMP - 20 * ONE_DAY_LEDGERS;

pub fn extend_instance_ttl(e: &Env) {
    e.storage().instance().extend_ttl(INSTANCE_THRESHOLD, INSTANCE_BUMP);
}

pub fn get_config(e: &Env) -> Config {
    // Set in the constructor, so it always exists after deployment.
    e.storage().instance().get(&DataKey::Config).unwrap()
}

pub fn set_config(e: &Env, config: &Config) {
    e.storage().instance().set(&DataKey::Config, config);
}

pub fn get_total_shares(e: &Env) -> i128 {
    e.storage().instance().get(&DataKey::TotalShares).unwrap_or(0)
}

pub fn set_total_shares(e: &Env, shares: i128) {
    e.storage().instance().set(&DataKey::TotalShares, &shares);
}

/// Read without writing: `balance()` uses this so it stays a pure read.
pub fn peek_shares(e: &Env, owner: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::VaultPos(owner.clone()))
        .unwrap_or(0)
}

pub fn set_shares(e: &Env, owner: &Address, shares: i128) {
    let key = DataKey::VaultPos(owner.clone());
    if shares == 0 {
        // Delete rather than store zero so dead positions do not cost rent.
        e.storage().persistent().remove(&key);
    } else {
        e.storage().persistent().set(&key, &shares);
        e.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
    }
}

pub fn is_depositor(e: &Env, who: &Address) -> bool {
    e.storage().persistent().has(&DataKey::Depositor(who.clone()))
}

pub fn set_depositor(e: &Env, who: &Address, allowed: bool) {
    let key = DataKey::Depositor(who.clone());
    if allowed {
        e.storage().persistent().set(&key, &true);
        e.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
    } else {
        e.storage().persistent().remove(&key);
    }
}

pub fn is_allowed_hash(e: &Env, hash: &BytesN<32>) -> bool {
    e.storage().persistent().has(&DataKey::AllowedHash(hash.clone()))
}

pub fn set_allowed_hash(e: &Env, hash: &BytesN<32>, allowed: bool) {
    let key = DataKey::AllowedHash(hash.clone());
    if allowed {
        e.storage().persistent().set(&key, &true);
        e.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
    } else {
        e.storage().persistent().remove(&key);
    }
}
