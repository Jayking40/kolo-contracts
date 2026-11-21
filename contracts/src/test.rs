#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::token;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, testutils::Address as _, testutils::Events,
    vec, Address, Env, IntoVal, String, Vec,
};

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let name = String::from_str(&env, "Test Group");
    let contribution_amount = 1000;

    client.initialize(
        &admin,
        &token,
        &name,
        &contribution_amount,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_double_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let name = String::from_str(&env, "Test Group");
    let contribution_amount = 1000;

    client.initialize(
        &admin,
        &token,
        &name,
        &contribution_amount,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );
    client.initialize(
        &admin,
        &token,
        &name,
        &contribution_amount,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );
}

#[test]
fn test_add_member() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let name = String::from_str(&env, "Test Group");
    let contribution_amount = 1000;

    client.initialize(
        &admin,
        &token,
        &name,
        &contribution_amount,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    env.mock_all_auths();
    let member1 = Address::generate(&env);
    client.add_member(&member1);
}

#[test]
#[should_panic(expected = "Not a member")]
fn test_contribute_not_member() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let name = String::from_str(&env, "Test Group");
    let contribution_amount = 1000;

    client.initialize(
        &admin,
        &token,
        &name,
        &contribution_amount,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    env.mock_all_auths();
    let not_member = Address::generate(&env);
    client.contribute(&not_member, &1000);
}

#[test]
#[should_panic(expected = "Already contributed this cycle")]
fn test_contribute_twice_same_cycle_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &1000);
    client.contribute(&member, &1000);
}

#[test]
fn test_contribute_allowed_after_reset() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &1000);
    client.reset_cycle();
    client.contribute(&member, &1000);

    assert_eq!(client.get_contribution(&member), 2000);
}

#[test]
fn test_events() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let name = String::from_str(&env, "Test Group");
    let contribution_amount = 1000;

    // 1. Test Initialize Event
    client.initialize(
        &admin,
        &token,
        &name,
        &contribution_amount,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                soroban_sdk::vec![&env, symbol_short!("init").into_val(&env)],
                (
                    admin.clone(),
                    token.clone(),
                    name.clone(),
                    contribution_amount
                )
                    .into_val(&env)
            )
        ]
    );

    // 2. Test Add Member Event
    let member1 = Address::generate(&env);
    client.add_member(&member1);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                soroban_sdk::vec![
                    &env,
                    symbol_short!("add_mem").into_val(&env),
                    member1.clone().into_val(&env)
                ],
                ().into_val(&env)
            )
        ]
    );
}

#[test]
fn test_goalbased_flexible_contributions() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Goal Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::GoalBased,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    // Can contribute varying amounts
    client.contribute(&member, &500);

    // Wait, the test above calls contribute twice in a row, but HasContributedThisCycle is still active.
    // So we need to call reset_cycle() or it will panic.
    client.reset_cycle();
    client.contribute(&member, &1500);

    assert_eq!(client.get_contribution(&member), 2000);
}

#[test]
fn test_goalbased_partial_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Goal Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::GoalBased,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &2000);

    assert_eq!(token_client.balance(&member), 3000);

    client.withdraw_savings(&member, &500);

    assert_eq!(client.get_contribution(&member), 1500);
    assert_eq!(token_client.balance(&member), 3500);
}

#[test]
#[should_panic(expected = "Insufficient savings to withdraw")]
fn test_goalbased_over_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Goal Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::GoalBased,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &1000);
    client.withdraw_savings(&member, &1500);
}

#[test]
#[should_panic(expected = "Target amount not reached yet")]
fn test_goalbased_locked_until_target() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Goal Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::GoalBased,
        &Some(2000), // target amount
        &true,       // lock until target
        &None,       // expected_cycle_days
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &1000);
    // Should panic because 1000 < 2000
    client.withdraw_savings(&member, &500);
}

#[test]
fn test_remove_member_no_contribution() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member1);
    client.add_member(&member2);

    client.remove_member(&member2);

    env.as_contract(&contract_id, || {
        let members: Vec<Address> = env.storage().instance().get(&DataKey::Members).unwrap();
        assert_eq!(members.len(), 1);
        assert!(members.contains(&member1));
        assert!(!members.contains(&member2));
    });
}

#[test]
fn test_remove_member_with_contribution_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member1);
    client.add_member(&member2);
    token_client.mint(&member1, &5000);
    token_client.mint(&member2, &5000);

    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);

    assert_eq!(token_client.balance(&member2), 4000);

    client.remove_member(&member2);

    assert_eq!(token_client.balance(&member2), 5000);

    env.as_contract(&contract_id, || {
        // MemberState entry removed entirely during remove_member (O(1) design)
        assert!(!env
            .storage()
            .instance()
            .has(&DataKey::Member(member2.clone())));
    });
}

#[test]
#[should_panic(expected = "Cannot remove member after their payout turn")]
fn test_remove_member_after_payout_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member1);
    client.add_member(&member2);
    token_client.mint(&member1, &5000);
    token_client.mint(&member2, &5000);

    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);

    client.payout(&member1, &None);

    client.remove_member(&member1);
}

