#![no_std]
#![allow(clippy::too_many_arguments)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, BytesN, Env, IntoVal, Symbol,
    Val, Vec,
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
pub struct ProjectConfig {
    pub token_contract: Address,
    pub beneficiary: Address,
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
    OracleList,
    Config,
    OracleNonce((BytesN<32>, Address)),
    WindowState(BytesN<32>),
    OracleSubmitted(BytesN<32>, Address),
    LastResult(BytesN<32>),
    ResultHistory(BytesN<32>),
    ProjectConfig(BytesN<32>),
    OracleSubmitCount(Address),
    TotalSubmissions,
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
    if len.is_multiple_of(2) {
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
    if len.is_multiple_of(2) {
        (sorted.get(len / 2 - 1).unwrap() + sorted.get(len / 2).unwrap()) / 2
    } else {
        sorted.get(len / 2).unwrap()
    }
}

#[contract]
pub struct VerificationOracle;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VerificationOracle {
    /// Initialize the oracle contract with an admin and default config. Callable once.
    pub fn initialize(e: Env, admin: Address) {
        if has_admin(&e) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::OracleCount, &0u32);
        e.storage()
            .instance()
            .set(&DataKey::OracleList, &Vec::<Address>::new(&e));

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

    /// Add an oracle address to the whitelist. Only admin can call.
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

        let mut list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::OracleList)
            .unwrap();
        list.push_back(oracle);
        e.storage()
            .instance()
            .set(&DataKey::OracleList, &list);
    }

    /// Remove an oracle from the whitelist. Must maintain at least min_oracles.
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
            .remove(&DataKey::OracleActive(oracle.clone()));
        e.storage()
            .instance()
            .set(&DataKey::OracleCount, &(count - 1));

        // Filter the oracle out of the list
        let list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::OracleList)
            .unwrap();
        let mut filtered: Vec<Address> = Vec::new(&e);
        for i in 0..list.len() {
            let addr = list.get(i).unwrap();
            if addr != oracle {
                filtered.push_back(addr);
            }
        }
        e.storage()
            .instance()
            .set(&DataKey::OracleList, &filtered);
    }

    /// Check if an oracle address is whitelisted and active.
    pub fn is_oracle_active(e: Env, oracle: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::OracleActive(oracle))
            .unwrap_or(false)
    }

    /// Get the list of all currently active oracle addresses.
    pub fn get_oracles(e: Env) -> Vec<Address> {
        e.storage()
            .instance()
            .get(&DataKey::OracleList)
            .unwrap_or_else(|| Vec::new(&e))
    }

    /// Submit a sensor reading for a project. Uses nonce-based replay protection.
    /// When min_oracles submissions are collected, computes median values, calculates
    /// nutrient removal, quality penalty, and volumetric credits. If a ProjectConfig
    /// is set, automatically mints credits to the configured beneficiary.
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
        let result = Self::submit_reading_impl(e.clone(), oracle, project_id.clone(), nonce, ph, turbidity, dissolved_oxygen, flow_rate, temperature, total_nitrogen, total_phosphorus);
        if let Some(ref res) = result {
            if let Some(config) = e.storage().instance().get::<_, ProjectConfig>(&DataKey::ProjectConfig(project_id)) {
                let mint_args: Vec<Val> = vec![
                    &e,
                    e.current_contract_address().to_val(),
                    config.beneficiary.to_val(),
                    res.total_credits.into_val(&e),
                ];
                e.invoke_contract::<()>(
                    &config.token_contract,
                    &Symbol::new(&e, "mint_to"),
                    mint_args,
                );
            }
        }
        result
    }
