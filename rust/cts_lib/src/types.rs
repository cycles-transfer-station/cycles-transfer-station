use crate::{
    ic_cdk::{
        api::{
            time,
            call::{
                RejectionCode
            },
        },
        export::{
            Principal,
            candid::{
                self,
                CandidType,
                Deserialize,   
            }
        }
    }
};




pub type Cycles = u128;
pub type CyclesTransferRefund = Cycles;
pub type CTSFuel = Cycles;
pub type UserId = Principal;
pub type UserCanisterId = Principal;
pub type UsersMapCanisterId = Principal;
pub type XdrPerMyriadPerIcp = u64;







#[derive(CandidType, Deserialize, Clone, serde::Serialize)]
pub enum CyclesTransferMemo {
    Nat(u128),
    Text(String),
    Blob(Vec<u8>)   // with serde bytes
}

#[derive(CandidType, Deserialize, Clone, serde::Serialize)]
pub struct CyclesTransfer {
    pub memo: CyclesTransferMemo
}






pub mod canister_code {
    use super::{candid, CandidType, Deserialize};
    
    #[derive(CandidType, Deserialize, Clone)]
    pub struct CanisterCode {
        #[serde(with = "serde_bytes")]
        module: Vec<u8>,
        module_hash: [u8; 32]
    }

    impl CanisterCode {
        pub fn new(mut module: Vec<u8>) -> Self { // :mut for the shrink_to_fit
            module.shrink_to_fit();
            Self {
                module_hash: crate::tools::sha256(&module), // put this on the top if move error
                module: module,
            }
        }
        pub fn module(&self) -> &Vec<u8> {
            &self.module
        }
        pub fn module_hash(&self) -> &[u8; 32] {
            &self.module_hash
        }
        pub fn change_module(&mut self, module: Vec<u8>) {
            *self = Self::new(module);
        }
    }
}






pub mod user_canister_cache {
    use super::{UserId, UserCanisterId, time};
    use std::collections::{HashMap};
    
    // private
    #[derive(Clone, Copy)]
    struct UserCacheData {
        timestamp_nanos: u64,
        opt_user_canister_id: Option<UserCanisterId>
    }

    // cacha for this. with a max users->user-canisters
    // on a new user, put/update insert the new user into this cache
    // on a user-contract-termination, void[remove/delete] the (user,user-canister)-log in this cache
        
    pub struct UserCanisterCache {
        hashmap: HashMap<UserId, UserCacheData>,
        max_size: usize
    }
    impl UserCanisterCache {
        
        pub fn new(max_size: usize) -> Self {
            Self {
                hashmap: HashMap::new(),
                max_size
            }
        }
        
        pub fn put(&mut self, user_id: UserId, opt_user_canister_id: Option<UserCanisterId>) {
            if self.hashmap.len() >= self.max_size {
                self.hashmap.remove(
                    &(self.hashmap.iter().min_by_key(
                        |(_user_id, user_cache_data)| {
                            user_cache_data.timestamp_nanos
                        }
                    ).unwrap().0.clone())
                );
            }
            self.hashmap.insert(user_id, UserCacheData{ opt_user_canister_id, timestamp_nanos: time() });
        }
        
        pub fn check(&mut self, user_id: UserId) -> Option<Option<UserCanisterId>> {
            match self.hashmap.get_mut(&user_id) {
                None => None,
                Some(user_cache_data) => {
                    user_cache_data.timestamp_nanos = time();
                    Some(user_cache_data.opt_user_canister_id)
                }
            }
        }
    }

}





pub mod management_canister {
    use super::*;
    
    #[derive(CandidType, Deserialize)]
    pub struct ManagementCanisterInstallCodeQuest<'a> {
        pub mode : ManagementCanisterInstallCodeMode,
        pub canister_id : Principal,
        #[serde(with = "serde_bytes")]
        pub wasm_module : &'a [u8],
        #[serde(with = "serde_bytes")]
        pub arg : &'a [u8],
    }

    #[allow(non_camel_case_types)]
    #[derive(CandidType, Deserialize)]
    pub enum ManagementCanisterInstallCodeMode {
        install, 
        reinstall, 
        upgrade
    }
    
    #[derive(CandidType, Deserialize)]
    pub struct ManagementCanisterCreateCanisterQuest {
        pub settings : Option<ManagementCanisterOptionalCanisterSettings>
    }

    #[derive(CandidType, Deserialize, Clone)]
    pub struct ManagementCanisterOptionalCanisterSettings {
        pub controllers : Option<Vec<Principal>>,
        pub compute_allocation : Option<u128>,
        pub memory_allocation : Option<u128>,
        pub freezing_threshold : Option<u128>,
    }

    #[derive(CandidType, Deserialize, Clone, PartialEq, Eq)]
    pub struct ManagementCanisterCanisterSettings {
        pub controllers : Vec<Principal>,
        pub compute_allocation : u128,
        pub memory_allocation : u128,
        pub freezing_threshold : u128
    }

    #[derive(CandidType, Deserialize, Clone)]
    pub struct ManagementCanisterCanisterStatusRecord {
        pub status : ManagementCanisterCanisterStatusVariant,
        pub settings: ManagementCanisterCanisterSettings,
        pub module_hash: Option<[u8; 32]>,
        pub memory_size: u128,
        pub cycles: u128
    }

    #[allow(non_camel_case_types)]
    #[derive(CandidType, Deserialize, PartialEq, Eq, Clone)]
    pub enum ManagementCanisterCanisterStatusVariant {
        running,
        stopping,
        stopped,
    }

    #[derive(CandidType, Deserialize)]
    pub struct CanisterIdRecord {
        pub canister_id : Principal
    }

    #[derive(CandidType, Deserialize)]
    pub struct ChangeCanisterSettingsRecord {
        pub canister_id : Principal,
        pub settings : ManagementCanisterOptionalCanisterSettings
    }


}