#[test]
fn test_remove_member_adjusts_cycle_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    client.add_member(&member1);
    client.add_member(&member2);
    client.add_member(&member3);
    token_client.mint(&member1, &5000);
    token_client.mint(&member2, &5000);
    token_client.mint(&member3, &5000);

    client.contribute(&member1, &1000);

    env.as_contract(&contract_id, || {
        let count: i128 = env
            .storage()
            .instance()
            .get(&DataKey::CycleMemberCount)
            .unwrap();
        assert_eq!(count, 3);
    });

    client.remove_member(&member3);

    env.as_contract(&contract_id, || {
        let count_after: i128 = env
            .storage()
            .instance()
            .get(&DataKey::CycleMemberCount)
            .unwrap();
        assert_eq!(count_after, 2);
    });

    client.contribute(&member2, &1000);

    client.payout(&member1, &None);

    let contract_balance = token_client.balance(&contract_id);
    assert_eq!(contract_balance, 0);

    client.reset_cycle();

    env.as_contract(&contract_id, || {
        assert!(!env.storage().instance().has(&DataKey::CycleMemberCount));
    });
}

#[test]
fn test_remove_last_member_clears_cycle_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &1000);

    env.as_contract(&contract_id, || {
        let count: i128 = env
            .storage()
            .instance()
            .get(&DataKey::CycleMemberCount)
            .unwrap();
        assert_eq!(count, 1);
    });

    client.remove_member(&member);

    env.as_contract(&contract_id, || {
        assert!(!env.storage().instance().has(&DataKey::CycleMemberCount));
    });
}

#[test]
fn test_deterministic_payout_order() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);
    client.add_member(&member2);

    token_client.mint(&member0, &10000);
    token_client.mint(&member1, &10000);
    token_client.mint(&member2, &10000);

    // Payout 1 goes to member0 (index 0 in join order)
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member0);
    client.payout(&member0, &None);
    assert!(client.has_received_payout(&member0));
    assert!(!client.has_received_payout(&member1));
    assert!(!client.has_received_payout(&member2));
    // member0 gets the full pool: 1000 * 3 = 3000
    // balance = 10000 - 1000 (contrib) + 3000 (payout) = 12000
    assert_eq!(token_client.balance(&member0), 12000);

    // After payout, NextPayoutIndex advanced to 1 — member1 is next
    assert_eq!(client.get_next_payout_recipient(), member1);

    // Payout 2 goes to member1 (index 1) after reset + re-contribute
    client.reset_cycle();
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member1);
    client.payout(&member1, &None);
    assert!(client.has_received_payout(&member1));
    assert!(!client.has_received_payout(&member2));
    // 10000 - 1000 (round1) - 1000 (round2) + 3000 (payout) = 11000
    assert_eq!(token_client.balance(&member1), 11000);

    // Payout 3 goes to member2 (index 2)
    client.reset_cycle();
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member2);
    client.payout(&member2, &None);
    assert!(client.has_received_payout(&member2));
    // 10000 - 1000*3 (3 rounds) + 3000 (payout) = 10000
    assert_eq!(token_client.balance(&member2), 10000);
}

#[test]
fn test_queue_enforced_payout_order() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);
    client.add_member(&member2);

    token_client.mint(&member0, &10000);
    token_client.mint(&member1, &10000);
    token_client.mint(&member2, &10000);

    // Payout must go to member0 (index 0), admin cannot choose
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);

    assert_eq!(client.get_next_payout_recipient(), member0);
    client.payout(&member0, &None);
    assert!(client.has_received_payout(&member0));
    assert!(!client.has_received_payout(&member2));

    // After reset, queue advances to member1 (NextPayoutIndex persists)
    client.reset_cycle();
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);

    assert_eq!(client.get_next_payout_recipient(), member1);
    client.payout(&member1, &None);
    assert!(client.has_received_payout(&member1));
    assert!(!client.has_received_payout(&member2));

    // After another reset, finally member2
    client.reset_cycle();
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);

    assert_eq!(client.get_next_payout_recipient(), member2);
    client.payout(&member2, &None);
    assert!(client.has_received_payout(&member2));
}

#[test]
fn test_cycle_resets_and_starts_again() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);
    client.add_member(&member2);

    token_client.mint(&member0, &10000);
    token_client.mint(&member1, &10000);
    token_client.mint(&member2, &10000);

    // --- Full rotation: member0 → member1 → member2 ---
    // Round 1: payout to member0
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member0);
    client.payout(&member0, &None);
    assert!(client.has_received_payout(&member0));

    // Round 2: payout to member1
    client.reset_cycle();
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member1);
    client.payout(&member1, &None);
    assert!(client.has_received_payout(&member1));

    // Round 3: payout to member2
    client.reset_cycle();
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member2);
    client.payout(&member2, &None);
    assert!(client.has_received_payout(&member2));

    // --- Full reset: reset_rotation() resets NextPayoutIndex to 0 ---
    client.reset_cycle();
    client.reset_rotation();

    // After full reset, all has_received_payout flags are cleared
    assert!(!client.has_received_payout(&member0));
    assert!(!client.has_received_payout(&member1));
    assert!(!client.has_received_payout(&member2));

    // New rotation starts with member0 again
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);
    client.contribute(&member2, &1000);
    assert_eq!(client.get_next_payout_recipient(), member0);
    client.payout(&member0, &None);
    assert!(client.has_received_payout(&member0));
}

#[test]
#[should_panic(expected = "Contribution amount must be positive")]
fn test_initialize_with_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test"),
        &-1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );
}

#[test]
#[should_panic(expected = "Contribution amount must be positive")]
fn test_initialize_with_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test"),
        &0i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );
}

#[test]
#[should_panic(expected = "Contribution amount exceeds maximum limit")]
fn test_initialize_with_exceeding_max_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test"),
        &1_000_000_000_000_001i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );
}