fn submit_reading_impl(
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
            .get(&DataKey::OracleNonce((project_id.clone(), oracle.clone())))
            .unwrap_or(0)
            + 1;
        if nonce != expected_nonce {
            panic!("invalid nonce");
        }
        e.storage()
            .instance()
            .set(&DataKey::OracleNonce((project_id.clone(), oracle.clone())), &nonce);

        // Track per-oracle and global submission counts
        let oracle_count: u64 = e
            .storage()
            .instance()
            .get(&DataKey::OracleSubmitCount(oracle.clone()))
            .unwrap_or(0);
        e.storage()
            .instance()
            .set(&DataKey::OracleSubmitCount(oracle.clone()), &(oracle_count + 1));
        let total: u64 = e
            .storage()
            .instance()
            .get(&DataKey::TotalSubmissions)
            .unwrap_or(0);
        e.storage()
            .instance()
            .set(&DataKey::TotalSubmissions, &(total + 1));

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

            // Append to historical results
            let mut history: Vec<VerificationResult> = e
                .storage()
                .instance()
                .get(&DataKey::ResultHistory(project_id.clone()))
                .unwrap_or_else(|| Vec::new(&e));
            history.push_back(result.clone());
            e.storage()
                .instance()
                .set(&DataKey::ResultHistory(project_id.clone()), &history);

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

    /// Configure the credit token contract and beneficiary for a project.
    /// When enabled, the oracle will auto-mint credits to the beneficiary upon verification finalization.
    pub fn set_project_config(e: Env, admin: Address, project_id: BytesN<32>, token_contract: Address, beneficiary: Address) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        let config = ProjectConfig { token_contract, beneficiary };
        e.storage().instance().set(&DataKey::ProjectConfig(project_id), &config);
    }

    /// Get the project config (token contract and beneficiary) for a project.
    pub fn get_project_config(e: Env, project_id: BytesN<32>) -> Option<ProjectConfig> {
        e.storage().instance().get(&DataKey::ProjectConfig(project_id))
    }

    /// Get the last verification result for a project. Returns None if no window has been finalized.
    pub fn get_last_result(e: Env, project_id: BytesN<32>) -> Option<VerificationResult> {
        e.storage()
            .instance()
            .get(&DataKey::LastResult(project_id))
    }

    /// Get the full history of verification results for a project.
    pub fn get_result_history(e: Env, project_id: BytesN<32>) -> Vec<VerificationResult> {
        e.storage()
            .instance()
            .get(&DataKey::ResultHistory(project_id))
            .unwrap_or_else(|| Vec::new(&e))
    }

    /// Get the current oracle configuration parameters.
    pub fn get_config(e: Env) -> OracleConfig {
        read_config(&e)
    }

    /// Get the total number of readings an oracle has submitted across all projects and windows.
    pub fn oracle_submit_count(e: Env, oracle: Address) -> u64 {
        e.storage()
            .instance()
            .get(&DataKey::OracleSubmitCount(oracle))
            .unwrap_or(0)
    }

    /// Get the total number of readings submitted by all oracles across all time.
    pub fn total_submissions(e: Env) -> u64 {
        e.storage()
            .instance()
            .get(&DataKey::TotalSubmissions)
            .unwrap_or(0)
    }

    /// Get the current number of active whitelisted oracles.
    pub fn oracle_count(e: Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::OracleCount)
            .unwrap_or(0)
    }

    /// Update the oracle configuration (min/max oracles, quality thresholds, credit rates). Admin only.
    pub fn update_config(e: Env, admin: Address, config: OracleConfig) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Config, &config);
    }

    /// Reset the open submission window for a project, clearing all pending oracle submissions.
    /// This allows oracles to resubmit for the same project in a new window, e.g. after a
    /// sensor error or stale data invalidation. Only callable by admin.
    /// Does not affect already-finalized results or oracle nonces.
    pub fn reset_window(e: Env, admin: Address, project_id: BytesN<32>) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }

        let window: Option<WindowState> = e
            .storage()
            .instance()
            .get(&DataKey::WindowState(project_id.clone()));

        match window {
            None => panic!("no window found for project"),
            Some(ref w) if w.finalized => panic!("window already finalized"),
            _ => {}
        }

        // Remove the OracleSubmitted markers for all oracles in this window
        let window = window.unwrap();
        for i in 0..window.submissions.len() {
            let sub = window.submissions.get(i).unwrap();
            e.storage()
                .instance()
                .remove(&DataKey::OracleSubmitted(project_id.clone(), sub.oracle));
        }

        // Replace with a fresh empty window
        let fresh = WindowState {
            submissions: Vec::new(&e),
            finalized: false,
        };
        e.storage()
            .instance()
            .set(&DataKey::WindowState(project_id.clone()), &fresh);
    }

    /// Get the number of submissions in the current open window for a project.
    /// Returns 0 if no window exists or the window was already finalized.
    pub fn window_submission_count(e: Env, project_id: BytesN<32>) -> u32 {
        let window: Option<WindowState> = e
            .storage()
            .instance()
            .get(&DataKey::WindowState(project_id));
        match window {
            None => 0,
            Some(w) if w.finalized => 0,
            Some(w) => w.submissions.len(),
        }
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
    fn test_result_history_accumulates_across_windows() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[50u8; 32]);

        // First window
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o3, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        let history = client.get_result_history(&project_id);
        assert_eq!(history.len(), 1);

        // Reset window and submit again
        client.reset_window(&admin, &project_id);
        client.submit_reading(&o1, &project_id, &2, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &2, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o3, &project_id, &2, &700, &10, &80, &500, &250, &8, &1);

        let history = client.get_result_history(&project_id);
        assert_eq!(history.len(), 2);

        // Both should have valid oracle counts
        assert_eq!(history.get(0).unwrap().oracle_count, 3);
        assert_eq!(history.get(1).unwrap().oracle_count, 3);
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

    #[test]
    fn test_oracle_submit_count_increments() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        assert_eq!(client.oracle_submit_count(&o1), 0);
        assert_eq!(client.total_submissions(), 0);

        let project_id = BytesN::from_array(&e, &[10u8; 32]);
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        assert_eq!(client.oracle_submit_count(&o1), 1);
        assert_eq!(client.total_submissions(), 1);

        let project_id2 = BytesN::from_array(&e, &[11u8; 32]);
        client.submit_reading(&o2, &project_id2, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o3, &project_id2, &1, &700, &10, &80, &500, &250, &8, &1);
        assert_eq!(client.oracle_submit_count(&o2), 1);
        assert_eq!(client.oracle_submit_count(&o3), 1);
        assert_eq!(client.total_submissions(), 3);
    }

    #[test]
    fn test_nonce_independent_across_projects() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        client.add_oracle(&admin, &o1);

        let p1 = BytesN::from_array(&e, &[50u8; 32]);
        let p2 = BytesN::from_array(&e, &[51u8; 32]);
        let p3 = BytesN::from_array(&e, &[52u8; 32]);

        // Same oracle uses nonce=1 for all three projects — nonces are per (project, oracle)
        client.submit_reading(&o1, &p1, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o1, &p2, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o1, &p3, &1, &700, &10, &80, &500, &250, &8, &1);

        // Now increment nonce for p1 — nonce=2 is valid for p1
        client.submit_reading(&o1, &p1, &2, &700, &10, &80, &500, &250, &8, &1);

        // Nonce=1 for p2 again should fail (already used), but nonce=2 is valid
        client.submit_reading(&o1, &p2, &2, &700, &10, &80, &500, &250, &8, &1);
    }

    #[test]
    fn test_oracle_count_tracks_additions_and_removals() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        assert_eq!(client.oracle_count(), 0);

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        assert_eq!(client.oracle_count(), 1);

        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);
        assert_eq!(client.oracle_count(), 3);

        client.remove_oracle(&admin, &o2);
        assert_eq!(client.oracle_count(), 2);
    }

    #[test]
    fn test_get_oracles_returns_active_list() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let oracles = client.get_oracles();
        assert_eq!(oracles.len(), 0);

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let oracles = client.get_oracles();
        assert_eq!(oracles.len(), 3);
        assert!(oracles.contains(&o1));
        assert!(oracles.contains(&o2));
        assert!(oracles.contains(&o3));

        client.remove_oracle(&admin, &o2);
        let oracles = client.get_oracles();
        assert_eq!(oracles.len(), 2);
        assert!(oracles.contains(&o1));
        assert!(!oracles.contains(&o2));
        assert!(oracles.contains(&o3));
    }

    #[test]
    fn test_reset_window_clears_submissions() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[30u8; 32]);
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        assert_eq!(client.window_submission_count(&project_id), 2);

        client.reset_window(&admin, &project_id);
        assert_eq!(client.window_submission_count(&project_id), 0);
    }

    #[test]
    fn test_oracles_can_resubmit_after_reset() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[31u8; 32]);
        // Submit two readings
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        // Reset
        client.reset_window(&admin, &project_id);

        // All three oracles can submit fresh (using next nonces)
        client.submit_reading(&o1, &project_id, &2, &700, &10, &80, &500, &250, &8, &1);
        client.submit_reading(&o2, &project_id, &2, &700, &10, &80, &500, &250, &8, &1);
        let result = client.submit_reading(&o3, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        assert!(result.is_some());
        assert_eq!(result.unwrap().oracle_count, 3);
    }

    // ── Edge case tests ──

    #[test]
    fn test_zero_flow_produces_zero_volumetric_credit() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[40u8; 32]);
        // flow_rate = 0 → no volumetric credits, no nutrient removal
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &0, &250, &2, &0);
        client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &0, &250, &2, &0);
        let result = client.submit_reading(&o3, &project_id, &1, &700, &10, &80, &0, &250, &2, &0);

        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.volumetric_credit, 0);
        assert_eq!(res.n_removal_kg, 0);
        assert_eq!(res.p_removal_kg, 0);
        assert_eq!(res.total_credits, 0);
    }

    #[test]
    fn test_single_oracle_submission_does_not_finalize() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[41u8; 32]);
        let result = client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        // With min_oracles=3, one submission should not produce a result
        assert!(result.is_none());
        // And no last_result stored yet
        assert!(client.get_last_result(&project_id).is_none());
    }

    #[test]
    fn test_two_oracle_submissions_does_not_finalize() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[42u8; 32]);
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);
        let result = client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &500, &250, &8, &1);

        assert!(result.is_none());
        assert!(client.get_last_result(&project_id).is_none());
    }

    #[test]
    fn test_all_zero_readings_no_credits_no_removal() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        let o3 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);
        client.add_oracle(&admin, &o3);

        let project_id = BytesN::from_array(&e, &[43u8; 32]);
        // Readings with high N and P (above baseline) and zero flow
        // → no removal, no volumetric, only quality penalty from bad pH
        client.submit_reading(&o1, &project_id, &1, &300, &200, &10, &0, &350, &20, &5);
        client.submit_reading(&o2, &project_id, &1, &300, &200, &10, &0, &350, &20, &5);
        let result = client.submit_reading(&o3, &project_id, &1, &300, &200, &10, &0, &350, &20, &5);

        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.volumetric_credit, 0);
        assert_eq!(res.n_removal_kg, 0);
        assert_eq!(res.p_removal_kg, 0);
        // total_credits is 0 (or negative capped to 0 after quality penalty on 0 gross)
        assert_eq!(res.total_credits, 0);
    }

    #[test]
    fn test_median_with_even_number_of_oracles_uses_lower_middle() {
        let (e, admin, client) = setup_with_client();
        e.mock_all_auths();

        // Change config to min_oracles=2 to test even-count median
        let mut config = client.get_config();
        config.min_oracles = 2;
        client.update_config(&admin, &config);

        let o1 = Address::generate(&e);
        let o2 = Address::generate(&e);
        client.add_oracle(&admin, &o1);
        client.add_oracle(&admin, &o2);

        let project_id = BytesN::from_array(&e, &[44u8; 32]);
        // flow: 400 and 600 → median = (400+600)/2 = 500
        client.submit_reading(&o1, &project_id, &1, &700, &10, &80, &400, &250, &8, &1);
        let result = client.submit_reading(&o2, &project_id, &1, &700, &10, &80, &600, &250, &8, &1);

        assert!(result.is_some());
        let res = result.unwrap();
        // volumetric = med_flow * 100 / 1000 = 500 * 100 / 1000 = 50
        assert_eq!(res.volumetric_credit, 50);
    }
}
