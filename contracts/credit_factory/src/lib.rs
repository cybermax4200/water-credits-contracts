#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Bytes, BytesN, Env, String,
    Symbol, Val, Vec,
};

#[cfg(test)]
extern crate std;

// ── Events ──
const EVENT_PROJ_REG: Symbol = symbol_short!("proj_reg");

// ── Data Types ──

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectInfo {
    pub id: BytesN<32>,
    pub name: String,
    pub latitude: i64,
    pub longitude: i64,
    pub methodology: String,
    pub owner: Address,
    pub status: String,
    pub credit_token: Address,
    pub registration_date: u64,
    pub area_hectares: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    ProjectCount,
    Project(BytesN<32>),
}

fn has_admin(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Admin)
}

fn read_admin(e: &Env) -> Address {
    e.storage().instance().get(&DataKey::Admin).unwrap()
}

#[contract]
pub struct CreditFactory;

#[contractimpl]
impl CreditFactory {
    pub fn initialize(e: Env, admin: Address) {
        if has_admin(&e) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::ProjectCount, &0u64);
    }

    pub fn admin(e: Env) -> Address {
        read_admin(&e)
    }

    pub fn register_project(
        e: Env,
        admin: Address,
        name: String,
        latitude: i64,
        longitude: i64,
        methodology: String,
        owner: Address,
        area_hectares: u64,
        credit_token_wasm_hash: BytesN<32>,
    ) -> BytesN<32> {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }

        if name.len() == 0 {
            panic!("name must not be empty");
        }
        if latitude < -90000000 || latitude > 90000000 {
            panic!("invalid latitude");
        }
        if longitude < -180000000 || longitude > 180000000 {
            panic!("invalid longitude");
        }
        if area_hectares == 0 {
            panic!("area must be positive");
        }

        let count: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ProjectCount)
            .unwrap();
        let timestamp = e.ledger().timestamp();

        // Generate unique project ID from count + timestamp
        let mut preimage: Bytes = Bytes::new(&e);
        {
            let count_bytes = count.to_be_bytes();
            preimage.append(&Bytes::from_array(&e, &count_bytes));
        }
        {
            let ts_bytes = timestamp.to_be_bytes();
            preimage.append(&Bytes::from_array(&e, &ts_bytes));
        }
        let project_id: BytesN<32> = e.crypto().sha256(&preimage);

        // Deploy new credit_token contract
        let salt = project_id.clone();
        let token_address: Address = e
            .deployer()
            .with_current_contract(salt)
            .deploy(credit_token_wasm_hash);

        // Prepare initialize args and call the token
        let token_symbol = String::from_str(&e, "WC");
        let init_args: Vec<Val> = vec![
            &e,
            admin.clone().to_val(),
            name.clone().to_val(),
            token_symbol.to_val(),
            project_id.clone().to_val(),
            methodology.clone().to_val(),
        ];
        e.invoke_contract::<()>(
            &token_address,
            &Symbol::new(&e, "initialize"),
            init_args,
        );

        let project = ProjectInfo {
            id: project_id.clone(),
            name,
            latitude,
            longitude,
            methodology,
            owner,
            status: String::from_str(&e, "registered"),
            credit_token: token_address,
            registration_date: timestamp,
            area_hectares,
        };

        e.storage()
            .instance()
            .set(&DataKey::Project(project_id.clone()), &project);
        e.storage()
            .instance()
            .set(&DataKey::ProjectCount, &(count + 1));

        e.events().publish((EVENT_PROJ_REG,), (project_id.clone(),));

        project_id
    }

    pub fn get_project(e: Env, project_id: BytesN<32>) -> Option<ProjectInfo> {
        e.storage().instance().get(&DataKey::Project(project_id))
    }

    pub fn update_project_status(e: Env, admin: Address, project_id: BytesN<32>, status: String) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }

        let mut project: ProjectInfo = e
            .storage()
            .instance()
            .get(&DataKey::Project(project_id.clone()))
            .unwrap_or_else(|| panic!("project not found"));

        let valid = status == String::from_str(&e, "registered")
            || status == String::from_str(&e, "active")
            || status == String::from_str(&e, "completed")
            || status == String::from_str(&e, "suspended");
        if !valid {
            panic!("invalid status");
        }

        project.status = status;
        e.storage()
            .instance()
            .set(&DataKey::Project(project_id), &project);
    }

    pub fn project_count(e: Env) -> u64 {
        e.storage()
            .instance()
            .get(&DataKey::ProjectCount)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{Address, Env, TryFromVal};

    fn setup_with_client() -> (Env, Address, Address, BytesN<32>, CreditFactoryClient<'static>) {
        let e = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let wasm = include_bytes!("../../../target/wasm32-unknown-unknown/release/credit_token.wasm");
        let wasm_hash = e.deployer().upload_contract_wasm(Bytes::from_slice(&e, wasm));
        let contract_id = e.register_contract(None, CreditFactory);
        let client = CreditFactoryClient::new(&e, &contract_id);

        client.initialize(&admin);

        (e, admin, owner, wasm_hash, client)
    }

    #[test]
    fn test_initialize_sets_admin() {
        let e = Env::default();
        let admin = Address::generate(&e);
        let contract_id = e.register_contract(None, CreditFactory);
        let client = CreditFactoryClient::new(&e, &contract_id);

        client.initialize(&admin);

        assert_eq!(client.admin(), admin);
        assert_eq!(client.project_count(), 0);
    }

    #[test]
    fn test_register_project_succeeds() {
        let (e, admin, owner, wasm_hash, client) = setup_with_client();
        e.mock_all_auths();

        let name = String::from_str(&e, "Green Valley Wetland");
        let methodology = String::from_str(&e, "Wetland_Restoration_v2.1");

        let project_id = client.register_project(
            &admin,
            &name,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &500,
            &wasm_hash,
        );

        let project = client.get_project(&project_id).unwrap();
        assert_eq!(project.name, name);
        assert_eq!(project.latitude, 38897700);
        assert_eq!(project.longitude, -77036500);
        assert_eq!(project.methodology, methodology);
        assert_eq!(project.owner, owner);
        assert_eq!(project.status, String::from_str(&e, "registered"));
        assert_eq!(project.area_hectares, 500);
        assert_eq!(client.project_count(), 1);
    }

    #[test]
    fn test_register_project_increments_count() {
        let (e, admin, owner, wasm_hash, client) = setup_with_client();
        e.mock_all_auths();

        let name1 = String::from_str(&e, "Green Valley Wetland");
        let name2 = String::from_str(&e, "Blue River Restoration");
        let methodology = String::from_str(&e, "Wetland_Restoration_v2.1");

        let _id1 = client.register_project(
            &admin,
            &name1,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &500,
            &wasm_hash,
        );
        assert_eq!(client.project_count(), 1);

        let _id2 = client.register_project(
            &admin,
            &name2,
            &38900000,
            &(-77040000),
            &methodology,
            &owner,
            &300,
            &wasm_hash,
        );
        assert_eq!(client.project_count(), 2);
    }

    #[test]
    fn test_register_project_unique_ids() {
        let (e, admin, owner, wasm_hash, client) = setup_with_client();
        e.mock_all_auths();

        let name = String::from_str(&e, "Test Project");
        let methodology = String::from_str(&e, "Test_v1");

        let id1 = client.register_project(
            &admin,
            &name,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &500,
            &wasm_hash,
        );
        let id2 = client.register_project(
            &admin,
            &name,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &500,
            &wasm_hash,
        );

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_register_project_emits_event() {
        let (e, admin, owner, wasm_hash, client) = setup_with_client();
        e.mock_all_auths();

        let name = String::from_str(&e, "Event Test");
        let methodology = String::from_str(&e, "Test_v1");
        let _project_id = client.register_project(
            &admin,
            &name,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &100,
            &wasm_hash,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        let (_contract, topics, _data) = &events.get(0).unwrap();
        let topic: Symbol = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, symbol_short!("proj_reg"));
    }

    #[test]
    fn test_get_project_nonexistent_returns_none() {
        let (e, _admin, _owner, _wasm_hash, client) = setup_with_client();
        let fake_id = BytesN::from_array(&e, &[0xffu8; 32]);

        let result = client.get_project(&fake_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_update_project_status_active() {
        let (e, admin, owner, wasm_hash, client) = setup_with_client();
        e.mock_all_auths();

        let name = String::from_str(&e, "Status Test");
        let methodology = String::from_str(&e, "Test_v1");
        let project_id = client.register_project(
            &admin,
            &name,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &100,
            &wasm_hash,
        );

        let new_status = String::from_str(&e, "active");
        client.update_project_status(&admin, &project_id, &new_status);

        let project = client.get_project(&project_id).unwrap();
        assert_eq!(project.status, new_status);
    }

    #[test]
    fn test_update_project_status_full_cycle() {
        let (e, admin, owner, wasm_hash, client) = setup_with_client();
        e.mock_all_auths();

        let name = String::from_str(&e, "Full Cycle");
        let methodology = String::from_str(&e, "Test_v1");
        let project_id = client.register_project(
            &admin,
            &name,
            &38897700,
            &(-77036500),
            &methodology,
            &owner,
            &100,
            &wasm_hash,
        );

        for status in [
            String::from_str(&e, "active"),
            String::from_str(&e, "completed"),
        ] {
            client.update_project_status(&admin, &project_id, &status);
            let project = client.get_project(&project_id).unwrap();
            assert_eq!(project.status, status);
        }
    }
}