#[test]
#[should_panic(expected = "Math overflow in pool calculation")]
fn test_payout_pool_size_overflow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    client.add_member(&member0);

    env.as_contract(&contract_id, || {
        // Force an overflow scenario
        env.storage()
            .instance()
            .set(&DataKey::ContributionAmount, &i128::MAX);
        env.storage()
            .instance()
            .set(&DataKey::CycleMemberCount, &2i128);
    });

    client.payout(&member0, &None);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_payout_wrong_recipient_auth_fails() {
    let env = Env::default();

    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    // Initialize with mock all auths to easily bypass initialization auth
    env.mock_all_auths();
    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);

    token_client.mint(&member0, &10000);
    client.contribute(&member0, &1000);

    // We only explicitly mock the auth for payout with the WRONG recipient (member1)
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "payout",
            args: (member1.clone(), None::<i128>).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    // Calling it with member0 should fail because auth is for member1
    client.payout(&member0, &None);
}

// ---------------------------------------------------------------------------
// Partial payout tests (proceedWithPartialPool support)
// ---------------------------------------------------------------------------

#[test]
fn test_payout_partial_amount_succeeds_and_advances_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);
    client.add_member(&member2);

    token_client.mint(&member0, &10000);
    token_client.mint(&member1, &10000);

    // Only member0 and member1 contribute this cycle — member2 defaults.
    // The full pool would be 1000 * 3 = 3000, but the contract only holds
    // 2000, so the admin proceeds with a partial payout of exactly that.
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);

    assert_eq!(client.get_next_payout_recipient(), member0);
    client.payout(&member0, &Some(2000i128));

    assert!(client.has_received_payout(&member0));
    // 10000 - 1000 (contrib) + 2000 (partial payout) = 11000
    assert_eq!(token_client.balance(&member0), 11000);
    assert_eq!(token_client.balance(&contract_id), 0);

    // Rotation still advances normally after a partial payout.
    assert_eq!(client.get_next_payout_recipient(), member1);
}

#[test]
#[should_panic(expected = "Payout amount exceeds the full computed pool")]
fn test_payout_partial_amount_above_full_pool_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);

    token_client.mint(&member0, &10000);
    token_client.mint(&member1, &10000);
    client.contribute(&member0, &1000);
    client.contribute(&member1, &1000);

    // Full pool is 1000 * 2 = 2000 — asking for more than that is rejected
    // even though the admin is authorizing it, so a compromised or buggy
    // caller can never drain more than a cycle's own pool.
    client.payout(&member0, &Some(2001i128));
}

#[test]
#[should_panic(expected = "Payout amount must be positive")]
fn test_payout_partial_amount_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    client.add_member(&member0);
    token_client.mint(&member0, &10000);
    client.contribute(&member0, &1000);

    client.payout(&member0, &Some(0i128));
}

#[test]
#[should_panic(expected = "Insufficient funds in contract for requested payout")]
fn test_payout_partial_amount_above_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member0 = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member0);
    client.add_member(&member1);
    client.add_member(&member2);

    token_client.mint(&member0, &10000);
    // Only member0 contributes — contract balance is 1000, but the full pool
    // (1000 * 3 = 3000) is still within bounds for an explicit request, so
    // this must fail on the balance check, not the pool-size ceiling check.
    client.contribute(&member0, &1000);

    client.payout(&member0, &Some(1500i128));
}

// ---------------------------------------------------------------------------
// Reentrancy / Checks-Effects-Interactions tests
// ---------------------------------------------------------------------------
//
// `MaliciousToken` is a minimal mock that implements just enough of the token
// interface (`balance` and `transfer`) for `token::Client` to invoke it from
// inside `KoloSavingsContract`. Its `transfer` function is where a real
// malicious or unusually-implemented token could try to "call back" into the
// savings contract mid-payout. We use it to:
//
//   1. Prove the CEI ordering: by the time `transfer` runs, the effects of
//      `payout()` (NextPayoutIndex / has_received_payout) are already
//      committed — observable via a read-only call made from inside the
//      token's `transfer` callback.
//   2. Prove the reentrancy guard: a callback that tries to invoke `payout()`
//      again while the outer `payout()` call is still executing must panic
//      with "Reentrancy detected", rather than draining the pool twice.
//
// Note: because Soroban contract invocations are atomic, a panic anywhere
// during a call unwinds and reverts *all* storage writes made during that
// top-level call. This means we can't inspect "partial" state after a failed
// transfer from outside — the whole test would revert. So the meaningful way
// to verify ordering is to observe state *during* the call, before it either
// completes or panics, which is exactly what these mocks do.

#[contracttype]
#[derive(Clone)]
enum MalKey {
    Kolo,
    Mode,
    ObservedHasPayout,
}

#[contract]
pub struct MaliciousToken;

#[contractimpl]
impl MaliciousToken {
    /// Configure which savings contract to target. Starts in "observe" mode (0).
    pub fn init(env: Env, kolo: Address) {
        env.storage().instance().set(&MalKey::Kolo, &kolo);
        env.storage().instance().set(&MalKey::Mode, &0u32);
    }

    /// 0 = passively observe state during transfer(); 1 = attempt a reentrant
    /// call into payout() during transfer().
    pub fn set_mode(env: Env, mode: u32) {
        env.storage().instance().set(&MalKey::Mode, &mode);
    }

