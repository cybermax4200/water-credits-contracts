#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Vec};

#[cfg(test)]
extern crate std;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RetirementRecord {
    pub id: u64,
    pub retiree: Address,
    pub project_id: BytesN<32>,
    pub amount: i128,
    pub purpose: String,
    pub metadata_uri: String,
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    RecordCount,
    TotalRetired,
    Record(u64),
    RetireeRecords(Address),
    ProjectRecords(BytesN<32>),
    AuthorizedCaller(Address),
}

fn has_admin(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Admin)
}

fn read_admin(e: &Env) -> Address {
    e.storage().instance().get(&DataKey::Admin).unwrap()
}

#[contract]
pub struct RetirementRegistry;

#[contractimpl]
impl RetirementRegistry {
    /// Initialize the retirement registry with an admin. Callable once.
    pub fn initialize(e: Env, admin: Address) {
        if has_admin(&e) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::RecordCount, &0u64);
        e.storage().instance().set(&DataKey::TotalRetired, &0i128);
    }

    /// Record a retirement. Only callable by admin or an authorized caller contract.
    /// Returns the unique record ID.
    pub fn record_retirement(
        e: Env,
        caller: Address,
        retiree: Address,
        project_id: BytesN<32>,
        amount: i128,
        purpose: String,
        metadata_uri: String,
    ) -> u64 {
        caller.require_auth();
        let stored: Address = read_admin(&e);
        let authorized = e
            .storage()
            .instance()
            .get(&DataKey::AuthorizedCaller(caller.clone()))
            .unwrap_or(false);
        if caller != stored && !authorized {
            panic!("unauthorized");
        }

        if amount <= 0 {
            panic!("amount must be positive");
        }

        let count: u64 = e.storage().instance().get(&DataKey::RecordCount).unwrap();
        let record_id = count + 1;
        let timestamp = e.ledger().timestamp();

        let record = RetirementRecord {
            id: record_id,
            retiree: retiree.clone(),
            project_id: project_id.clone(),
            amount,
            purpose: purpose.clone(),
            metadata_uri: metadata_uri.clone(),
            timestamp,
        };

        e.storage()
            .instance()
            .set(&DataKey::Record(record_id), &record);

        let mut retiree_ids: Vec<u64> = e
            .storage()
            .instance()
            .get(&DataKey::RetireeRecords(retiree.clone()))
            .unwrap_or(Vec::new(&e));
        retiree_ids.push_back(record_id);
        e.storage()
            .instance()
            .set(&DataKey::RetireeRecords(retiree.clone()), &retiree_ids);

        let mut project_ids: Vec<u64> = e
            .storage()
            .instance()
            .get(&DataKey::ProjectRecords(project_id.clone()))
            .unwrap_or(Vec::new(&e));
        project_ids.push_back(record_id);
        e.storage()
            .instance()
            .set(&DataKey::ProjectRecords(project_id.clone()), &project_ids);

        let total: i128 = e.storage().instance().get(&DataKey::TotalRetired).unwrap();
        e.storage()
            .instance()
            .set(&DataKey::TotalRetired, &(total + amount));
        e.storage()
            .instance()
            .set(&DataKey::RecordCount, &record_id);

        record_id
    }

    /// Get a retirement record by its ID. Returns None if not found.
    pub fn get_record(e: Env, id: u64) -> Option<RetirementRecord> {
        e.storage().instance().get(&DataKey::Record(id))
    }

    /// Get the global total amount of credits retired across all projects.
    pub fn total_retired(e: Env) -> i128 {
        e.storage().instance().get(&DataKey::TotalRetired).unwrap()
    }

    /// Get the total number of retirement records in the registry.
    pub fn record_count(e: Env) -> u64 {
        e.storage().instance().get(&DataKey::RecordCount).unwrap()
    }

    /// Authorize or revoke a contract address to record retirements. Admin only.
    pub fn set_authorized_caller(e: Env, admin: Address, caller: Address, authorized: bool) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        e.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller(caller), &authorized);
    }

    /// Get all retirement records for a given retiree address.
    pub fn get_retirements_by_retiree(e: Env, retiree: Address) -> Vec<RetirementRecord> {
        let ids: Vec<u64> = e
            .storage()
            .instance()
            .get(&DataKey::RetireeRecords(retiree))
            .unwrap_or(Vec::new(&e));

        let mut records: Vec<RetirementRecord> = Vec::new(&e);
        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            if let Some(record) = e.storage().instance().get(&DataKey::Record(id)) {
                records.push_back(record);
            }
        }
        records
    }

    /// Get all retirement records for a given project ID.
    /// Useful for computing total retired supply per project and for audit trails.
    pub fn get_retirements_by_project(e: Env, project_id: BytesN<32>) -> Vec<RetirementRecord> {
        let ids: Vec<u64> = e
            .storage()
            .instance()
            .get(&DataKey::ProjectRecords(project_id))
            .unwrap_or(Vec::new(&e));

        let mut records: Vec<RetirementRecord> = Vec::new(&e);
        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            if let Some(record) = e.storage().instance().get(&DataKey::Record(id)) {
                records.push_back(record);
            }
        }
        records
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, Address, RetirementRegistryClient<'static>) {
        let e = Env::default();
        let admin = Address::generate(&e);
        let contract_id = e.register_contract(None, RetirementRegistry);
        let client = RetirementRegistryClient::new(&e, &contract_id);
        client.initialize(&admin);
        (e, admin, client)
    }

    #[test]
    fn test_initialize() {
        let (_e, _admin, client) = setup();
        assert_eq!(client.record_count(), 0);
        assert_eq!(client.total_retired(), 0);
    }

    #[test]
    fn test_record_retirement_succeeds() {
        let (e, admin, client) = setup();
        e.mock_all_auths();

        let retiree = Address::generate(&e);
        let project_id = BytesN::from_array(&e, &[1u8; 32]);
        let purpose = String::from_str(&e, "voluntary");
        let uri = String::from_str(&e, "ipfs://QmCert");

        let id = client.record_retirement(&admin, &retiree, &project_id, &500, &purpose, &uri);
        assert_eq!(id, 1);

        let record = client.get_record(&id).unwrap();
        assert_eq!(record.retiree, retiree);
        assert_eq!(record.amount, 500);
        assert_eq!(record.purpose, purpose);
        assert_eq!(record.metadata_uri, uri);

        assert_eq!(client.total_retired(), 500);
        assert_eq!(client.record_count(), 1);
    }

    #[test]
    fn test_record_retirement_multiple_entries() {
        let (e, admin, client) = setup();
        e.mock_all_auths();

        let retiree1 = Address::generate(&e);
        let retiree2 = Address::generate(&e);
        let project_id = BytesN::from_array(&e, &[1u8; 32]);
        let purpose = String::from_str(&e, "voluntary");
        let uri = String::from_str(&e, "ipfs://QmCert");

        client.record_retirement(&admin, &retiree1, &project_id, &300, &purpose, &uri);
        client.record_retirement(&admin, &retiree1, &project_id, &200, &purpose, &uri);
        client.record_retirement(&admin, &retiree2, &project_id, &100, &purpose, &uri);

        assert_eq!(client.record_count(), 3);
        assert_eq!(client.total_retired(), 600);

        let records1 = client.get_retirements_by_retiree(&retiree1);
        assert_eq!(records1.len(), 2);
        assert_eq!(records1.get(0).unwrap().amount, 300);
        assert_eq!(records1.get(1).unwrap().amount, 200);

        let records2 = client.get_retirements_by_retiree(&retiree2);
        assert_eq!(records2.len(), 1);
        assert_eq!(records2.get(0).unwrap().amount, 100);
    }

    #[test]
    fn test_record_authorized_only() {
        let (e, admin, client) = setup();
        e.mock_all_auths();

        let retiree = Address::generate(&e);
        let project_id = BytesN::from_array(&e, &[1u8; 32]);
        let purpose = String::from_str(&e, "voluntary");
        let uri = String::from_str(&e, "ipfs://QmCert");

        // Authorized admin can record
        client.record_retirement(&admin, &retiree, &project_id, &500, &purpose, &uri);
        assert_eq!(client.total_retired(), 500);
    }

    #[test]
    fn test_get_record_nonexistent() {
        let (_e, _admin, client) = setup();
        let record = client.get_record(&999);
        assert!(record.is_none());
    }

    #[test]
    fn test_empty_retiree_records() {
        let (e, _admin, client) = setup();
        let retiree = Address::generate(&e);
        let records = client.get_retirements_by_retiree(&retiree);
        assert_eq!(records.len(), 0);
    }

    #[test]
    fn test_get_retirements_by_project_single() {
        let (e, admin, client) = setup();
        e.mock_all_auths();

        let retiree1 = Address::generate(&e);
        let retiree2 = Address::generate(&e);
        let project_a = BytesN::from_array(&e, &[1u8; 32]);
        let project_b = BytesN::from_array(&e, &[2u8; 32]);
        let purpose = String::from_str(&e, "voluntary");
        let uri = String::from_str(&e, "ipfs://QmCert");

        client.record_retirement(&admin, &retiree1, &project_a, &300, &purpose, &uri);
        client.record_retirement(&admin, &retiree2, &project_a, &200, &purpose, &uri);
        client.record_retirement(&admin, &retiree1, &project_b, &100, &purpose, &uri);

        let proj_a_records = client.get_retirements_by_project(&project_a);
        assert_eq!(proj_a_records.len(), 2);

        let total_a: i128 = (0..proj_a_records.len())
            .map(|i| proj_a_records.get(i as u32).unwrap().amount)
            .sum();
        assert_eq!(total_a, 500);

        let proj_b_records = client.get_retirements_by_project(&project_b);
        assert_eq!(proj_b_records.len(), 1);
        assert_eq!(proj_b_records.get(0).unwrap().amount, 100);
    }

    #[test]
    fn test_get_retirements_by_project_empty() {
        let (e, _admin, client) = setup();
        let project_id = BytesN::from_array(&e, &[0xffu8; 32]);
        let records = client.get_retirements_by_project(&project_id);
        assert_eq!(records.len(), 0);
    }
}
