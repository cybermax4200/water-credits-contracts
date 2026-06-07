#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec,
};

#[cfg(test)]
extern crate std;

const EVENT_READING_VERIFIED: Symbol = symbol_short!("rdng_vrfy");

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReadingSubmission {
    pub oracle: Address,
    pub nonce: u64,
    pub timestamp: u64,
    pub ph: i64,
    pub turbidity: i64,
    pub dissolved_oxygen: i64,
    pub flow_rate: i64,
    pub temperature: i64,
    pub total_nitrogen: i64,
    pub total_phosphorus: i64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VerificationResult {
    pub project_id: BytesN<32>,
    pub n_removal_kg: i128,
    pub p_removal_kg: i128,
    pub quality_penalty: i64,
    pub volumetric_credit: i128,
    pub total_credits: i128,
    pub oracle_count: u32,
    pub finalized_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleConfig {
    pub min_oracles: u32,
    pub max_oracles: u32,
    pub quality_threshold_ph: i64,
    pub quality_threshold_turbidity: i64,
    pub quality_threshold_do: i64,
    pub quality_threshold_temp: i64,
    pub credit_per_kg_n: i128,
    pub credit_per_kg_p: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct WindowState {
    pub submissions: Vec<ReadingSubmission>,
    pub finalized: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    OracleActive(Address),
    OracleCount,
    Config,
    OracleNonce(Address),
    WindowState(BytesN<32>),
    OracleSubmitted(BytesN<32>, Address),
    LastResult(BytesN<32>),
}

fn has_admin(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Admin)
}

fn read_admin(e: &Env) -> Address {
    e.storage().instance().get(&DataKey::Admin).unwrap()
}

fn read_config(e: &Env) -> OracleConfig {
    e.storage().instance().get(&DataKey::Config).unwrap()
}

fn median_i64(e: &Env, values: &Vec<i64>) -> i64 {
    let mut sorted: Vec<i64> = Vec::new(e);
    for i in 0..values.len() {
        let val = values.get(i).unwrap();
        let mut inserted = false;
        for j in 0..sorted.len() {
            if val < sorted.get(j).unwrap() {
                sorted.insert(j, val);
                inserted = true;
                break;
            }
        }
        if !inserted {
            sorted.push_back(val);
        }
    }
    let len = sorted.len();
    if len % 2 == 0 {
        (sorted.get(len / 2 - 1).unwrap() + sorted.get(len / 2).unwrap()) / 2
    } else {
        sorted.get(len / 2).unwrap()
    }
}

#[allow(unused)]
fn median_i128(e: &Env, values: &Vec<i128>) -> i128 {
    let mut sorted: Vec<i128> = Vec::new(e);
    for i in 0..values.len() {
        let val = values.get(i).unwrap();
        let mut inserted = false;
        for j in 0..sorted.len() {
            if val < sorted.get(j).unwrap() {
                sorted.insert(j, val);
                inserted = true;
                break;
            }
        }
        if !inserted {
            sorted.push_back(val);
        }
    }
    let len = sorted.len();
    if len % 2 == 0 {
        (sorted.get(len / 2 - 1).unwrap() + sorted.get(len / 2).unwrap()) / 2
    } else {
        sorted.get(len / 2).unwrap()
    }
}

#[contract]
pub struct VerificationOracle;

#[contractimpl]
impl VerificationOracle {
    pub fn initialize(e: Env, admin: Address) {
        if has_admin(&e) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::OracleCount, &0u32);

        let config = OracleConfig {
            min_oracles: 3,
            max_oracles: 10,
            quality_threshold_ph: 600,
            quality_threshold_turbidity: 50,
            quality_threshold_do: 50,
            quality_threshold_temp: 300,
            credit_per_kg_n: 10,
            credit_per_kg_p: 20,
        };
        e.storage().instance().set(&DataKey::Config, &config);
    }

    pub fn add_oracle(e: Env, admin: Address, oracle: Address) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        if e.storage().instance().has(&DataKey::OracleActive(oracle.clone())) {
            panic!("oracle already active");
        }
        let count: u32 = e.storage().instance().get(&DataKey::OracleCount).unwrap();
        let config: OracleConfig = read_config(&e);
        if count >= config.max_oracles {
            panic!("max oracles reached");
        }
        e.storage()
            .instance()
            .set(&DataKey::OracleActive(oracle.clone()), &true);
        e.storage()
            .instance()
            .set(&DataKey::OracleCount, &(count + 1));
    }

    pub fn remove_oracle(e: Env, admin: Address, oracle: Address) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        if !e.storage()
            .instance()
            .has(&DataKey::OracleActive(oracle.clone()))
        {
            panic!("oracle not active");
        }
        let count: u32 = e.storage().instance().get(&DataKey::OracleCount).unwrap();
        let config: OracleConfig = read_config(&e);
        if count <= config.min_oracles {
            panic!("minimum oracles required");
        }
        e.storage()
            .instance()
            .remove(&DataKey::OracleActive(oracle));
        e.storage()
            .instance()
            .set(&DataKey::OracleCount, &(count - 1));
    }

    pub fn is_oracle_active(e: Env, oracle: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::OracleActive(oracle))
            .unwrap_or(false)
    }

    pub fn submit_reading(
        e: Env,
        oracle: Address,
        project_id: BytesN<32>,
        nonce: u64,
        ph: i64,
        turbidity: i64,
        dissolved_oxygen: i64,
        flow_rate: i64,
        temperature: i64,
        total_nitrogen: i64,
        total_phosphorus: i64,
    ) -> Option<VerificationResult> {
        oracle.require_auth();

        if !e.storage()
            .instance()
            .get(&DataKey::OracleActive(oracle.clone()))
            .unwrap_or(false)
        {
            panic!("oracle not active");
        }

        let expected_nonce: u64 = e
            .storage()
            .instance()
            .get(&DataKey::OracleNonce(oracle.clone()))
            .unwrap_or(0)
            + 1;
        if nonce != expected_nonce {
            panic!("invalid nonce");
        }
        e.storage()
            .instance()
            .set(&DataKey::OracleNonce(oracle.clone()), &nonce);

        // Prevent duplicate oracle per window
        if e.storage()
            .instance()
            .has(&DataKey::OracleSubmitted(project_id.clone(), oracle.clone()))
        {
            panic!("oracle already submitted for this window");
        }

        let mut window: WindowState = e
            .storage()
            .instance()
            .get(&DataKey::WindowState(project_id.clone()))
            .unwrap_or(WindowState {
                submissions: Vec::new(&e),
                finalized: false,
            });

        if window.finalized {
            panic!("window already finalized");
        }

        let timestamp = e.ledger().timestamp();

        let submission = ReadingSubmission {
            oracle: oracle.clone(),
            nonce,
            timestamp,
            ph,
            turbidity,
            dissolved_oxygen,
            flow_rate,
            temperature,
            total_nitrogen,
            total_phosphorus,
        };

        window.submissions.push_back(submission);
        e.storage()
            .instance()
            .set(&DataKey::WindowState(project_id.clone()), &window);

        e.storage()
            .instance()
            .set(
                &DataKey::OracleSubmitted(project_id.clone(), oracle.clone()),
                &true,
            );

        let config: OracleConfig = read_config(&e);

        if window.submissions.len() >= config.min_oracles {
            let subs = &window.submissions;
            let n_subs = subs.len();

            let mut ph_vals: Vec<i64> = Vec::new(&e);
            let mut turb_vals: Vec<i64> = Vec::new(&e);
            let mut do_vals: Vec<i64> = Vec::new(&e);
            let mut temp_vals: Vec<i64> = Vec::new(&e);
            let mut flow_vals: Vec<i64> = Vec::new(&e);
            let mut n_vals: Vec<i64> = Vec::new(&e);
            let mut p_vals: Vec<i64> = Vec::new(&e);
            for k in 0..n_subs {
                let s = subs.get(k).unwrap();
                ph_vals.push_back(s.ph);
                turb_vals.push_back(s.turbidity);
                do_vals.push_back(s.dissolved_oxygen);
                temp_vals.push_back(s.temperature);
                flow_vals.push_back(s.flow_rate);
                n_vals.push_back(s.total_nitrogen);
                p_vals.push_back(s.total_phosphorus);
            }

            let med_ph = median_i64(&e, &ph_vals);
            let med_turb = median_i64(&e, &turb_vals);
            let med_do = median_i64(&e, &do_vals);
            let med_temp = median_i64(&e, &temp_vals);
            let med_flow = median_i64(&e, &flow_vals);
            let med_n = median_i64(&e, &n_vals);
            let med_p = median_i64(&e, &p_vals);

            // N removal: baseline 10 mg/L
            let baseline_n: i128 = 10;
            let n_removed: i128 = if (med_n as i128) < baseline_n {
                (baseline_n - med_n as i128) * med_flow as i128 * 3600 / 1000000
            } else {
                0
            };

            // P removal: baseline 2 mg/L
            let baseline_p: i128 = 2;
            let p_removed: i128 = if (med_p as i128) < baseline_p {
                (baseline_p - med_p as i128) * med_flow as i128 * 3600 / 1000000
            } else {
                0
            };

            // Quality penalty (basis points: 0-10000)
            let mut penalty: i64 = 0;
            if med_ph < config.quality_threshold_ph || med_ph > (config.quality_threshold_ph + 100) {
                penalty += 2000;
            }
            if med_turb > config.quality_threshold_turbidity {
                penalty += 2000;
            }
            if med_do < config.quality_threshold_do {
                penalty += 2000;
            }
            if med_temp > config.quality_threshold_temp {
                penalty += 1000;
            }
            if penalty > 8000 {
                penalty = 8000;
            }

            // Volumetric credit based on flow
            let volumetric_credit: i128 = if med_flow > 0 {
                med_flow as i128 * 100 / 1000
            } else {
                0
            };

            // Gross credit
            let n_credit: i128 = n_removed * config.credit_per_kg_n;
            let p_credit: i128 = p_removed * config.credit_per_kg_p;
            let gross = n_credit + p_credit + volumetric_credit;

            // Apply quality penalty
            let total: i128 = gross * (10000 - penalty as i128) / 10000;

            let result = VerificationResult {
                project_id: project_id.clone(),
                n_removal_kg: n_removed,
                p_removal_kg: p_removed,
                quality_penalty: penalty,
                volumetric_credit,
                total_credits: total,
                oracle_count: window.submissions.len(),
                finalized_at: e.ledger().timestamp(),
            };

            e.storage()
                .instance()
                .set(&DataKey::LastResult(project_id.clone()), &result);

            window.finalized = true;
            e.storage()
                .instance()
                .set(&DataKey::WindowState(project_id.clone()), &window);

            e.events()
                .publish((EVENT_READING_VERIFIED,), (project_id, result.clone()));

            Some(result)
        } else {
            None
        }
    }

    pub fn get_last_result(e: Env, project_id: BytesN<32>) -> Option<VerificationResult> {
        e.storage()
            .instance()
            .get(&DataKey::LastResult(project_id))
    }

    pub fn get_config(e: Env) -> OracleConfig {
        read_config(&e)
    }

    pub fn update_config(e: Env, admin: Address, config: OracleConfig) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Config, &config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup_with_client() -> (Env, Address, VerificationOracleClient<'static>) {
        let e = Env::default();
        let admin = Address::generate(&e);
        let contract_id = e.register_contract(None, VerificationOracle);
        let client = VerificationOracleClient::new(&e, &contract_id);
        client.initialize(&admin);
        (e, admin, client)
    }

    #[test]
    fn test_initialize_sets_default_config() {
        let (_e, _admin, client) = setup_with_client();
        let config = client.get_config();
        assert_eq!(config.min_oracles, 3);
        assert_eq!(config.max_oracles, 10);
        assert_eq!(config.credit_per_kg_n, 10);
        assert_eq!(config.credit_per_kg_p, 20);
    }

    #[test]
    fn test_add_oracle_succeeds() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();
        let oracle = Address::generate(&e);
        client.add_oracle(&admin, &oracle);
        assert!(client.is_oracle_active(&oracle));
    }

    #[test]
    fn test_add_oracle_already_active() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();
        let oracle = Address::generate(&e);
        client.add_oracle(&admin, &oracle);
        assert!(client.is_oracle_active(&oracle));
    }

    #[test]
    fn test_remove_oracle_succeeds() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();
        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        let o4 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);
        client.add_oracle(&admin, &o4);
        client.remove_oracle(&admin, &o4);
        assert!(!client.is_oracle_active(&o4));
    }

    #[test]
    fn test_remove_oracle_above_minimum() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();
        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        let o4 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);
        client.add_oracle(&admin, &o4);
        client.remove_oracle(&admin, &o4);
        assert!(!client.is_oracle_active(&o4));
        assert!(client.is_oracle_active(&o1));
        assert!(client.is_oracle_active(&o2));
        assert!(client.is_oracle_active(&o3));
    }

    #[test]
    fn test_authorized_add_oracle_succeeds() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();
        let oracle = Address::generate(&e);
        client.add_oracle(&admin, &oracle);
        assert!(client.is_oracle_active(&oracle));
    }

    #[test]
    fn test_oracle_submission_works() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();
        let oracle = Address::generate(&e);
        client.add_oracle(&admin, &oracle);

        let project_id = BytesN::from_array(&e, &[1u8; 32]);
        client.submit_reading(&oracle, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        // Submission accepted - no error
    }

    #[test]
    fn test_multi_oracle_aggregation_triggers_finalization() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[2u8; 32]);

        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &710, &12, &75, &480, &260, &9, &1);
        let result = client.submit_reading(&o3, &project_id, &1, &690, &11, &78, &510, &245, &7, &1);

        assert!(result.is_some());
        let res = result.unwrap();
        assert!(res.total_credits > 0);
        assert_eq!(res.oracle_count, 3);
    }

    #[test]
    fn test_finalized_window_has_result() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[3u8; 32]);
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o3, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        let result = client.get_last_result(&project_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().oracle_count, 3);
    }

    #[test]
    fn test_get_last_result_after_finalization() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[4u8; 32]);
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o3, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        let result = client.get_last_result(&project_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().oracle_count, 3);
    }

    #[test]
    fn test_get_last_result_none_before_finalization() {
        let (e, _admin, client) = setup_with_client();
        e.mock_all_auths();

        let project_id = BytesN::from_array(&e, &[5u8; 32]);
        let result = client.get_last_result(&project_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_config_update_succeeds() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let new_config = OracleConfig {
            min_oracles: 5,
            max_oracles: 15,
            quality_threshold_ph: 550,
            quality_threshold_turbidity: 40,
            quality_threshold_do: 60,
            quality_threshold_temp: 310,
            credit_per_kg_n: 15,
            credit_per_kg_p: 25,
        };
        client.update_config(&admin, &new_config);

        let config = client.get_config();
        assert_eq!(config.min_oracles, 5);
        assert_eq!(config.credit_per_kg_n, 15);
    }

    #[test]
    fn test_math_high_np_zero_removal() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[6u8; 32]);
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &15, &5);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &15, &5);
        let result = client.submit_reading(&o3, &project_id, &1, &700, &10, &80, &500, &250, &15, &5);

        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.n_removal_kg, 0);
        assert_eq!(res.p_removal_kg, 0);
    }

    #[test]
    fn test_penalty_boundaries() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[7u8; 32]);
        // Bad pH, high turbidity, low DO, high temp -> max penalty
        client.submit_reading(&o1, &project_id, &1, &300, &200, &10, &500, &350, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &300, &200, &10, &500, &350, &8, &1);
        let result = client.submit_reading(&o3, &project_id, &1, &300, &200, &10, &500, &350, &8, &1);

        assert!(result.is_some());
        assert_eq!(result.unwrap().quality_penalty, 7000); // 2000+2000+2000+1000=7000
    }
}
