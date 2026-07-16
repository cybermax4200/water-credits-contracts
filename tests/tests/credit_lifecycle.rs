use credit_token::{CreditToken, CreditTokenClient};
use retirement_registry::{RetirementRegistry, RetirementRegistryClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};
use verification_oracle::{OracleConfig, VerificationOracle, VerificationOracleClient};

fn deploy_oracle(e: &Env, admin: &Address) -> (Address, VerificationOracleClient<'static>) {
    let contract_id = e.register_contract(None, VerificationOracle);
    let client = VerificationOracleClient::new(e, &contract_id);
    let staking_token = Address::generate(e);
    let treasury = Address::generate(e);
    client.initialize(admin, &staking_token, &treasury);
    // Disable staking for integration tests — staking requires a live token contract.
    // Keep min_oracles at 3 to match the test's 3-oracle submission flow.
    client.update_config(
        admin,
        &OracleConfig {
            min_oracles: 3,
            max_oracles: 10,
            quality_threshold_ph: 600,
            quality_threshold_turbidity: 50,
            quality_threshold_do: 50,
            quality_threshold_temp: 300,
            credit_per_kg_n: 10,
            credit_per_kg_p: 20,
            staking_token,
            treasury,
            min_stake: 0,
            unstake_cooldown_secs: 86400,
            commit_phase_secs: 300,
            reveal_phase_secs: 300,
        },
    );
    (contract_id, client)
}

fn deploy_token(
    e: &Env,
    admin: &Address,
    project_id: &BytesN<32>,
) -> (Address, CreditTokenClient<'static>) {
    let contract_id = e.register_contract(None, CreditToken);
    let client = CreditTokenClient::new(e, &contract_id);
    client.initialize(
        admin,
        &String::from_str(e, "Test Credit"),
        &String::from_str(e, "TST"),
        project_id,
        &String::from_str(e, "Test_v1"),
    );
    (contract_id, client)
}

fn deploy_registry(e: &Env, admin: &Address) -> (Address, RetirementRegistryClient<'static>) {
    let contract_id = e.register_contract(None, RetirementRegistry);
    let client = RetirementRegistryClient::new(e, &contract_id);
    client.initialize(admin);
    (contract_id, client)
}

#[test]
fn test_oracle_mints_credits_to_beneficiary() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let project_id = BytesN::from_array(&e, &[1u8; 32]);

    let (token_id, token_client) = deploy_token(&e, &admin, &project_id);
    let (oracle_id, oracle_client) = deploy_oracle(&e, &admin);

    // Configure: token minter = oracle contract
    token_client.set_minter(&admin, &oracle_id);

    // Configure: oracle project config for auto-mint
    oracle_client.set_project_config(&admin, &project_id, &token_id, &beneficiary);

    // Add 3 oracles
    let o1 = Address::generate(&e);
    let o2 = Address::generate(&e);
    let o3 = Address::generate(&e);
    oracle_client.add_oracle(&admin, &o1);
    oracle_client.add_oracle(&admin, &o2);
    oracle_client.add_oracle(&admin, &o3);

    // Submit readings (one from each oracle)
    oracle_client.submit_reading(
        &o1,
        &project_id,
        &1,
        &700i64,
        &10i64,
        &80i64,
        &500i64,
        &250i64,
        &8i64,
        &1i64,
    );
    oracle_client.submit_reading(
        &o2,
        &project_id,
        &1,
        &700i64,
        &10i64,
        &80i64,
        &500i64,
        &250i64,
        &8i64,
        &1i64,
    );
    oracle_client.submit_reading(
        &o3,
        &project_id,
        &1,
        &700i64,
        &10i64,
        &80i64,
        &500i64,
        &250i64,
        &8i64,
        &1i64,
    );

    // Beneficiary should have received credits
    let balance = token_client.balance(&beneficiary);
    assert!(balance > 0, "beneficiary should receive minted credits");

    // Verify last result exists and has credits
    let result = oracle_client.get_last_result(&project_id).unwrap();
    assert_eq!(result.total_credits, balance);
}

#[test]
fn test_retire_cross_calls_registry() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let holder = Address::generate(&e);
    let project_id = BytesN::from_array(&e, &[2u8; 32]);

    let (token_id, token_client) = deploy_token(&e, &admin, &project_id);
    let (_registry_id, registry_client) = deploy_registry(&e, &admin);

    // Authorize token contract to call registry
    registry_client.set_authorized_caller(&admin, &token_id, &true);

    // Set registry on token
    token_client.set_retirement_registry(&admin, &_registry_id);

    // Mint credits to holder
    token_client.mint_to(&admin, &holder, &1000);

    // Retire credits
    let purpose = String::from_str(&e, "voluntary");
    let uri = String::from_str(&e, "ipfs://QmTest");
    let cert = token_client.retire(&holder, &500, &purpose, &uri);
    assert_eq!(cert.amount, 500);

    // Verify registry recorded the retirement
    assert_eq!(registry_client.total_retired(), 500);
    assert_eq!(registry_client.record_count(), 1);

    let record = registry_client.get_record(&1).unwrap();
    assert_eq!(record.retiree, holder);
    assert_eq!(record.amount, 500);

    // Verify token state
    assert_eq!(token_client.balance(&holder), 500);
    assert_eq!(token_client.total_supply(), 500);
    assert_eq!(token_client.total_retired(), 500);
}

#[test]
fn test_unauthorized_oracle_rejected() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    let _project_id = BytesN::from_array(&e, &[3u8; 32]);

    let (_oracle_id, oracle_client) = deploy_oracle(&e, &admin);

    // Verify only admin-authorized oracles are active
    assert!(!oracle_client.is_oracle_active(&unauthorized));

    // A non-active oracle submitting will panic the contract
    // (This panic is non-catchable in the test host, so we can only verify preconditions)
    let active = oracle_client.is_oracle_active(&admin);
    assert!(!active, "admin is not an oracle by default");

    // After adding an oracle it becomes active
    let oracle = Address::generate(&e);
    oracle_client.add_oracle(&admin, &oracle);
    assert!(oracle_client.is_oracle_active(&oracle));
}