pub mod cycles_transferrer {
    use super::{Principal, CyclesTransferMemo, Cycles, candid, CandidType, Deserialize};
    
    #[derive(CandidType, Deserialize)]
    pub struct CyclesTransferrerCanisterInit {
        pub cts_id: Principal
    }
    
    #[derive(CandidType, Deserialize)]
    pub struct CyclesTransfer {
        pub memo: CyclesTransferMemo,
        pub original_caller: Option<Principal>
    }
    
    #[derive(CandidType, Deserialize)]    
    pub struct TransferCyclesQuest{
        pub user_cycles_transfer_id: u128,
        pub for_the_canister: Principal,
        pub cycles: Cycles,
        pub cycles_transfer_memo: CyclesTransferMemo
    }
    
    #[derive(CandidType, Deserialize)]
    pub enum TransferCyclesError {
        MsgCyclesTooLow{ transfer_cycles_fee: Cycles },
        MaxOngoingCyclesTransfers,
        CyclesTransferQuestCandidCodeError(String)
    }
    
    #[derive(CandidType, Deserialize, Clone)]
    pub struct TransferCyclesCallbackQuest {
        pub user_cycles_transfer_id: u128,
        pub opt_cycles_transfer_call_error: Option<(u32/*reject_code*/, String/*reject_message*/)> // None means callstatus == 'replied'
    }
    
}




pub mod cts {
    use super::*;
    
    #[derive(CandidType, Deserialize)]
    pub struct UserCanisterLifetimeTerminationQuest {
        pub user_id: UserId,
        pub user_cycles_balance: Cycles
    }
    
}







pub mod users_map_canister {
    use super::*;

    #[derive(CandidType, Deserialize)]
    pub struct UsersMapCanisterInit {
        pub cts_id: Principal
    }

    #[derive(CandidType, Deserialize, Clone)]    
    pub struct UMCUserData {
        pub user_canister_id: UserCanisterId,
        pub user_canister_latest_known_module_hash: [u8; 32],
    }

    #[derive(CandidType,Deserialize)]
    pub enum PutNewUserError {
        CanisterIsFull,
        FoundUser(UMCUserData)
    }

    
    pub type UMCUpgradeUCError = (UserCanisterId, UMCUpgradeUCCallErrorType, (u32, String));

    #[derive(CandidType, Deserialize, Clone, Debug)]
    pub enum UMCUpgradeUCCallErrorType {
        StopCanisterCallError,
        UpgradeCodeCallError{wasm_module_hash: [u8; 32]},
        StartCanisterCallError
    }
    




}






pub mod user_canister {
    use super::*;

    #[derive(CandidType, Deserialize)]
    pub struct UserCanisterInit {
        pub user_id: UserId,
        pub cts_id: Principal,
        pub cycles_market_id: Principal, 
        pub user_canister_storage_size_mib: u64,                         
        pub user_canister_lifetime_termination_timestamp_seconds: u64,
        pub cycles_transferrer_canisters: Vec<Principal>
    }
    
    #[derive(CandidType, Deserialize, Clone)]
    pub struct UserTransferCyclesQuest {
        pub for_the_canister: Principal,
        pub cycles: Cycles,
        pub cycles_transfer_memo: CyclesTransferMemo
    }
    
}


pub mod cycles_market {
    use super::{CandidType, Deserialize, Cycles, XdrPerMyriadPerIcp};
    use crate::ic_ledger_types::{IcpTokens, IcpBlockHeight, IcpTransferError, IcpId};
    
    pub type PositionId = u128;
    pub type PurchaseId = u128;
    
    #[derive(CandidType, Deserialize)]
    pub struct CreateCyclesPositionQuest {
        pub cycles: Cycles,
        pub minimum_purchase: Cycles,
        pub xdr_permyriad_per_icp_rate: XdrPerMyriadPerIcp,
        
    }

    #[derive(CandidType, Deserialize)]
    pub enum CreateCyclesPositionError{
        MinimumPurchaseMustBeEqualOrLessThanTheCyclesPosition,
        MsgCyclesTooLow{ create_position_fee: Cycles },
        CyclesMarketIsBusy,
        CyclesMarketIsFull,
        CyclesMarketIsFull_MinimumRateAndMinimumCyclesPositionForABump{ minimum_rate_for_a_bump: XdrPerMyriadPerIcp, minimum_cycles_position_for_a_bump: Cycles },
        MinimumCyclesPosition(Cycles)   
    }