    pub fn get_observed(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&MalKey::ObservedHasPayout)
            .unwrap_or(false)
    }

    pub fn balance(env: Env, _id: Address) -> i128 {
        let mode: u32 = env.storage().instance().get(&MalKey::Mode).unwrap_or(0);
        if mode == 2 {
            let kolo: Address = env.storage().instance().get(&MalKey::Kolo).unwrap();
            let client = KoloSavingsContractClient::new(&env, &kolo);
            // Attempt reentrancy from inside balance(), before transfer() is ever reached.
            client.payout(&kolo, &None); // recipient doesn't matter — should never get this far
        }
        i128::MAX
    }

    /// Called by KoloSavingsContract via `token::Client::transfer`. This is the
    /// "interaction" step where a malicious token gets a chance to re-enter.
    pub fn transfer(env: Env, _from: Address, to: Address, _amount: i128) {
        let kolo: Address = env.storage().instance().get(&MalKey::Kolo).unwrap();
        let mode: u32 = env.storage().instance().get(&MalKey::Mode).unwrap_or(0);

        if mode == 1 {
            // Attack: try to trigger a second payout to the same recipient
            // while the outer payout() call is still mid-flight.
            let client = KoloSavingsContractClient::new(&env, &kolo);
            client.payout(&to, &None);
        } else {
            // Observe: read back whether payout()'s effects were already
            // applied by the time this callback runs.
            let already_marked: bool = env.as_contract(&kolo, || {
                let state: Option<MemberState> =
                    env.storage().persistent().get(&DataKey::Member(to.clone()));
                state.map(|s| s.has_received_payout).unwrap_or(false)
            });
            env.storage()
                .instance()
                .set(&MalKey::ObservedHasPayout, &already_marked);
        }
    }
}

fn setup_with_malicious_token(
    env: &Env,
) -> (
    Address,
    KoloSavingsContractClient<'_>,
    Address,
    MaliciousTokenClient<'_>,
) {
    let kolo_id = env.register_contract(None, KoloSavingsContract);
    let kolo_client = KoloSavingsContractClient::new(env, &kolo_id);

    let mal_token_id = env.register_contract(None, MaliciousToken);
    let mal_client = MaliciousTokenClient::new(env, &mal_token_id);

    let admin = Address::generate(env);
    let name = String::from_str(env, "Test Group");

    kolo_client.initialize(
        &admin,
        &mal_token_id,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    (kolo_id, kolo_client, mal_token_id, mal_client)
}

/// Requirement: verify the CEI pattern is implemented — state (has_received_payout,
/// via NextPayoutIndex) must already reflect the payout by the time the external
/// token transfer callback executes.
#[test]
fn test_cei_effects_committed_before_external_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let (kolo_id, kolo_client, _mal_token_id, mal_client) = setup_with_malicious_token(&env);

    let member = Address::generate(&env);
    kolo_client.add_member(&member);

    mal_client.init(&kolo_id);
    mal_client.set_mode(&0u32);

    kolo_client.contribute(&member, &1000);
    assert!(!mal_client.get_observed()); // contribute's own transfer targets the contract, not a member

    kolo_client.payout(&member, &None);

    // The MaliciousToken observed, from *inside* the transfer() callback, that
    // has_received_payout(member) was already true — proving effects were
    // committed before the interaction, per CEI.
    assert!(mal_client.get_observed());
}

/// Requirement: a malicious token attempting to re-enter payout() during its
/// transfer() callback must be blocked by the reentrancy guard.
#[test]
#[should_panic(expected = "Error(Context, InvalidAction)")]
fn test_reentrant_payout_call_is_blocked() {
    let env = Env::default();
    env.mock_all_auths();

    let (kolo_id, kolo_client, _mal_token_id, mal_client) = setup_with_malicious_token(&env);

    let member = Address::generate(&env);
    kolo_client.add_member(&member);

    mal_client.init(&kolo_id);

    // Contribute first while still in observe mode (mode 0) so the deposit
    // succeeds normally.
    kolo_client.contribute(&member, &1000);

    // Now arm the attack: the next transfer() call (triggered by payout())
    // will try to call payout() again for the same recipient.
    mal_client.set_mode(&1u32);

    // This should panic with "Reentrancy detected" because the guard set at
    // the top of payout() is still held when the reentrant call happens.
    kolo_client.payout(&member, &None);
}

/// Requirement: contribute() must also refuse to run if a payout is (somehow)
/// already mid-execution, as a defense-in-depth measure.
#[test]
#[should_panic(expected = "Reentrancy detected")]
fn test_contribute_blocked_while_payout_executing() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);
    let name = String::from_str(&env, "Test Group");

    client.initialize(
        &admin,
        &token,
        &name,
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    // Simulate being mid-payout by setting the guard flag directly.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::IsExecutingPayout, &true);
    });

    client.contribute(&member, &1000);
}

#[test]
#[should_panic(expected = "Contract is paused for emergency")]
fn test_pause_blocks_contribute() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.pause();
    client.contribute(&member, &1000);
}

#[test]
#[should_panic(expected = "Error(Context, InvalidAction)")]
fn test_reentrant_payout_call_via_balance_is_blocked() {
    let env = Env::default();
    env.mock_all_auths();
    let (kolo_id, kolo_client, _mal_token_id, mal_client) = setup_with_malicious_token(&env);

    let member = Address::generate(&env);
    kolo_client.add_member(&member);
    mal_client.init(&kolo_id);

    kolo_client.contribute(&member, &1000);

    mal_client.set_mode(&2u32); // reenter via balance(), not transfer()
    kolo_client.payout(&member, &None);
}

#[test]
fn test_unpause_allows_contribute() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.pause();
    client.unpause();
    client.contribute(&member, &1000);

    assert_eq!(client.get_contribution(&member), 1000);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_non_admin_cannot_pause() {
    let env = Env::default();

    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    // No auth mocked for pause — should fail since caller isn't the admin.
    let attacker = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.pause();
}

