#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

#[cfg(test)]
extern crate std;

const EVENT_PROPOSAL_CREATED: Symbol = symbol_short!("prop_crt");
const EVENT_PROPOSAL_EXECUTED: Symbol = symbol_short!("prop_exe");
const EVENT_VOTE_CAST: Symbol = symbol_short!("vote_cst");
const EVENT_MEMBER_ADDED: Symbol = symbol_short!("memb_add");
const EVENT_MEMBER_REMOVED: Symbol = symbol_short!("memb_rmv");

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Active,
    Approved,
    Executed,
    Rejected,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GovernanceConfig {
    pub fee_bps: u32,
    pub voting_period: u64,
    pub timelock_duration: u64,
    pub approval_threshold_bps: u32,
    pub min_proposal_deposit: i128,
    pub max_active_proposals: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub actions: Vec<GovernanceAction>,
    pub votes_for: Vec<Address>,
    pub votes_against: Vec<Address>,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub voting_ends_at: u64,
    pub timelock_ends_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GovernanceAction {
    pub target: Address,
    pub function: String,
    pub args: Vec<Symbol>,
}

#[contracttype]
pub struct VoteCounts {
    pub yes: u32,
    pub no: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Config,
    Member(Address),
    MemberCount,
    ProposalCount,
    Proposal(u64),
    HasVoted(u64, Address),
    ActiveProposals,
}

fn has_admin(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Admin)
}

fn read_admin(e: &Env) -> Address {
    e.storage().instance().get(&DataKey::Admin).unwrap()
}

fn read_config(e: &Env) -> GovernanceConfig {
    e.storage().instance().get(&DataKey::Config).unwrap()
}

fn is_member(e: &Env, addr: &Address) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Member(addr.clone()))
        .unwrap_or(false)
}

fn member_count(e: &Env) -> u32 {
    e.storage().instance().get(&DataKey::MemberCount).unwrap()
}

#[contract]
pub struct Governance;

#[contractimpl]
impl Governance {
    pub fn initialize(e: Env, admin: Address, initial_members: Vec<Address>) {
        if has_admin(&e) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Admin, &admin);

        let config = GovernanceConfig {
            fee_bps: 50,
            voting_period: 604800,
            timelock_duration: 86400,
            approval_threshold_bps: 6000,
            min_proposal_deposit: 1000,
            max_active_proposals: 10,
        };
        e.storage().instance().set(&DataKey::Config, &config);
        e.storage().instance().set(&DataKey::ProposalCount, &0u64);
        e.storage()
            .instance()
            .set(&DataKey::ActiveProposals, &Vec::<u64>::new(&e));

