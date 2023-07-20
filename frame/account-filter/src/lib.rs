//! # Account Filter Pallet
//!
//! The Account Filter Pallet provides functionality to restrict extrinsic submission to a set of
//! allowed accounts. The filtering of accounts is done during the transaction queue validation.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub trait BlockCallMatcher<T: Config> {
    fn matches(call: &<T as frame_system::Config>::RuntimeCall) -> bool;
}

#[frame_support::pallet]
pub mod pallet {

    use super::*;
    use frame_support::dispatch::DispatchInfo;
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use parity_scale_codec::{Decode, Encode};
    use scale_info::TypeInfo;
    use sp_runtime::Percent;
    use sp_runtime::{
        traits::{DispatchInfoOf, Dispatchable, SignedExtension},
        transaction_validity::{
            InvalidTransaction, TransactionLongevity, TransactionValidity,
            TransactionValidityError, ValidTransaction,
        },
    };
    use sp_std::convert::TryInto;
    use sp_std::fmt::Debug;
    use sp_std::marker::PhantomData;
    use sp_std::prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type CallsToFilter: BlockCallMatcher<Self>;
        type VotesToAllow: Get<Percent>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // The allowed accounts.
    #[pallet::storage]
    #[pallet::getter(fn allowed_accounts)]
    pub type AllowedAccounts<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, ()>;

    // Voting process for the allow-list.
    // The key is the account that is being voted for. The value is the account that is voting for.
    #[pallet::storage]
    #[pallet::getter(fn votes)]
    pub type Votes<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, T::AccountId, Blake2_128Concat, T::AccountId, ()>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        // When a new account is added to the allow-list.
        AccountAllowed {
            account: T::AccountId,
            voted_for: Vec<T::AccountId>,
        },
        AccountVoted {
            referrer: T::AccountId,
            referee: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        AlreadyAllowed,
        DuplicateVote,
        NotAllowedToVote,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub allowed_accounts: Vec<(T::AccountId, ())>,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                allowed_accounts: Vec::new(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            Pallet::<T>::initialize_allowed_accounts(&self.allowed_accounts);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Add a new account to the allow-list.
        /// Can only be called by the defined origin.
        #[pallet::weight(0)]
        #[pallet::call_index(0)]
        pub fn vote_for_account(
            origin: OriginFor<T>,
            new_account: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let account = ensure_signed(origin)?;
            ensure!(
                <AllowedAccounts<T>>::contains_key(&account),
                Error::<T>::NotAllowedToVote
            );
            ensure!(
                !<AllowedAccounts<T>>::contains_key(&new_account),
                Error::<T>::AlreadyAllowed
            );
            ensure!(
                !<Votes<T>>::contains_key(&new_account, &account),
                Error::<T>::DuplicateVote
            );

            // Check if the new account has enough votes to be added to the allow-list.
            let votes_for = Votes::<T>::iter_prefix(&new_account).count() + 1;
            let votes_required = AllowedAccounts::<T>::iter().count();
            let percent = Percent::from_rational(votes_for, votes_required);

            if percent >= T::VotesToAllow::get() {
                // Enough votes to add the new account to the allow-list.
                <AllowedAccounts<T>>::insert(&new_account, ());
                let voted_for = <Votes<T>>::drain_prefix(&new_account)
                    .map(|(k, _)| k)
                    .chain(sp_std::iter::once(account.clone()))
                    .collect();

                Self::deposit_event(Event::AccountVoted {
                    referrer: account,
                    referee: new_account.clone(),
                });
                Self::deposit_event(Event::AccountAllowed {
                    account: new_account,
                    voted_for,
                });
            } else {
                // Vote for the new account.
                <Votes<T>>::insert(&new_account, &account, ());
                Self::deposit_event(Event::AccountVoted {
                    referrer: account,
                    referee: new_account,
                });
            }

            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        fn initialize_allowed_accounts(allowed_accounts: &[(T::AccountId, ())]) {
            if !allowed_accounts.is_empty() {
                for (account, extrinsics) in allowed_accounts.iter() {
                    <AllowedAccounts<T>>::insert(account, extrinsics);
                }
            }
        }
    }
    /// The following section implements the `SignedExtension` trait
    /// for the `AllowAccount` type.
    /// `SignedExtension` is being used here to filter out the not allowed accounts
    /// when they try to send filtered extrinsics to the runtime.
    /// Inside the `validate` function of the `SignedExtension` trait,
    /// we check for filtered extrinsics and if the sender (origin) of the extrinsic is part of the
    /// allow-list or not.
    /// The extrinsic will be rejected as invalid if the origin is not part
    /// of the allow-list.
    /// The validation happens at the transaction queue level,
    ///  and the extrinsics are filtered out before they hit the pallet logic.

    /// The `AllowAccount` struct.
    #[derive(Encode, Decode, Clone, Eq, PartialEq, Default, TypeInfo)]
    pub struct AllowAccount<T: pallet::Config + Send + Sync>(PhantomData<T>);

    impl<T: pallet::Config + Send + Sync> AllowAccount<T> {
        /// utility constructor. Used only in client/factory code.
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }

    /// Debug impl for the `AllowAccount` struct.
    impl<T: pallet::Config + Send + Sync> Debug for AllowAccount<T> {
        #[cfg(feature = "std")]
        fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
            write!(f, "AllowAccount")
        }

        #[cfg(not(feature = "std"))]
        fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
            Ok(())
        }
    }

    /// Implementation of the `SignedExtension` trait for the `AllowAccount` struct.
    impl<T: pallet::Config + Sync + Send + TypeInfo> SignedExtension for AllowAccount<T>
    where
        T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
    {
        type AccountId = T::AccountId;
        type Call = T::RuntimeCall;
        type AdditionalSigned = ();
        type Pre = ();
        const IDENTIFIER: &'static str = "AllowAccount";

        fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
            Ok(())
        }

        // Filter out the not allowed keys for predefined calls.
        // If the key is in the allow-list, return a valid transaction,
        // else return a custom error.
        fn validate(
            &self,
            who: &Self::AccountId,
            call: &Self::Call,
            info: &DispatchInfoOf<Self::Call>,
            _len: usize,
        ) -> TransactionValidity {
            if T::CallsToFilter::matches(call) && !<AllowedAccounts<T>>::contains_key(who) {
                Err(InvalidTransaction::BadSigner.into())
            } else {
                Ok(ValidTransaction {
                    priority: info.weight.ref_time(),
                    longevity: TransactionLongevity::max_value(),
                    propagate: true,
                    ..Default::default()
                })
            }
        }

        fn pre_dispatch(
            self,
            who: &Self::AccountId,
            call: &Self::Call,
            info: &DispatchInfoOf<Self::Call>,
            len: usize,
        ) -> Result<Self::Pre, TransactionValidityError> {
            self.validate(who, call, info, len)
                .map(|_| ())
                .map_err(Into::into)
        }
    }
}