#[test]
fn test_emergency_withdraw_full_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.add_member(&member1);
    client.add_member(&member2);
    token_client.mint(&member1, &5000);
    token_client.mint(&member2, &5000);

    client.contribute(&member1, &1000); // contract: 1000, CycleMemberCount frozen at 2
    client.contribute(&member2, &1000); // contract: 2000

    // Bug discovered / admin vanishes — pause and let member1 exit.
    client.pause();
    client.emergency_withdraw(&member1); // contract: 1000 (refunded to member1)

    assert_eq!(token_client.balance(&member1), 5000); // got their 1000 back
    assert_eq!(client.get_contribution(&member1), 0);

    env.as_contract(&contract_id, || {
        let count: i128 = env
            .storage()
            .instance()
            .get(&DataKey::CycleMemberCount)
            .unwrap();
        assert_eq!(count, 1); // decremented from 2 to 1
    });

    // Unpause: member1 re-contributes, and it's still their turn to be paid
    // (NextPayoutIndex never moved — emergency withdraw doesn't jump the queue).
    client.unpause();
    client.contribute(&member1, &1000); // contract: 2000 again

    assert_eq!(client.get_next_payout_recipient(), member1);
    client.payout(&member1, &None);

    // pool_size = contribution_amount(1000) * frozen CycleMemberCount(1) = 1000
    // member1: 4000 (after re-contribute) + 1000 (payout) = 5000
    assert_eq!(token_client.balance(&member1), 5000);
    assert!(client.has_received_payout(&member1));

    // member2's 1000 is still safely in the pool, untouched, waiting for their turn.
    assert_eq!(client.get_contribution(&member2), 1000);
    assert_eq!(client.get_next_payout_recipient(), member2);
}

#[test]
#[should_panic(expected = "No contribution to withdraw this cycle")]
fn test_emergency_withdraw_twice_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(&env, "Test Group"),
        &1000i128,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let member = Address::generate(&env);
    client.add_member(&member);
    token_client.mint(&member, &5000);

    client.contribute(&member, &1000);
    client.pause();
    client.emergency_withdraw(&member);
    // Second call should panic — no contribution left this cycle.
    client.emergency_withdraw(&member);
}

// ---------------------------------------------------------------------------
// Balance accounting tests (Wave 037)
// ---------------------------------------------------------------------------
//
// These tests verify that the contract's contribution-based readiness logic
// is fully independent of its actual token balance.  Soroban tokens can be
// transferred directly to any contract address (there is no receive hook the
// contract could use to block them), so the contract must never rely on
// balance alone to determine whether a cycle is funded or a payout is ready.
//
// Covered scenarios:
//   - Exact funding: balance equals recorded contributions after a full cycle.
//   - Surplus funding: a direct transfer inflates balance but cannot satisfy
//     contribution checks or make an unfunded cycle "ready."
//   - Partial payout: balance and recorded contributions diverge correctly
//     after a partial payout.
//   - Withdrawal accounting: GoalBased withdrawals reduce recorded
//     contributions and balance by the same amount.
//   - External funding before payout: surplus balance does not let the admin
//     skip the contribution requirement.
//   - Full-pool readiness vs. balance: a surplus balance cannot substitute
//     for missing cycle contributions, raise the ceiling on a partial payout,
//     fund a second payout in the same cycle, or backfill an emergency refund.

// -- helpers --

/// Set up a standard rotational group with a real Stellar token.
/// Returns (contract_id, client, token_address, token_admin_client, admin, [member0, member1, member2]).
fn setup_rotational_group(
    env: &Env,
    contribution: i128,
) -> (
    Address,
    KoloSavingsContractClient<'_>,
    Address,
    token::StellarAssetClient<'_>,
    Address,
    [Address; 3],
) {
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(env, "Balance Test Group"),
        &contribution,
        &GroupType::Rotational,
        &None,
        &false,
        &None,
    );

    let m0 = Address::generate(env);
    let m1 = Address::generate(env);
    let m2 = Address::generate(env);
    client.add_member(&m0);
    client.add_member(&m1);
    client.add_member(&m2);

    token_client.mint(&m0, &10_000);
    token_client.mint(&m1, &10_000);
    token_client.mint(&m2, &10_000);

    (
        contract_id,
        client,
        token,
        token_client,
        admin,
        [m0, m1, m2],
    )
}

/// Set up a GoalBased group with one member. Returns the usual tuple plus the member address.
fn setup_goalbased_group(
    env: &Env,
    contribution: i128,
    target: Option<i128>,
    lock: bool,
) -> (
    Address,
    KoloSavingsContractClient<'_>,
    Address,
    token::StellarAssetClient<'_>,
    Address,
) {
    let contract_id = env.register_contract(None, KoloSavingsContract);
    let client = KoloSavingsContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(env, &token);

    client.initialize(
        &admin,
        &token,
        &String::from_str(env, "Goal Balance Group"),
        &contribution,
        &GroupType::GoalBased,
        &target,
        &lock,
        &None,
    );

    let member = Address::generate(env);
    client.add_member(&member);
    token_client.mint(&member, &20_000);

    (contract_id, client, token, token_client, member)
}

// -- exact funding --

/// When every member contributes once, the contract balance must equal the sum
/// of contributions and `get_balance()` must reflect that precisely.
#[test]
fn test_balance_exact_after_full_contributions() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    for m in &members {
        client.contribute(m, &1000);
    }

    // Balance is exactly 3 × 1000 = 3000, matching the recorded contributions.
    assert_eq!(client.get_balance(), 3000);
    assert_eq!(token_client.balance(&contract_id), 3000);

    // Individual contribution records are consistent.
    for m in &members {
        assert_eq!(client.get_contribution(m), 1000);
    }
}

