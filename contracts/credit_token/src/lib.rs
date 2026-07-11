#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, BytesN, Env, String, Symbol,
    Val, Vec,
};

use soroban_sdk::IntoVal;

#[cfg(test)]
extern crate std;

// ── Events (max 9 chars for symbol_short) ──
const EVENT_MINTED: Symbol = symbol_short!("minted");
const EVENT_XFER: Symbol = symbol_short!("xfer");
const EVENT_RETIRED: Symbol = symbol_short!("retired");

// ── Data Types ──

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CreditMetadata {
    pub project_id: BytesN<32>,
    pub methodology: String,
    pub vintage: u64,
    pub issuance_date: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RetirementCertificate {
    pub retiree: Address,
    pub project_id: BytesN<32>,
    pub amount: i128,
    pub purpose: String,
    pub timestamp: u64,
    pub metadata_uri: String,
}

#[contracttype]
pub enum DataKey {
    Balance(Address),
    Allowance(Address, Address),
    Admin,
    Minter,
    RetirementRegistry,
    TotalSupply,
    TotalRetired,
    Name,
    Symbol,
    Decimals,
    Metadata,
    Cert(u64),
    CertCount,
    Paused,
    MaxSupply,
}

fn is_paused(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

fn require_not_paused(e: &Env) {
    if is_paused(e) {
        panic!("contract is paused");
    }
}

fn has_admin(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Admin)
}

fn read_admin(e: &Env) -> Address {
    e.storage().instance().get(&DataKey::Admin).unwrap()
}

fn read_minter(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Minter)
        .unwrap_or_else(|| read_admin(e))
}

fn require_minter(e: &Env, caller: &Address) {
    caller.require_auth();
    let minter = read_minter(e);
    let admin = read_admin(e);
    if *caller != minter && *caller != admin {
        panic!("unauthorized minter");
    }
}

fn read_balance(e: &Env, addr: &Address) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::Balance(addr.clone()))
        .unwrap_or(0)
}

fn save_balance(e: &Env, addr: &Address, amount: i128) {
    e.storage()
        .instance()
        .set(&DataKey::Balance(addr.clone()), &amount);
}

fn read_total_supply(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap()
}

fn save_total_supply(e: &Env, amount: i128) {
    e.storage()
        .instance()
        .set(&DataKey::TotalSupply, &amount);
}

fn read_total_retired(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::TotalRetired)
        .unwrap()
}

fn save_total_retired(e: &Env, amount: i128) {
    e.storage()
        .instance()
        .set(&DataKey::TotalRetired, &amount);
}

fn read_allowance(e: &Env, from: &Address, spender: &Address) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::Allowance(from.clone(), spender.clone()))
        .unwrap_or(0)
}

fn save_allowance(e: &Env, from: &Address, spender: &Address, amount: i128) {
    e.storage()
        .instance()
        .set(&DataKey::Allowance(from.clone(), spender.clone()), &amount);
}

#[contract]
pub struct CreditToken;

