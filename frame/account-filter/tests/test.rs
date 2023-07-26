mod mock;

use crate::mock::{test, AccountFilter, RuntimeOrigin, Test, VotesToAllow, BLOCKED_CALL};
use frame_support::{assert_noop, assert_ok, dispatch::DispatchInfo, pallet_prelude::*};
use sp_runtime::traits::SignedExtension;
use substrate_account_filter::{Error, Event};

#[test]
fn default_test() {
    test().execute_with(|| {
        assert!(AccountFilter::allowed_accounts_list(1u64).is_some());
        assert!(AccountFilter::allowed_accounts_list(2u64).is_some());
        assert!(AccountFilter::allowed_accounts_list(3u64).is_some());

        assert_eq!(AccountFilter::allowed_accounts(), 3u128);
    });
}

#[test]
fn one_vote_is_not_enough_to_add_account() {
    test().execute_with(|| {
        assert_eq!(AccountFilter::allowed_accounts_list(4u64), None);
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 4));
        mock::System::assert_has_event(
            Event::AccountVoted {
                referrer: 1u64,
                referee: 4u64,
            }
            .into(),
        );

        assert_eq!(AccountFilter::votes_for_account(4u64).unwrap(), 1u128);
        assert!(AccountFilter::allowed_accounts_list(4u64).is_none());
    });
}

#[test]
fn test_adding_to_allowlist() {
    test().execute_with(|| {
        assert_eq!(AccountFilter::allowed_accounts_list(4u64), None);
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 4));
        mock::System::assert_has_event(
            Event::AccountVoted {
                referrer: 1u64,
                referee: 4u64,
            }
            .into(),
        );
        assert_eq!(AccountFilter::votes_for_account(4u64).unwrap(), 1u128);
        assert!(AccountFilter::allowed_accounts_list(4u64).is_none());

        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(2), 4));
        mock::System::assert_has_event(
            Event::AccountVoted {
                referrer: 2u64,
                referee: 4u64,
            }
            .into(),
        );

        mock::System::assert_has_event(
            Event::AccountAllowed {
                account: 4u64,
                voted_for: vec![1u64, 2u64],
            }
            .into(),
        );

        assert!(AccountFilter::allowed_accounts_list(4u64).is_some());
        assert_eq!(AccountFilter::allowed_accounts(), 4u128);
    });
}

#[test]
fn complexity_growth_as_allowed_account_grow() {
    test().execute_with(|| {
        let initial_accounts = 3;
        // Add 10 accounts.
        for i in 0..10u64 {
            let account_to_add = 4 + i;
            let accounts = initial_accounts + i;
            assert_eq!(
                accounts as u128,
                substrate_account_filter::AllowedAccounts::<Test>::get(),
            );
            assert_eq!(AccountFilter::allowed_accounts_list(account_to_add), None);
            let votes_required = VotesToAllow::get().mul_ceil(accounts);

            for j in 0..votes_required {
                let account_to_vote = 1 + j;
                assert_ok!(AccountFilter::vote_for_account(
                    RuntimeOrigin::signed(account_to_vote),
                    account_to_add
                ));

                mock::System::assert_has_event(
                    Event::AccountVoted {
                        referrer: account_to_vote,
                        referee: account_to_add,
                    }
                    .into(),
                );
            }

            assert!(AccountFilter::allowed_accounts_list(account_to_add).is_some());
        }
    });
}

#[test]
fn failure_to_vote_with_wrong_origin() {
    test().execute_with(|| {
        assert_noop!(
            AccountFilter::vote_for_account(RuntimeOrigin::signed(0), 4),
            crate::Error::<Test>::NotAllowedToVote
        );
    });
}

#[test]
fn duplicate_adding_failure() {
    test().execute_with(|| {
        assert!(AccountFilter::allowed_accounts_list(2u64).is_some());
        assert_noop!(
            AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 2u64),
            Error::<Test>::AlreadyAllowed
        );
    });
}

#[test]
fn duplicate_voting_failure() {
    test().execute_with(|| {
        assert_eq!(AccountFilter::allowed_accounts_list(4u64), None);
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 4));
        assert_noop!(
            AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 4),
            Error::<Test>::DuplicateVote
        );
    });
}

#[test]
fn send_transfer_success() {
    test().execute_with(|| {
        let info = DispatchInfo::default();
        let len = 0_usize;
        assert!(AccountFilter::allowed_accounts_list(2u64).is_some());
        assert_ok!(
            substrate_account_filter::AllowAccount::<Test>::new().validate(
                &2,
                BLOCKED_CALL,
                &info,
                len
            )
        );
    });
}

#[test]
fn send_transfer_failure() {
    let mut ext = test();
    ext.execute_with(|| {
        let info = DispatchInfo::default();
        let len = 0_usize;
        assert_eq!(AccountFilter::allowed_accounts_list(4u64), None);
        assert_noop!(
            substrate_account_filter::AllowAccount::<Test>::new().validate(
                &4,
                BLOCKED_CALL,
                &info,
                len
            ),
            InvalidTransaction::BadSigner
        );
    });
}

#[test]
fn send_success_after_adding_account() {
    let mut ext = test();
    ext.execute_with(|| {
        let info = DispatchInfo::default();
        let len = 0_usize;
        assert_eq!(AccountFilter::allowed_accounts_list(4u64), None);
        assert_noop!(
            substrate_account_filter::AllowAccount::<Test>::new().validate(
                &4,
                BLOCKED_CALL,
                &info,
                len
            ),
            InvalidTransaction::BadSigner
        );
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 4));
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(2), 4));

        assert_ok!(
            substrate_account_filter::AllowAccount::<Test>::new().validate(
                &4,
                BLOCKED_CALL,
                &info,
                len
            )
        );
    });
}

#[test]
fn not_blocked_call_should_be_usable_by_any() {
    let mut ext = test();
    ext.execute_with(|| {
        let info = DispatchInfo::default();
        let len = 0_usize;
        assert_eq!(AccountFilter::allowed_accounts_list(4u64), None);

        let call = mock::RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
        assert_ok!(
            substrate_account_filter::AllowAccount::<Test>::new().validate(&4, &call, &info, len)
        );

        // Still can after adding account.
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(1), 4));
        assert_ok!(AccountFilter::vote_for_account(RuntimeOrigin::signed(2), 4));
        assert!(AccountFilter::allowed_accounts_list(4u64).is_some());
        assert_ok!(
            substrate_account_filter::AllowAccount::<Test>::new().validate(&4, &call, &info, len)
        );
    });
}