// -- surplus funding --

/// A direct token transfer (mint to contract) inflates the token balance but
/// does NOT affect any member's recorded contribution state. The surplus is
/// invisible to contribution accounting.
#[test]
fn test_surplus_transfer_does_not_affect_contribution_state() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // Only member0 contributes through the contract.
    client.contribute(&members[0], &1000);

    // An external actor sends 5000 tokens directly to the contract.
    token_client.mint(&contract_id, &5000);

    // Token balance reflects the surplus: 1000 (contribution) + 5000 (direct) = 6000
    assert_eq!(client.get_balance(), 6000);

    // But contribution records are unchanged — only the legitimate contribution counts.
    assert_eq!(client.get_contribution(&members[0]), 1000);
    assert_eq!(client.get_contribution(&members[1]), 0);
    assert_eq!(client.get_contribution(&members[2]), 0);
}

/// A surplus balance does not relax the once-per-cycle contribution rule:
/// even with far more tokens in the contract than the pool needs, a second
/// contribution from the same member in the same cycle is rejected.
#[test]
#[should_panic(expected = "Already contributed this cycle")]
fn test_surplus_cannot_bypass_contribution_check() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // Flood the contract with surplus tokens.
    token_client.mint(&contract_id, &100_000);

    // member0 contributes legitimately once.
    client.contribute(&members[0], &1000);

    // Despite the huge balance, contributing again in the same cycle is rejected.
    client.contribute(&members[0], &1000);
}

/// Surplus funding does not let the admin trigger a payout when no contributions
/// have been made in the cycle. The payout path requires `CycleMemberCount`
/// (frozen on first contribution), not just a sufficient balance.
#[test]
#[should_panic(expected = "No active cycle")]
fn test_surplus_cannot_trigger_payout_without_contributions() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // No one contributes, but the contract has tokens from a direct transfer.
    token_client.mint(&contract_id, &50_000);

    // The admin cannot pay out — CycleMemberCount was never frozen because
    // no one contributed, so the contract has "no active cycle."
    client.payout(&members[0], &None);
}

// -- partial payout --

/// After a partial payout, the contract balance drops by the payout amount
/// while the members' total_contributions records stay at their original
/// values (contributions are lifetime sums, payouts don't debit them).
#[test]
fn test_balance_after_partial_payout() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // Two out of three contribute — balance = 2000, full pool = 3000.
    client.contribute(&members[0], &1000);
    client.contribute(&members[1], &1000);

    // Admin proceeds with a partial payout (only what's available).
    client.payout(&members[0], &Some(1500));

    // Balance: 2000 - 1500 = 500 remaining.
    assert_eq!(client.get_balance(), 500);
    assert_eq!(token_client.balance(&contract_id), 500);

    // Contribution records are unaffected by the payout.
    assert_eq!(client.get_contribution(&members[0]), 1000);
    assert_eq!(client.get_contribution(&members[1]), 1000);

    // Recipient received their partial payout.
    // member0 started with 10000, contributed 1000, received 1500 → 10500
    assert_eq!(token_client.balance(&members[0]), 10_500);
}

// -- withdrawal accounting (GoalBased) --

/// A GoalBased withdrawal decreases both the recorded contributions and the
/// contract balance by exactly the withdrawn amount. The two must stay in sync.
#[test]
fn test_withdrawal_keeps_balance_and_contributions_in_sync() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, member) =
        setup_goalbased_group(&env, 1000, None, false);

    // GoalBased contributions can be any positive amount.
    client.contribute(&member, &3000);
    assert_eq!(client.get_balance(), 3000);
    assert_eq!(client.get_contribution(&member), 3000);

    // Withdraw 1200.
    client.withdraw_savings(&member, &1200);

    assert_eq!(client.get_balance(), 1800);
    assert_eq!(token_client.balance(&contract_id), 1800);
    assert_eq!(client.get_contribution(&member), 1800);
    // Member wallet: 20000 - 3000 + 1200 = 18200
    assert_eq!(token_client.balance(&member), 18_200);
}

/// An external transfer to a GoalBased contract does not inflate recorded
/// savings, so the member cannot withdraw more than they contributed.
#[test]
#[should_panic(expected = "Insufficient savings to withdraw")]
fn test_surplus_does_not_inflate_goalbased_savings() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, member) =
        setup_goalbased_group(&env, 1000, None, false);

    client.contribute(&member, &1000);

    // External surplus — the contract now holds 6000 tokens.
    token_client.mint(&contract_id, &5000);
    assert_eq!(client.get_balance(), 6000);

    // But the member only contributed 1000, so withdrawing 2000 must fail
    // based on the recorded contribution, not the inflated balance.
    client.withdraw_savings(&member, &2000);
}

// -- external funding and readiness --

/// Contributions and balance diverge when external funding is added, but the
/// payout still transfers the correct amount and the resulting balance reflects
/// both the contributions and the surplus.
#[test]
fn test_external_funding_payout_uses_pool_not_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // All three members contribute.
    for m in &members {
        client.contribute(m, &1000);
    }
    assert_eq!(client.get_balance(), 3000);

    // An external actor adds 2000 more.
    token_client.mint(&contract_id, &2000);
    assert_eq!(client.get_balance(), 5000);

    // Payout is computed from contribution_amount × frozen_count = 1000 × 3 = 3000,
    // NOT from the inflated 5000 balance.
    client.payout(&members[0], &None);

    // 5000 - 3000 (pool payout) = 2000 left (the surplus).
    assert_eq!(client.get_balance(), 2000);
    assert_eq!(token_client.balance(&contract_id), 2000);

    // Recipient: 10000 - 1000 + 3000 = 12000
    assert_eq!(token_client.balance(&members[0]), 12_000);
}