        let mut count: u32 = 0;
        for i in 0..initial_members.len() {
            let member = initial_members.get(i).unwrap();
            if !e.storage().instance().has(&DataKey::Member(member.clone())) {
                e.storage()
                    .instance()
                    .set(&DataKey::Member(member.clone()), &true);
                count += 1;
            }
        }
        e.storage().instance().set(&DataKey::MemberCount, &count);
    }

    pub fn get_config(e: Env) -> GovernanceConfig {
        read_config(&e)
    }

    pub fn get_proposal(e: Env, proposal_id: u64) -> Option<Proposal> {
        e.storage().instance().get(&DataKey::Proposal(proposal_id))
    }

    pub fn propose(
        e: Env,
        proposer: Address,
        title: String,
        description: String,
        actions: Vec<GovernanceAction>,
    ) -> u64 {
        proposer.require_auth();

        if !is_member(&e, &proposer) {
            panic!("not a governance member");
        }

        let count: u64 = e.storage().instance().get(&DataKey::ProposalCount).unwrap();
        let proposal_id = count + 1;
        let timestamp = e.ledger().timestamp();
        let config: GovernanceConfig = read_config(&e);

        // Check active proposal limit
        let active: Vec<u64> = e.storage().instance().get(&DataKey::ActiveProposals).unwrap();
        if active.len() >= config.max_active_proposals {
            panic!("too many active proposals");
        }

        if title.len() == 0 {
            panic!("title must not be empty");
        }

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            title,
            description,
            actions,
            votes_for: Vec::new(&e),
            votes_against: Vec::new(&e),
            status: ProposalStatus::Pending,
            created_at: timestamp,
            voting_ends_at: timestamp + config.voting_period,
            timelock_ends_at: 0,
        };

        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        let mut active = active;
        active.push_back(proposal_id);
        e.storage()
            .instance()
            .set(&DataKey::ActiveProposals, &active);
        e.storage()
            .instance()
            .set(&DataKey::ProposalCount, &proposal_id);

        e.events()
            .publish((EVENT_PROPOSAL_CREATED,), (proposal_id, proposer));

        proposal_id
    }

    pub fn vote(e: Env, voter: Address, proposal_id: u64, approve: bool) {
        voter.require_auth();

        if !is_member(&e, &voter) {
            panic!("not a governance member");
        }

        if e.storage()
            .instance()
            .has(&DataKey::HasVoted(proposal_id, voter.clone()))
        {
            panic!("already voted");
        }

        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));

        let timestamp = e.ledger().timestamp();

        // Auto-activate if past pending
        if matches!(proposal.status, ProposalStatus::Pending) {
            proposal.status = ProposalStatus::Active;
        }

        if !matches!(proposal.status, ProposalStatus::Active) {
            panic!("proposal not active");
        }

        if timestamp > proposal.voting_ends_at {
            proposal.status = ProposalStatus::Expired;
            e.storage()
                .instance()
                .set(&DataKey::Proposal(proposal_id), &proposal);
            panic!("voting period ended");
        }

        if approve {
            proposal.votes_for.push_back(voter.clone());
        } else {
            proposal.votes_against.push_back(voter.clone());
        }

        e.storage()
            .instance()
            .set(&DataKey::HasVoted(proposal_id, voter.clone()), &true);
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events()
            .publish((EVENT_VOTE_CAST,), (proposal_id, voter, approve));

        // Check if threshold reached
        let total_members = member_count(&e);
        let total_votes = proposal.votes_for.len() + proposal.votes_against.len();
        let config: GovernanceConfig = read_config(&e);

        if total_votes >= total_members {
            let yes_pct = if total_votes > 0 {
                (proposal.votes_for.len() as u32) * 10000 / total_votes
            } else {
                0
            };
            if yes_pct >= config.approval_threshold_bps {
                proposal.status = ProposalStatus::Approved;
                proposal.timelock_ends_at = timestamp + config.timelock_duration;
                e.storage()
                    .instance()
                    .set(&DataKey::Proposal(proposal_id), &proposal);
            } else {
                proposal.status = ProposalStatus::Rejected;
                e.storage()
                    .instance()
                    .set(&DataKey::Proposal(proposal_id), &proposal);
            }
        }
    }

    pub fn execute(e: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();

        if !is_member(&e, &caller) {
            panic!("not a governance member");
        }

        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));

        if !matches!(proposal.status, ProposalStatus::Approved) {
            panic!("proposal not approved");
        }

        let timestamp = e.ledger().timestamp();
        if timestamp < proposal.timelock_ends_at {
            panic!("timelock not elapsed");
        }

        proposal.status = ProposalStatus::Executed;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        // Remove from active list
        let active: Vec<u64> = e.storage().instance().get(&DataKey::ActiveProposals).unwrap();
        let mut new_active: Vec<u64> = Vec::new(&e);
        for i in 0..active.len() {
            let id = active.get(i).unwrap();
            if id != proposal_id {
                new_active.push_back(id);
            }
        }
        e.storage()
            .instance()
            .set(&DataKey::ActiveProposals, &new_active);

        e.events()
            .publish((EVENT_PROPOSAL_EXECUTED,), (proposal_id,));
    }

    pub fn update_config(e: Env, admin: Address, config: GovernanceConfig) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        e.storage().instance().set(&DataKey::Config, &config);
    }

    pub fn add_member(e: Env, admin: Address, new_member: Address) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        if e.storage()
            .instance()
            .has(&DataKey::Member(new_member.clone()))
        {
            panic!("already a member");
        }
        e.storage()
            .instance()
            .set(&DataKey::Member(new_member.clone()), &true);
        let count: u32 = e.storage().instance().get(&DataKey::MemberCount).unwrap();
        e.storage()
            .instance()
            .set(&DataKey::MemberCount, &(count + 1));

        e.events()
            .publish((EVENT_MEMBER_ADDED,), (new_member,));
    }

    pub fn remove_member(e: Env, admin: Address, member: Address) {
        admin.require_auth();
        let stored: Address = read_admin(&e);
        if admin != stored {
            panic!("unauthorized");
        }
        if !e.storage()
            .instance()
            .has(&DataKey::Member(member.clone()))
        {
            panic!("not a member");
        }
        let count: u32 = e.storage().instance().get(&DataKey::MemberCount).unwrap();
        if count <= 1 {
            panic!("cannot remove last member");
        }
        e.storage().instance().remove(&DataKey::Member(member.clone()));
        e.storage()
            .instance()
            .set(&DataKey::MemberCount, &(count - 1));

        e.events()
            .publish((EVENT_MEMBER_REMOVED,), (member,));
    }

    pub fn is_member_fn(e: Env, addr: Address) -> bool {
        is_member(&e, &addr)
    }

    pub fn member_count_fn(e: Env) -> u32 {
        member_count(&e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn setup() -> (Env, Address, Address, GovernanceClient<'static>) {
        let e = Env::default();
        let admin = Address::generate(&e);
        let member1 = Address::generate(&e);
        let contract_id = e.register_contract(None, Governance);
        let client = GovernanceClient::new(&e, &contract_id);

        let members: Vec<Address> = Vec::from_array(&e, [member1.clone()]);
        client.initialize(&admin, &members);

        (e, admin, member1, client)
    }

    #[test]
    fn test_initialize_sets_config_and_members() {
        let (_e, _admin, member1, client) = setup();
        let config = client.get_config();
        assert_eq!(config.fee_bps, 50);
        assert_eq!(config.approval_threshold_bps, 6000);
        assert!(client.is_member_fn(&member1));
        assert_eq!(client.member_count_fn(), 1);
    }

    #[test]
    fn test_propose_creates_proposal() {
        let (e, _admin, member1, client) = setup();
        e.mock_all_auths();

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Test Proposal"),
            &String::from_str(&e, "A test proposal"),
            &actions,
        );
        assert_eq!(id, 1);

        let proposal = client.get_proposal(&id).unwrap();
        assert_eq!(proposal.title, String::from_str(&e, "Test Proposal"));
        assert!(matches!(proposal.status, ProposalStatus::Pending));
    }

    #[test]
    fn test_non_member_rejected() {
        let (e, _admin, member1, client) = setup();
        let rogue = Address::generate(&e);
        assert!(client.is_member_fn(&member1));
        assert!(!client.is_member_fn(&rogue));
    }

    #[test]
    fn test_vote_approval() {
        let (e, admin, member1, client) = setup();
        e.mock_all_auths();

        // Add a second member so we have 2 total
        let member2 = Address::generate(&e);
        client.add_member(&admin, &member2);

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Vote Test"),
            &String::from_str(&e, "desc"),
            &actions,
        );

        client.vote(&member1, &id, &true);
        client.vote(&member2, &id, &true);

        let proposal = client.get_proposal(&id).unwrap();
        assert!(matches!(proposal.status, ProposalStatus::Approved));
    }

    #[test]
    fn test_vote_rejection() {
        let (e, admin, member1, client) = setup();
        e.mock_all_auths();

        let member2 = Address::generate(&e);
        client.add_member(&admin, &member2);

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Reject Test"),
            &String::from_str(&e, "desc"),
            &actions,
        );

        client.vote(&member1, &id, &false);
        client.vote(&member2, &id, &false);

        let proposal = client.get_proposal(&id).unwrap();
        assert!(matches!(proposal.status, ProposalStatus::Rejected));
    }

    #[test]
    fn test_vote_tracking() {
        let (e, _admin, member1, client) = setup();
        e.mock_all_auths();

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Vote Tracking"),
            &String::from_str(&e, "desc"),
            &actions,
        );

        client.vote(&member1, &id, &true);
        let proposal = client.get_proposal(&id).unwrap();
        assert_eq!(proposal.votes_for.len(), 1);
        assert_eq!(proposal.votes_against.len(), 0);
    }

    #[test]
    fn test_execute_after_timelock() {
        let e = Env::default();
        e.mock_all_auths();
        let admin = Address::generate(&e);
        let member1 = Address::generate(&e);
        let member2 = Address::generate(&e);
        let contract_id = e.register_contract(None, Governance);
        let client = GovernanceClient::new(&e, &contract_id);

        let members: Vec<Address> = Vec::from_array(&e, [member1.clone(), member2.clone()]);
        client.initialize(&admin, &members);

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Exec Test"),
            &String::from_str(&e, "desc"),
            &actions,
        );

        client.vote(&member1, &id, &true);
        client.vote(&member2, &id, &true);

        let proposal = client.get_proposal(&id).unwrap();
        assert!(matches!(proposal.status, ProposalStatus::Approved));

        // Jump past timelock
        let mut info = e.ledger().get();
        info.timestamp = proposal.timelock_ends_at + 1;
        e.ledger().set(info);

        client.execute(&member1, &id);

        let proposal = client.get_proposal(&id).unwrap();
        assert!(matches!(proposal.status, ProposalStatus::Executed));
    }

    #[test]
    fn test_timelock_not_elapsed() {
        let (e, admin, member1, client) = setup();
        e.mock_all_auths();

        let member2 = Address::generate(&e);
        client.add_member(&admin, &member2);

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Timelock Test"),
            &String::from_str(&e, "desc"),
            &actions,
        );

        client.vote(&member1, &id, &true);
        client.vote(&member2, &id, &true);

        let proposal = client.get_proposal(&id).unwrap();
        assert!(matches!(proposal.status, ProposalStatus::Approved));
        assert!(proposal.timelock_ends_at > e.ledger().timestamp());
    }

    #[test]
    fn test_add_member() {
        let (e, admin, _member1, client) = setup();
        e.mock_all_auths();

        let new_member = Address::generate(&e);
        client.add_member(&admin, &new_member);
        assert!(client.is_member_fn(&new_member));
        assert_eq!(client.member_count_fn(), 2);
    }

    #[test]
    fn test_remove_member() {
        let (e, admin, _member1, client) = setup();
        e.mock_all_auths();

        let member2 = Address::generate(&e);
        client.add_member(&admin, &member2);
        client.remove_member(&admin, &member2);
        assert!(!client.is_member_fn(&member2));
        assert_eq!(client.member_count_fn(), 1);
    }

    #[test]
    fn test_last_member_guard() {
        let (e, admin, _member1, client) = setup();
        e.mock_all_auths();

        // Add second member, remove it, then check count is still 1
        let member2 = Address::generate(&e);
        client.add_member(&admin, &member2);
        client.remove_member(&admin, &member2);
        assert_eq!(client.member_count_fn(), 1);
    }

    #[test]
    fn test_update_config_succeeds() {
        let (e, admin, _member1, client) = setup();
        e.mock_all_auths();

        let new_config = GovernanceConfig {
            fee_bps: 100,
            voting_period: 432000,
            timelock_duration: 43200,
            approval_threshold_bps: 5000,
            min_proposal_deposit: 500,
            max_active_proposals: 20,
        };
        client.update_config(&admin, &new_config);

        let config = client.get_config();
        assert_eq!(config.fee_bps, 100);
        assert_eq!(config.max_active_proposals, 20);
    }

    #[test]
    fn test_expired_proposal_state() {
        let (e, _admin, member1, client) = setup();
        e.mock_all_auths();

        let actions: Vec<GovernanceAction> = Vec::new(&e);
        let id = client.propose(
            &member1,
            &String::from_str(&e, "Expired"),
            &String::from_str(&e, "desc"),
            &actions,
        );

        // Jump past voting deadline
        let config = client.get_config();
        let mut info = e.ledger().get();
        info.timestamp = config.voting_period + 1;
        e.ledger().set(info);

        let proposal = client.get_proposal(&id).unwrap();
        assert!(proposal.voting_ends_at < e.ledger().timestamp());
    }

}