#[contractimpl]
impl CreditToken {
    /// Initialize the token with project metadata. Callable once by the deploying admin.
    pub fn initialize(
        e: Env,
        admin: Address,
        name: String,
        symbol: String,
        project_id: BytesN<32>,
        methodology: String,
    ) {
        if has_admin(&e) {
            panic!("already initialized");
        }
        let timestamp = e.ledger().timestamp();
        let metadata = CreditMetadata {
            project_id,
            methodology,
            vintage: timestamp,
            issuance_date: timestamp,
        };
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Name, &name);
        e.storage().instance().set(&DataKey::Symbol, &symbol);
        e.storage().instance().set(&DataKey::Decimals, &7u32);
        e.storage().instance().set(&DataKey::TotalSupply, &0i128);
        e.storage().instance().set(&DataKey::TotalRetired, &0i128);
        e.storage().instance().set(&DataKey::Metadata, &metadata);
        e.storage().instance().set(&DataKey::CertCount, &0u64);
    }

    /// Transfer contract admin rights to a new address.
    pub fn set_admin(e: Env, admin: Address, new_admin: Address) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Designate the address allowed to mint credits (typically the verification oracle).
    pub fn set_minter(e: Env, admin: Address, minter: Address) {
        admin.require_auth();
        if admin != read_admin(&e) {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Minter, &minter);
    }

    /// Link the global retirement registry for on-chain retirement recording.
    pub fn set_retirement_registry(e: Env, admin: Address, registry: Address) {
        admin.require_auth();
        if admin != read_admin(&e) {
            panic!("unauthorized");
        }
        e.storage()
            .instance()
            .set(&DataKey::RetirementRegistry, &registry);
    }

    /// Pause all token operations (mint, transfer, retire). Admin only.
    /// Useful for emergency halts or project suspension.
    pub fn pause(e: Env, admin: Address) {
        admin.require_auth();
        if admin != read_admin(&e) {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Paused, &true);
    }

    /// Resume token operations after a pause. Admin only.
    pub fn unpause(e: Env, admin: Address) {
        admin.require_auth();
        if admin != read_admin(&e) {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Paused, &false);
    }

    /// Returns true if the contract is currently paused.
    pub fn paused(e: Env) -> bool {
        is_paused(&e)
    }

    /// Set the maximum total supply for this token. Set to 0 to remove the cap.
    /// Admin only. Should be set once at project initialization to match the
    /// verified project area and methodology ceiling.
    pub fn set_max_supply(e: Env, admin: Address, max: i128) {
        admin.require_auth();
        if admin != read_admin(&e) {
            panic!("unauthorized");
        }
        if max < 0 {
            panic!("max supply must be non-negative");
        }
        e.storage().instance().set(&DataKey::MaxSupply, &max);
    }

    /// Get the configured maximum supply (0 = uncapped).
    pub fn max_supply(e: Env) -> i128 {
        e.storage()
            .instance()
            .get(&DataKey::MaxSupply)
            .unwrap_or(0)
    }

    /// Mint new credits to a beneficiary. Callable by admin or designated minter.
    pub fn mint_to(e: Env, minter: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        require_not_paused(&e);
        require_minter(&e, &minter);

        let total = read_total_supply(&e);
        let max: i128 = e.storage().instance().get(&DataKey::MaxSupply).unwrap_or(0);
        if max > 0 && total.checked_add(amount).expect("overflow") > max {
            panic!("max supply exceeded");
        }

        let balance = read_balance(&e, &to);
        save_balance(&e, &to, balance.checked_add(amount).expect("overflow"));
        save_total_supply(&e, total.checked_add(amount).expect("overflow"));

        e.events().publish((EVENT_MINTED,), (to, amount));
    }

    /// Mint credits to multiple recipients in a single call.
    /// Each entry in `recipients` receives the corresponding amount from `amounts`.
    /// The two slices must be the same length. Callable by admin or designated minter.
    pub fn batch_mint_to(e: Env, minter: Address, recipients: Vec<Address>, amounts: Vec<i128>) {
        if recipients.len() != amounts.len() {
            panic!("recipients and amounts length mismatch");
        }
        if recipients.len() == 0 {
            panic!("empty batch");
        }
        require_not_paused(&e);
        require_minter(&e, &minter);

        let mut total = read_total_supply(&e);
        let max: i128 = e.storage().instance().get(&DataKey::MaxSupply).unwrap_or(0);

        for i in 0..recipients.len() {
            let to = recipients.get(i).unwrap();
            let amount = amounts.get(i).unwrap();
            if amount <= 0 {
                panic!("amount must be positive");
            }
            if max > 0 && total.checked_add(amount).expect("overflow") > max {
                panic!("max supply exceeded");
            }
            let balance = read_balance(&e, &to);
            save_balance(&e, &to, balance.checked_add(amount).expect("overflow"));
            total = total.checked_add(amount).expect("overflow");
            e.events().publish((EVENT_MINTED,), (to, amount));
        }

        save_total_supply(&e, total);
    }

    /// Burn credits from a holder. Admin only.
    pub fn burn(e: Env, admin: Address, from: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }

        let balance = read_balance(&e, &from);
        let total = read_total_supply(&e);
        if balance < amount {
            panic!("insufficient balance");
        }
        save_balance(&e, &from, balance - amount);
        save_total_supply(&e, total - amount);
    }

    /// Transfer credits between wallets.
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        require_not_paused(&e);
        from.require_auth();

        let from_balance = read_balance(&e, &from);
        if from_balance < amount {
            panic!("insufficient balance");
        }
        let to_balance = read_balance(&e, &to);
        save_balance(&e, &from, from_balance - amount);
        save_balance(&e, &to, to_balance.checked_add(amount).expect("overflow"));

        e.events().publish((EVENT_XFER,), (from, to, amount));
    }

    /// Transfer credits on behalf of an approved holder.
    pub fn transfer_from(e: Env, spender: Address, from: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        require_not_paused(&e);
        spender.require_auth();

        let allowance = read_allowance(&e, &from, &spender);
        if allowance < amount {
            panic!("insufficient allowance");
        }
        let from_balance = read_balance(&e, &from);
        if from_balance < amount {
            panic!("insufficient balance");
        }
        let to_balance = read_balance(&e, &to);
        save_allowance(&e, &from, &spender, allowance - amount);
        save_balance(&e, &from, from_balance - amount);
        save_balance(&e, &to, to_balance.checked_add(amount).expect("overflow"));
    }

    /// Approve a spender to transfer up to `amount` credits.
    pub fn approve(e: Env, from: Address, spender: Address, amount: i128, _expiration_ledger: u32) {
        if amount < 0 {
            panic!("amount must be non-negative");
        }
        from.require_auth();
        save_allowance(&e, &from, &spender, amount);
    }

    /// Permanently retire credits and optionally record in the retirement registry.
    pub fn retire(
        e: Env,
        holder: Address,
        amount: i128,
        purpose: String,
        metadata_uri: String,
    ) -> RetirementCertificate {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        require_not_paused(&e);
        holder.require_auth();

        let balance = read_balance(&e, &holder);
        if balance < amount {
            panic!("insufficient balance");
        }
        save_balance(&e, &holder, balance - amount);

        let total = read_total_supply(&e);
        save_total_supply(&e, total - amount);

        let total_retired = read_total_retired(&e);
        save_total_retired(&e, total_retired + amount);

        let metadata: CreditMetadata = e.storage().instance().get(&DataKey::Metadata).unwrap();
        let project_id = metadata.project_id.clone();
        let cert_count: u64 = e
            .storage()
            .instance()
            .get(&DataKey::CertCount)
            .unwrap();
        let timestamp = e.ledger().timestamp();

        let cert = RetirementCertificate {
            retiree: holder.clone(),
            project_id: metadata.project_id,
            amount,
            purpose: purpose.clone(),
            timestamp,
            metadata_uri: metadata_uri.clone(),
        };
        e.storage()
            .instance()
            .set(&DataKey::Cert(cert_count), &cert);
        e.storage()
            .instance()
            .set(&DataKey::CertCount, &(cert_count + 1));

        e.events()
            .publish((EVENT_RETIRED,), (holder.clone(), amount, cert.clone()));

        if let Some(registry) = e.storage().instance().get::<_, Address>(&DataKey::RetirementRegistry) {
            let record_args: Vec<Val> = vec![
                &e,
                e.current_contract_address().to_val(),
                holder.to_val(),
                project_id.to_val(),
                amount.into_val(&e),
                purpose.to_val(),
                metadata_uri.to_val(),
            ];
            e.invoke_contract::<Val>(
                &registry,
                &Symbol::new(&e, "record_retirement"),
                record_args,
            );
        }

        cert
    }

    // ── Read-Only Functions ──

    pub fn balance(e: Env, addr: Address) -> i128 {
        read_balance(&e, &addr)
    }

    pub fn total_supply(e: Env) -> i128 {
        read_total_supply(&e)
    }

    pub fn total_retired(e: Env) -> i128 {
        read_total_retired(&e)
    }

    pub fn allowance(e: Env, from: Address, spender: Address) -> i128 {
        read_allowance(&e, &from, &spender)
    }

    pub fn name(e: Env) -> String {
        e.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(e: Env) -> String {
        e.storage().instance().get(&DataKey::Symbol).unwrap()
    }

    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    pub fn metadata(e: Env) -> CreditMetadata {
        e.storage().instance().get(&DataKey::Metadata).unwrap()
    }

    pub fn get_certificate(e: Env, index: u64) -> Option<RetirementCertificate> {
        e.storage().instance().get(&DataKey::Cert(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{Address, Env, String, TryFromVal};

    fn setup() -> (Env, Address, Address, Address, BytesN<32>, CreditTokenClient<'static>) {
        let e = Env::default();
        let admin = Address::generate(&e);
        let user1 = Address::generate(&e);
        let user2 = Address::generate(&e);
        let project_id = BytesN::from_array(&e, &[1u8; 32]);
        let name = String::from_str(&e, "Green Valley Credits");
        let symbol = String::from_str(&e, "GVC");
        let methodology = String::from_str(&e, "Wetland_Restoration_v2.1");
        let contract_id = e.register_contract(None, CreditToken);
        let client = CreditTokenClient::new(&e, &contract_id);

        client.initialize(&admin, &name, &symbol, &project_id, &methodology);

        (e, admin, user1, user2, project_id, client)
    }

    #[test]
    fn test_initialize_sets_values() {
        let e = Env::default();
        let admin = Address::generate(&e);
        let project_id = BytesN::from_array(&e, &[2u8; 32]);
        let name = String::from_str(&e, "Test Credit");
        let symbol = String::from_str(&e, "TST");
        let methodology = String::from_str(&e, "Riparian_Buffer_v1.0");
        let contract_id = e.register_contract(None, CreditToken);
        let client = CreditTokenClient::new(&e, &contract_id);

        client.initialize(&admin, &name, &symbol, &project_id, &methodology);

        assert_eq!(client.name(), name);
        assert_eq!(client.symbol(), symbol);
        assert_eq!(client.decimals(), 7);
        assert_eq!(client.total_supply(), 0);
        assert_eq!(client.total_retired(), 0);
        let meta = client.metadata();
        assert_eq!(meta.project_id, project_id);
        assert_eq!(meta.methodology, methodology);
    }

    #[test]
    fn test_mint_to_increases_balance_and_supply() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &1000);

        assert_eq!(client.balance(&user), 1000);
        assert_eq!(client.total_supply(), 1000);
        assert_eq!(client.total_retired(), 0);
    }

    #[test]
    fn test_mint_emits_event() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &500);

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        let (_contract, topics, _data) = &events.get(0).unwrap();
        let topic: Symbol = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, symbol_short!("minted"));
    }

    #[test]
    fn test_burn_decreases_balance_and_supply() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &1000);
        client.burn(&admin, &user, &300);

        assert_eq!(client.balance(&user), 700);
        assert_eq!(client.total_supply(), 700);
    }

    #[test]
    fn test_transfer_moves_balance() {
        let (e, admin, user1, user2, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user1, &1000);
        client.transfer(&user1, &user2, &300);

        assert_eq!(client.balance(&user1), 700);
        assert_eq!(client.balance(&user2), 300);
        assert_eq!(client.total_supply(), 1000);
    }

    #[test]
    fn test_transfer_emits_event() {
        let (e, admin, user1, user2, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user1, &500);
        client.transfer(&user1, &user2, &200);

        let events = e.events().all();
        assert_eq!(events.len(), 2);
        let (_contract, topics, _data) = &events.get(1).unwrap();
        let topic: Symbol = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, symbol_short!("xfer"));
    }

    #[test]
    fn test_approve_sets_and_overwrites() {
        let (e, _admin, owner, spender, _project_id, client) = setup();
        e.mock_all_auths();

        client.approve(&owner, &spender, &100, &100000);
        assert_eq!(client.allowance(&owner, &spender), 100);

        client.approve(&owner, &spender, &250, &100001);
        assert_eq!(client.allowance(&owner, &spender), 250);
    }

    #[test]
    fn test_transfer_from_with_allowance() {
        let (e, admin, owner, spender, _project_id, client) = setup();
        let recipient = Address::generate(&e);
        e.mock_all_auths();

        client.mint_to(&admin, &owner, &1000);
        client.approve(&owner, &spender, &500, &100000);
        client.transfer_from(&spender, &owner, &recipient, &200);

        assert_eq!(client.balance(&owner), 800);
        assert_eq!(client.balance(&recipient), 200);
        assert_eq!(client.allowance(&owner, &spender), 300);
    }

    #[test]
    fn test_retire_burns_and_generates_certificate() {
        let (e, admin, user, _, project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &1000);
        let purpose = String::from_str(&e, "voluntary");
        let uri = String::from_str(&e, "ipfs://QmCert");

        let cert = client.retire(&user, &300, &purpose, &uri);

        assert_eq!(cert.retiree, user);
        assert_eq!(cert.project_id, project_id);
        assert_eq!(cert.amount, 300);
        assert_eq!(cert.purpose, purpose);
        assert_eq!(cert.metadata_uri, uri);

        assert_eq!(client.balance(&user), 700);
        assert_eq!(client.total_supply(), 700);
        assert_eq!(client.total_retired(), 300);
    }

    #[test]
    fn test_retire_multiple_certificates() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &1000);
        let purpose = String::from_str(&e, "voluntary");
        let uri1 = String::from_str(&e, "ipfs://Cert1");
        let uri2 = String::from_str(&e, "ipfs://Cert2");

        let cert1 = client.retire(&user, &400, &purpose, &uri1);
        assert_eq!(cert1.amount, 400);

        let cert2 = client.retire(&user, &200, &purpose, &uri2);
        assert_eq!(cert2.amount, 200);

        assert_eq!(client.balance(&user), 400);
        assert_eq!(client.total_retired(), 600);

        let retrieved1 = client.get_certificate(&0).unwrap();
        assert_eq!(retrieved1.amount, 400);
        assert_eq!(retrieved1.metadata_uri, uri1);

        let retrieved2 = client.get_certificate(&1).unwrap();
        assert_eq!(retrieved2.amount, 200);
        assert_eq!(retrieved2.metadata_uri, uri2);

        assert!(client.get_certificate(&5).is_none());
    }

    #[test]
    fn test_retire_emits_event() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &500);
        let purpose = String::from_str(&e, "compliance");
        let uri = String::from_str(&e, "ipfs://QmCert");
        client.retire(&user, &200, &purpose, &uri);

        let events = e.events().all();
        assert_eq!(events.len(), 2);
        let (_contract, topics, _data) = &events.get(1).unwrap();
        let topic: Symbol = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, symbol_short!("retired"));
    }

    #[test]
    fn test_set_admin_transfers_ownership() {
        let (e, admin, _user1, _user2, _project_id, client) = setup();
        let new_admin = Address::generate(&e);
        e.mock_all_auths();

        client.set_admin(&admin, &new_admin);
        client.mint_to(&new_admin, &new_admin, &200);
        assert_eq!(client.balance(&new_admin), 200);
    }

    #[test]
    fn test_full_credit_lifecycle() {
        let (e, admin, farmer, buyer, project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &farmer, &5000);
        assert_eq!(client.balance(&farmer), 5000);

        client.transfer(&farmer, &buyer, &1000);
        assert_eq!(client.balance(&farmer), 4000);
        assert_eq!(client.balance(&buyer), 1000);

        let purpose = String::from_str(&e, "voluntary");
        let uri = String::from_str(&e, "ipfs://QmCert");
        let cert = client.retire(&buyer, &500, &purpose, &uri);
        assert_eq!(cert.amount, 500);
        assert_eq!(cert.project_id, project_id);

        assert_eq!(client.balance(&buyer), 500);
        assert_eq!(client.total_retired(), 500);
        assert_eq!(client.total_supply(), 4500);
    }

    #[test]
    fn test_max_supply_blocks_over_cap() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.set_max_supply(&admin, &1000);
        assert_eq!(client.max_supply(), 1000);

        client.mint_to(&admin, &user, &1000);
        assert_eq!(client.total_supply(), 1000);

        // Minting beyond cap should panic
        let result = std::panic::catch_unwind(|| {
            client.mint_to(&admin, &user, &1);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_max_supply_zero_means_uncapped() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        // Default: 0 = uncapped
        assert_eq!(client.max_supply(), 0);
        client.mint_to(&admin, &user, &1_000_000);
        assert_eq!(client.total_supply(), 1_000_000);
    }

    #[test]
    fn test_max_supply_allows_exact_cap() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.set_max_supply(&admin, &500);
        client.mint_to(&admin, &user, &300);
        client.mint_to(&admin, &user, &200); // exactly at cap
        assert_eq!(client.total_supply(), 500);
    }

    #[test]
    fn test_batch_mint_to_distributes_correctly() {
        let (e, admin, user1, user2, _project_id, client) = setup();
        let user3 = Address::generate(&e);
        e.mock_all_auths();

        let recipients = Vec::from_array(&e, [user1.clone(), user2.clone(), user3.clone()]);
        let amounts: Vec<i128> = Vec::from_array(&e, [100i128, 200i128, 300i128]);

        client.batch_mint_to(&admin, &recipients, &amounts);

        assert_eq!(client.balance(&user1), 100);
        assert_eq!(client.balance(&user2), 200);
        assert_eq!(client.balance(&user3), 300);
        assert_eq!(client.total_supply(), 600);
    }

    #[test]
    fn test_batch_mint_to_same_recipient_accumulates() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        let recipients = Vec::from_array(&e, [user.clone(), user.clone()]);
        let amounts: Vec<i128> = Vec::from_array(&e, [150i128, 250i128]);

        client.batch_mint_to(&admin, &recipients, &amounts);
        assert_eq!(client.balance(&user), 400);
        assert_eq!(client.total_supply(), 400);
    }

    #[test]
    fn test_pause_blocks_mint() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.pause(&admin);
        assert!(client.paused());

        let result = std::panic::catch_unwind(|| {
            client.mint_to(&admin, &user, &100);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_pause_blocks_transfer() {
        let (e, admin, user1, user2, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user1, &1000);
        client.pause(&admin);

        let result = std::panic::catch_unwind(|| {
            client.transfer(&user1, &user2, &100);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_unpause_restores_operations() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.pause(&admin);
        assert!(client.paused());

        client.unpause(&admin);
        assert!(!client.paused());

        client.mint_to(&admin, &user, &500);
        assert_eq!(client.balance(&user), 500);
    }

    #[test]
    fn test_paused_state_does_not_affect_reads() {
        let (e, admin, user, _, _project_id, client) = setup();
        e.mock_all_auths();

        client.mint_to(&admin, &user, &300);
        client.pause(&admin);

        // Read-only functions still work while paused
        assert_eq!(client.balance(&user), 300);
        assert_eq!(client.total_supply(), 300);
        assert!(client.paused());
    }
}