/// Even with massive surplus funding, the payout amount is bounded by the
/// computed pool (contribution_amount × frozen_count), never by the balance.
#[test]
#[should_panic(expected = "Payout amount exceeds the full computed pool")]
fn test_surplus_cannot_increase_payout_above_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    for m in &members {
        client.contribute(m, &1000);
    }

    // Contract now holds a lot more than the pool.
    token_client.mint(&contract_id, &100_000);

    // Admin tries to pay out 50000, which is well within balance but far
    // above the computed pool of 3000. This must be rejected.
    client.payout(&members[0], &Some(50_000));
}

// -- full-pool readiness vs. balance --

/// A cycle is NOT "ready" for a full payout just because the balance covers
/// the pool. With only one of three members contributing, a 100k surplus must
/// not let `payout(None)` pay the 3000 full pool — two thirds of it would be
/// funded by tokens no member contributed this cycle.
#[test]
#[should_panic(expected = "Payout exceeds contributions recorded this cycle")]
fn test_pool_readiness_requires_contributions_not_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // Only member0 contributes. CycleMemberCount freezes at 3 on first contribution.
    client.contribute(&members[0], &1000);

    // Dump enough tokens directly to cover any imaginable payout.
    token_client.mint(&contract_id, &100_000);
    assert_eq!(client.get_balance(), 101_000);

    client.payout(&members[0], &None);
}

/// The most tempting shortcut: someone transfers exactly the missing member's
/// share straight to the contract. The balance now equals the full pool, but
/// the cycle is still one contribution short and the full payout is refused.
#[test]
#[should_panic(expected = "Payout exceeds contributions recorded this cycle")]
fn test_direct_transfer_of_missing_share_does_not_make_cycle_ready() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    client.contribute(&members[0], &1000);
    client.contribute(&members[1], &1000);
    // member2's 1000 arrives as a plain token transfer, not via contribute().
    token_client.mint(&contract_id, &1000);
    assert_eq!(client.get_balance(), 3000);

    client.payout(&members[0], &None);
}

/// Partial payouts are capped by recorded contributions too. One member has
/// contributed 1000; a surplus makes 1500 affordable on balance alone, and
/// 1500 is under the 3000 full pool, but it is still more than was paid in.
#[test]
#[should_panic(expected = "Payout exceeds contributions recorded this cycle")]
fn test_partial_payout_capped_by_recorded_contributions_not_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    client.contribute(&members[0], &1000);
    token_client.mint(&contract_id, &5000);

    client.payout(&members[0], &Some(1500));
}

/// Once a cycle's contributions have been paid out in full, surplus tokens
/// cannot fund another payout in that same cycle. The next recipient has to
/// wait for `reset_cycle()` and a fresh round of contributions.
#[test]
#[should_panic(expected = "Payout exceeds contributions recorded this cycle")]
fn test_surplus_cannot_fund_second_payout_in_same_cycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    for m in &members {
        client.contribute(m, &1000);
    }
    client.payout(&members[0], &None);

    token_client.mint(&contract_id, &3000);
    assert_eq!(client.get_balance(), 3000);

    client.payout(&members[1], &None);
}

/// Contributions left over after a partial payout can still be paid out in
/// the same cycle, down to the last unit, and no further — even with surplus
/// sitting in the contract. Every transfer matches a recorded contribution.
#[test]
fn test_partial_payouts_draw_down_recorded_contributions_exactly() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    client.contribute(&members[0], &1000);
    client.contribute(&members[1], &1000);
    token_client.mint(&contract_id, &4000);
    // Recorded this cycle: 2000. Balance: 6000.

    client.payout(&members[0], &Some(1500));
    client.payout(&members[1], &Some(500));
    assert_eq!(token_client.balance(&members[0]), 10_500); // 10000 - 1000 + 1500
    assert_eq!(token_client.balance(&members[1]), 9_500); // 10000 - 1000 + 500

    // All 2000 recorded this cycle has been paid out; only the surplus is left.
    assert_eq!(client.get_balance(), 4000);
    assert!(client.try_payout(&members[2], &Some(1)).is_err());
    assert_eq!(client.get_next_payout_recipient(), members[2]);
}

/// An emergency refund takes the member's contribution out of the cycle's
/// recorded funding. A direct transfer that "replaces" it does not restore it:
/// the full pool shrinks with the frozen count and the payout moves exactly
/// the remaining recorded contributions, leaving the transfer behind.
#[test]
fn test_emergency_refund_is_not_backfilled_by_direct_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    for m in &members {
        client.contribute(m, &1000);
    }

    client.pause();
    client.emergency_withdraw(&members[2]);
    client.unpause();
    assert_eq!(token_client.balance(&members[2]), 10_000);

    token_client.mint(&contract_id, &1000);
    assert_eq!(client.get_balance(), 3000);

    // Frozen count is now 2, so the full pool is 2000 — the 1000 transfer is
    // not part of it.
    client.payout(&members[0], &None);
    assert_eq!(token_client.balance(&members[0]), 11_000); // 10000 - 1000 + 2000
    assert_eq!(client.get_balance(), 1000);
}