    #[derive(CandidType, Deserialize)]
    pub struct CreateCyclesPositionSuccess {
        pub position_id: PositionId,
    }

    #[derive(CandidType, Deserialize)]
    pub struct CreateIcpPositionQuest {
        pub icp: IcpTokens,
        pub minimum_purchase: IcpTokens,
        pub xdr_permyriad_per_icp_rate: XdrPerMyriadPerIcp,
    }

    #[derive(CandidType, Deserialize)]
    pub enum CreateIcpPositionError {
        MinimumPurchaseMustBeEqualOrLessThanTheIcpPosition,
        MsgCyclesTooLow{ create_position_fee: Cycles },
        CyclesMarketIsFull,
        CallerIsInTheMiddleOfACreateIcpPositionOrPurchaseCyclesPositionOrTransferIcpBalanceCall,
        CheckUserCyclesMarketIcpLedgerBalanceError((u32, String)),
        UserIcpBalanceTooLow{ user_icp_balance: IcpTokens },
        CyclesMarketIsFull_MaximumRateAndMinimumIcpPositionForABump{ maximum_rate_for_a_bump: XdrPerMyriadPerIcp, minimum_icp_position_for_a_bump: IcpTokens },
        MinimumIcpPosition(IcpTokens),
    }

    #[derive(CandidType, Deserialize)]
    pub struct CreateIcpPositionSuccess {
        pub position_id: PositionId
    }

    #[derive(CandidType, Deserialize)]
    pub struct PurchaseCyclesPositionQuest {
        pub cycles_position_id: PositionId,
        pub cycles: Cycles
    }

    #[derive(CandidType, Deserialize)]
    pub enum PurchaseCyclesPositionError {
        MsgCyclesTooLow{ purchase_position_fee: Cycles },
        CyclesMarketIsBusy,
        CallerIsInTheMiddleOfACreateIcpPositionOrPurchaseCyclesPositionOrTransferIcpBalanceCall,
        CheckUserCyclesMarketIcpLedgerBalanceError((u32, String)),
        UserIcpBalanceTooLow{ user_icp_balance: IcpTokens },
        CyclesPositionNotFound,
        CyclesPositionCyclesIsLessThanThePurchaseQuest{ cycles_position_cycles: Cycles },
        CyclesPositionMinimumPurchaseIsGreaterThanThePurchaseQuest{ cycles_position_minimum_purchase: Cycles },
    }

    #[derive(CandidType, Deserialize)]
    pub struct PurchaseCyclesPositionSuccess {
        pub purchase_id: PurchaseId,
    }

    pub type PurchaseCyclesPositionResult = Result<PurchaseCyclesPositionSuccess, PurchaseCyclesPositionError>;

    #[derive(CandidType, Deserialize)]
    pub struct PurchaseIcpPositionQuest {
        pub icp_position_id: PositionId,
        pub icp: IcpTokens
    }

    #[derive(CandidType, Deserialize)]
    pub enum PurchaseIcpPositionError {
        MsgCyclesTooLow{ purchase_position_fee: Cycles },
        CyclesMarketIsBusy,
        IcpPositionNotFound,
        IcpPositionIcpIsLessThanThePurchaseQuest{ icp_position_icp: IcpTokens },
        IcpPositionMinimumPurchaseIsGreaterThanThePurchaseQuest{ icp_position_minimum_purchase: IcpTokens }
    }

    #[derive(CandidType, Deserialize)]
    pub struct PurchaseIcpPositionSuccess {
        pub purchase_id: PurchaseId
    }

    pub type PurchaseIcpPositionResult = Result<PurchaseIcpPositionSuccess, PurchaseIcpPositionError>;

    #[derive(CandidType, Deserialize)]
    pub struct VoidPositionQuest {
        pub position_id: PositionId
    }

    #[derive(CandidType, Deserialize)]
    pub enum VoidPositionError {
        WrongCaller,
        CyclesMarketIsBusy,
        PositionNotFound,
    }

    pub type VoidPositionResult = Result<(), VoidPositionError>;

    #[derive(CandidType, Deserialize)]
    pub struct TransferIcpBalanceQuest {
        pub icp: IcpTokens,
        pub icp_fee: Option<IcpTokens>,
        pub to: IcpId
    }

    #[derive(CandidType, Deserialize)]
    pub enum TransferIcpBalanceError {
        MsgCyclesTooLow{ transfer_icp_balance_fee: Cycles },
        CyclesMarketIsBusy,
        CallerIsInTheMiddleOfACreateIcpPositionOrPurchaseCyclesPositionOrTransferIcpBalanceCall,
        CheckUserCyclesMarketIcpLedgerBalanceCallError((u32, String)),
        UserIcpBalanceTooLow{ user_icp_balance: IcpTokens },
        IcpTransferCallError((u32, String)),
        IcpTransferError(IcpTransferError)
    }

    pub type TransferIcpBalanceResult = Result<IcpBlockHeight, TransferIcpBalanceError>;

}