/// After a full cycle payout, the contract balance should reflect the surplus
/// (if any) and the contributions consumed. An external transfer between
/// contribution and payout should be fully accounted for.
#[test]
fn test_balance_accounting_across_full_cycle_with_external_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // Full cycle: everyone contributes.
    for m in &members {
        client.contribute(m, &1000);
    }
    // Balance: 3000 from contributions.
    assert_eq!(client.get_balance(), 3000);

    // Someone sends 500 tokens directly between contributions and payout.
    token_client.mint(&contract_id, &500);
    assert_eq!(client.get_balance(), 3500);

    // Pool = 1000 × 3 = 3000. The 500 surplus stays.
    client.payout(&members[0], &None);
    assert_eq!(client.get_balance(), 500);

    // Start the next cycle, everyone contributes again.
    client.reset_cycle();
    for m in &members {
        client.contribute(m, &1000);
    }
    // Balance: 500 (leftover) + 3000 (new contributions) = 3500
    assert_eq!(client.get_balance(), 3500);

    // Payout member1 with the full pool.
    client.payout(&members[1], &None);
    // 3500 - 3000 = 500 — the surplus persists across cycles.
    assert_eq!(client.get_balance(), 500);
}

/// Surplus funding does not let a member who hasn't contributed receive credit
/// for a contribution. After a direct transfer, the member's contribution
/// record is still zero and they still need to call `contribute()`.
#[test]
fn test_external_transfer_does_not_credit_member_contribution() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // External transfer of exactly one contribution amount to the contract,
    // simulating what might happen if someone transferred tokens "on behalf of"
    // a member without going through contribute().
    token_client.mint(&contract_id, &1000);

    // Despite 1000 tokens in the contract, no member has a recorded contribution.
    assert_eq!(client.get_contribution(&members[0]), 0);
    assert_eq!(client.get_contribution(&members[1]), 0);
    assert_eq!(client.get_contribution(&members[2]), 0);

    // And the cycle has not started — CycleMemberCount is not frozen yet.
    env.as_contract(&contract_id, || {
        assert!(!env.storage().instance().has(&DataKey::CycleMemberCount));
    });
}

/// After multiple operations (contributions, partial payouts, external transfers,
/// cycle resets), the contract balance must always equal the token balance and
/// each operation must move the balance by exactly its stated amount.
#[test]
fn test_balance_tracking_through_mixed_operations() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, _admin, members) =
        setup_rotational_group(&env, 1000);

    // Step 1: external transfer.
    token_client.mint(&contract_id, &750);
    assert_eq!(client.get_balance(), 750);

    // Step 2: legitimate contributions.
    client.contribute(&members[0], &1000);
    client.contribute(&members[1], &1000);
    // 750 + 2000 = 2750
    assert_eq!(client.get_balance(), 2750);

    // Step 3: partial payout (pool = 1000 × 3 = 3000; request 2000 ≤ pool, ≤ balance).
    client.payout(&members[0], &Some(2000));
    // 2750 - 2000 = 750
    assert_eq!(client.get_balance(), 750);
    assert_eq!(token_client.balance(&contract_id), 750);

    // Step 4: more external funding.
    token_client.mint(&contract_id, &250);
    assert_eq!(client.get_balance(), 1000);

    // Step 5: next cycle, full contributions, full payout.
    client.reset_cycle();
    client.contribute(&members[0], &1000);
    client.contribute(&members[1], &1000);
    client.contribute(&members[2], &1000);
    // 1000 + 3000 = 4000
    assert_eq!(client.get_balance(), 4000);

    // Pool = 1000 × 3 = 3000
    client.payout(&members[1], &None);
    assert_eq!(client.get_balance(), 1000);
    assert_eq!(token_client.balance(&contract_id), 1000);
}

/// GoalBased: a surplus transfer followed by a legitimate contribution and
/// withdrawal must leave the balance exactly reflecting both the surplus
/// and the net contribution movement.
#[test]
fn test_goalbased_balance_with_surplus_and_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, member) =
        setup_goalbased_group(&env, 1000, None, false);

    // External surplus.
    token_client.mint(&contract_id, &800);
    assert_eq!(client.get_balance(), 800);

    // Legitimate contribution.
    client.contribute(&member, &2000);
    // 800 + 2000 = 2800
    assert_eq!(client.get_balance(), 2800);
    assert_eq!(client.get_contribution(&member), 2000);

    // Partial withdrawal.
    client.withdraw_savings(&member, &600);
    // 2800 - 600 = 2200; recorded contributions: 2000 - 600 = 1400
    assert_eq!(client.get_balance(), 2200);
    assert_eq!(client.get_contribution(&member), 1400);
    // member: 20000 - 2000 + 600 = 18600
    assert_eq!(token_client.balance(&member), 18_600);

    // The 800 surplus is still sitting in the contract on top of the 1400
    // in legitimate savings. Only the 1400 is withdrawable.
}

/// GoalBased with lock: surplus tokens cannot help the member reach the target
/// amount for unlocking withdrawals. Only actual contributions count.
#[test]
#[should_panic(expected = "Target amount not reached yet")]
fn test_surplus_does_not_satisfy_goalbased_lock_target() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, token_client, member) =
        setup_goalbased_group(&env, 1000, Some(5000), true);

    // Contribute 2000 through the contract.
    client.contribute(&member, &2000);

    // Dump tokens to bring balance well above the 5000 target.
    token_client.mint(&contract_id, &10_000);
    assert_eq!(client.get_balance(), 12_000);

    // But recorded contributions are only 2000 — below the 5000 lock target.
    assert_eq!(client.get_contribution(&member), 2000);

    // Withdrawal must still be blocked by the target lock.
    client.withdraw_savings(&member, &500);
}
