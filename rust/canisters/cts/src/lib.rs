
// lock each user from making other calls on each async call that awaits, like the collect_balance call, lock the user at the begining and unlock the user at the end. or better can take the funds within the first [exe]cution and if want can give back
// will callbacks (the code after an await) get dropped if the subnet is under heavy load?
// when calling canisters that i dont know if they can possible give-back unexpected candid, use call_raw and dont panic on the candid-decode, return an error.
// dont want to implement From<(RejectionCode, String)> for the return errors in the calls async that call other canisters because if the function makes more than one call then the ? with the from can give-back a wrong error type 
// always check user lock before any awaits (or maybe after the first await if not fective?). 
// in the cycles-market, let a seller set a minimum-purchase-quantity. which can be the full-mount that is up for the sale or less 
// always unlock the user af-ter the last await-call()
// does dereferencing a borrow give the ownership? try on a non-copy type. when using it for an 'expression' then yes. error[E0507]: cannot move out of `*cycles_transfer_purchase_log` which is behind a mutable reference
// sending cycles to a canister is the same risk as sending icp to a canister. 
// put a max_fee on a cycles-transfer-purchase & on a cycles-bank-purchase?
// 5xdr first-time-user-fee, valid for one year. with 100mbs of storage for the year and standard-call-rate-limits. after the year, if the user doesn't pay for more space, the user-storage gets deleted and the user cycles balance and icp balance stays for another 3 years.
// 0.1 GiB / 102.4 Mib / 107374182.4 bytes user-storage for the 1 year for the 5xdr. 

// I think using the cycles_transferrer canister is a good way to do it.

// choice for users to download a signed data canister-signature of past trassactions. 
// choice for users to delete past transactions to re-claim storage-space  
// if a user requested cycles-transfer call takes-more than half an hour to come back, the user is not refunded any cycles the callee does'nt take

// user does the main operations through the user_canister.
// the user-lock is on the user-canister

// each method is a contract


// icp transfers , 0.10-xdr / 14-cents flat fee


// tegrate with the icscan.io


// 10 years save the user's-cycles-balance and icp-balance if the user-canister finishes.  

// convert icp for the cycles as a service and send to a canister with the cycles_transfer-specification . for the users with a cts-user-contract.
// when taking icp as a payment for a service, take the icp fee first , then do the service


// the CTS can take (only) the CTS-governance-token for the payments for the cts-user-contracts. and burn them.

// MANAGE-MEMBERSHIP page in the frontcode



//#![allow(unused)] 
#![allow(non_camel_case_types)]

use std::{
    cell::{Cell, RefCell, RefMut}, 
    collections::{HashMap, VecDeque},
    future::Future,
    
};

use cts_lib::{
    types::{
        Cycles,
        CTSFuel,
        CyclesTransfer,
        CyclesTransferMemo,
        UserId,
        UserCanisterId,
        UsersMapCanisterId,
        canister_code::CanisterCode,
        management_canister::{
            ManagementCanisterInstallCodeMode,
            ManagementCanisterInstallCodeQuest,
            ManagementCanisterCanisterSettings,
            ManagementCanisterOptionalCanisterSettings,
            ManagementCanisterCanisterStatusRecord,
            ManagementCanisterCanisterStatusVariant,
            CanisterIdRecord,
            ChangeCanisterSettingsRecord,
            
        },
        cts::{
        
        },
        users_map_canister::{
            UMCUserData,
            UMCUpgradeUCError,
            UMCUpgradeUCCallErrorType
        },
        user_canister::{
            UserCanisterInit,
        },
        cycles_transferrer::{
            CyclesTransferrerCanisterInit,
            CyclesTransferRefund,
        },
    },
    consts::{
        MANAGEMENT_CANISTER_ID,
        WASM_PAGE_SIZE_BYTES,
        NETWORK_CANISTER_CREATION_FEE_CYCLES,
        NETWORK_GiB_STORAGE_PER_SECOND_FEE_CYCLES
    },
    tools::{
        sha256,
        localkey::{
            self,
            refcell::{
                with, 
                with_mut,
            },
            cell::{
                get,
                set
            }
        },
        thirty_bytes_as_principal,
        principal_icp_subaccount,
    },
    ic_cdk::{
        self,
        api::{
            trap,
            caller, 
            time,
            id,
            canister_balance128,
            performance_counter,
            call::{
                arg_data,
                arg_data_raw,
                arg_data_raw_size,
                call_raw128,
                call,
                call_with_payment128,
                CallResult,
                RejectionCode,
                msg_cycles_refunded128,
                msg_cycles_available128,
                msg_cycles_accept128,
                reject,
                reply,
                reply_raw
            },
            stable::{
                stable64_grow,
                stable64_read,
                stable64_size,
                stable64_write,
                stable_bytes
            }
        },
        export::{
            Principal,
            candid::{
                self,
                CandidType,
                Deserialize,
                utils::{
                    encode_one, 
                    decode_one
                },
            },
        },
    },
    ic_cdk_macros::{
        update, 
        query, 
        init, 
        pre_upgrade, 
        post_upgrade
    },
    ic_ledger_types::{
        IcpMemo,
        IcpId,
        IcpIdSub,
        IcpTokens,
        IcpBlockHeight,
        IcpTimestamp,
        ICP_DEFAULT_SUBACCOUNT,
        ICP_LEDGER_TRANSFER_DEFAULT_FEE,
        MAINNET_CYCLES_MINTING_CANISTER_ID,
        MAINNET_LEDGER_CANISTER_ID, 
        icp_transfer,
        IcpTransferArgs, 
        IcpTransferResult, 
        IcpTransferError,
        icp_account_balance,
        IcpAccountBalanceArgs
    },
    global_allocator_counter::get_allocated_bytes_count
};


#[cfg(test)]
mod t;

mod tools;
use tools::{
    user_cycles_balance_topup_memo_bytes,
    check_user_icp_ledger_balance,
    main_cts_icp_id,
    CheckCurrentXdrPerMyriadPerIcpCmcRateError,
    CheckCurrentXdrPerMyriadPerIcpCmcRateSponse,
    check_current_xdr_permyriad_per_icp_cmc_rate,
    // icptokens_to_cycles,
    cycles_to_icptokens,
    get_new_canister,
    GetNewCanisterError,
    IcpXdrConversionRate,
    take_user_icp_ledger,
    ICP_LEDGER_CREATE_CANISTER_MEMO,
    CmcNotifyError,
    CmcNotifyCreateCanisterQuest,
    PutNewUserIntoAUsersMapCanisterError,
    put_new_user_into_a_users_map_canister,
    FindUserInTheUsersMapCanistersError,
    find_user_in_the_users_map_canisters,
    put_new_canister,
    LedgerTopupCyclesCmcIcpTransferError,
    LedgerTopupCyclesCmcNotifyError,

    
};

mod frontcode;
use frontcode::{File, Files, FilesHashes, HttpRequest, HttpResponse, set_root_hash, make_file_certificate_header};



#[derive(CandidType, Deserialize)]
struct CTSData {
    controllers: Vec<Principal>,
    user_canister_code: CanisterCode,
    users_map_canister_code: CanisterCode,
    cycles_transferrer_canister_code: CanisterCode,
    frontcode_files: Files,
    frontcode_files_hashes: Vec<(String, [u8; 32])>, // field is [only] use for the upgrades.
    users_map_canisters: Vec<Principal>,
    create_new_users_map_canister_lock: bool,
    cycles_transferrer_canisters: Vec<Principal>,
    cycles_transferrer_canisters_round_robin_counter: u32,
    canisters_for_the_use: HashSet<Principal>,
    new_users: HashMap<Principal, NewUserData>,
    users_burn_icp_mint_cycles: HashMap<UserId, UserBurnIcpMintCyclesData>

}
impl CTSData {
    fn new() -> Self {
        Self {
            controllers: Vec::new(),
            user_canister_code: CanisterCode::new(Vec::new()),
            users_map_canister_code: CanisterCode::new(Vec::new()),
            cycles_transferrer_canister_code: CanisterCode::new(Vec::new()),
            frontcode_files: Files::new(),
            frontcode_files_hashes: Vec::new(), // field is [only] use for the upgrades.
            users_map_canisters: Vec::new(),
            create_new_users_map_canister_lock: false,
            cycles_transferrer_canisters: Vec::new(),
            cycles_transferrer_canisters_round_robin_counter: 0,
            canisters_for_the_use: HashSet::new(),
            new_users: HashMap::new(),
            users_burn_icp_mint_cycles: HashMap::new()
        }
    }
}


    

pub const NEW_USER_CONTRACT_COST_CYCLES: Cycles = 10_000_000_000_000; //10T-cycles for a new-user-contract. lifetime: 1-year, storage-size: 50mib/*100mib-canister-memory-allocation*/, start-with-the-ctsfuel: 5T-cycles. 
pub const NEW_USER_CONTRACT_LIFETIME_DURATION_SECONDS: u64 = 1*60*60*24*365; // 1-year.
pub const NEW_USER_CONTRACT_CTSFUEL: CTSFuel = 5_000_000_000_000; // 5T-cycles.
pub const NEW_USER_CONTRACT_STORAGE_SIZE_MiB: u64 = 50; // 50-mib
pub const NEW_USER_CANISTER_NETWORK_MEMORY_ALLOCATION_MiB: u64 = NEW_USER_CONTRACT_STORAGE_SIZE_MiB * 2;
pub const NEW_USER_CANISTER_BACKUP_CYCLES: Cycles = 1_400_000_000_000;
pub const NEW_USER_CANISTER_CREATION_CYCLES: Cycles = {
    NETWORK_CANISTER_CREATION_FEE_CYCLES
    + (
        NEW_USER_CONTRACT_LIFETIME_DURATION_SECONDS as u128 
        * NEW_USER_CANISTER_NETWORK_MEMORY_ALLOCATION_MiB as u128 
        * NETWORK_GiB_STORAGE_PER_SECOND_FEE_CYCLES as u128 
        / 1024 /*network mib storage per second*/ )
    + NEW_USER_CONTRACT_CTSFUEL
    + NEW_USER_CANISTER_BACKUP_CYCLES
};

pub const MAX_NEW_USERS: usize = 5000; // the max number of entries in the NEW_USERS-hashmap at the same-time
pub const MAX_USERS_MAP_CANISTERS: usize = 4; // can be 30-million at 1-gb, or 3-million at 0.1-gb,

pub const CTS_ICP_TRANSFER_FEE: IcpTokens = IcpTokens::from_e8s(30000);// calculate through the xdr conversion rate ? // 100_000_000_000-cycles

const MAX_USERS_BURN_ICP_MINT_CYCLES: usize = 1000;
const MINIMUM_USER_BURN_ICP_MINT_CYCLES: IcpTokens = IcpTokens::from_e8s(3000000); // 0.03 icp
const USER_BURN_ICP_MINT_CYCLES_FEE: Cycles = 50_000_000_000; //  user gets cmc-cycles minus this fee


pub const MINIMUM_CTS_CYCLES_TRANSFER_IN_CYCLES: Cycles = 5_000_000_000;


const STABLE_MEMORY_HEADER_SIZE_BYTES: u64 = 1024;


thread_local! {
    
    static CTS_DATA: RefCell<CTSData> = RefCell::new(CTSData::new());
    
    // not save through upgrades
    pub static FRONTCODE_FILES_HASHES: RefCell<FilesHashes> = RefCell::new(FilesHashes::new()); // is with the save through the upgrades by the frontcode_files_hashes field on the CTSData
    pub static LATEST_KNOWN_CMC_RATE: Cell<IcpXdrConversionRate> = Cell::new(IcpXdrConversionRate{ xdr_permyriad_per_icp: 0, timestamp_seconds: 0 });
    static     USER_CANISTER_CACHE: RefCell<UserCanisterCache> = RefCell::new(UserCanisterCache::new());
    static     STOP_CALLS: Cell<bool> = Cell::new(false);
    static     STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    
}



// -------------------------------------------------------------


#[derive(CandidType, Deserialize)]
struct CTSInit {
    controllers: Vec<Principal>
} 

#[init]
fn init(cts_init: CTSInit) {
    with_mut(&CTS_DATA, |cts_data| { cts_data.controllers = cts_init.controllers; });
} 


// -------------------------------------------------------------


fn create_cts_data_candid_bytes() -> Vec<u8> {
    
    with_mut(&CTS_DATA, |cts_data| {
        cts_data.frontcode_files_hashes = with(&FRONTCODE_FILES_HASHES, |frontcode_files_hashes| { 
            frontcode_files_hashes.iter().map(
                |ferences| { ferences.clone() }
            ).collect::<Vec<(String, [u8; 32])>>() 
        });
    });

    let mut cts_data_candid_bytes: Vec<u8> = with(&CTS_DATA, |cts_data| { encode_one(cts_data).unwrap() });
    cts_data_candid_bytes.shrink_to_fit();
    cts_data_candid_bytes
}

fn re_store_cts_data_candid_bytes(cts_data_candid_bytes: Vec<u8>) {
    
    let mut cts_data: CTSData = match decode_one::<CTSData>(&cts_data_candid_bytes) {
        Ok(cts_data) => cts_data,
        Err(_) => {
            trap("error decode of the CTSData");
            /*
            let old_cts_data: OldCTSData = decode_one::<CTSData>(&cts_data_candid_bytes).unwrap();
            let cts_data: CTSData = CTSData{
                controllers: old_cts_data.controllers
                ........
            };
            cts_data
            */
        }
    };

    std::mem::drop(cts_data_candid_bytes);
    
    with_mut(&FRONTCODE_FILES_HASHES, |frontcode_files_hashes| {
        *frontcode_files_hashes = FilesHashes::from_iter(cts_data.frontcode_files_hashes.drain(..));
        set_root_hash(frontcode_files_hashes);
    });
    
    with_mut(&CTS_DATA, |ctsd| {
        *ctsd = cts_data;    
    });
    
}


#[pre_upgrade]
fn pre_upgrade() {
    
    let cts_upgrade_data_candid_bytes: Vec<u8> = create_cts_data_candid_bytes();
    
    let current_stable_size_wasm_pages: u64 = stable64_size();
    let current_stable_size_bytes: u64 = current_stable_size_wasm_pages * WASM_PAGE_SIZE_BYTES as u64;
    
    let want_stable_memory_size_bytes: u64 = STABLE_MEMORY_HEADER_SIZE_BYTES + 8/*len of the cts_upgrade_data_candid_bytes*/ + cts_upgrade_data_candid_bytes.len() as u64; 
    if current_stable_size_bytes < want_stable_memory_size_bytes {
        stable64_grow(((want_stable_memory_size_bytes - current_stable_size_bytes) / WASM_PAGE_SIZE_BYTES as u64) + 1).unwrap();
    }
    
    stable64_write(STABLE_MEMORY_HEADER_SIZE_BYTES, &((cts_upgrade_data_candid_bytes.len() as u64).to_be_bytes()));
    stable64_write(STABLE_MEMORY_HEADER_SIZE_BYTES + 8, &cts_upgrade_data_candid_bytes);
    
}

#[post_upgrade]
fn post_upgrade() {
    let mut cts_upgrade_data_candid_bytes_len_u64_be_bytes: [u8; 8] = [0; 8];
    stable64_read(STABLE_MEMORY_HEADER_SIZE_BYTES, &mut cts_upgrade_data_candid_bytes_len_u64_be_bytes);
    let cts_upgrade_data_candid_bytes_len_u64: u64 = u64::from_be_bytes(cts_upgrade_data_candid_bytes_len_u64_be_bytes); 
    
    let mut cts_upgrade_data_candid_bytes: Vec<u8> = vec![0; cts_upgrade_data_candid_bytes_len_u64 as usize]; // usize is u32 on wasm32 so careful with the cast len_u64 as usize 
    stable64_read(STABLE_MEMORY_HEADER_SIZE_BYTES + 8, &mut cts_upgrade_data_candid_bytes);
    
    re_store_cts_data_candid_bytes(cts_upgrade_data_candid_bytes);
    
} 


// test this!
#[no_mangle]
pub fn canister_inspect_message() {
    // caution: this function is only called for ingress messages 
    use ic_cdk::api::call::{method_name,accept_message};
    
    if caller() == Principal::anonymous() 
        && !["see_fees"].contains(&&method_name()[..])
        {
        trap("caller cannot be anonymous for this method.");
    }
    
    // check the size of the arg_data_raw_size()

    if &method_name()[..] == "cycles_transfer" {
        trap("caller must be a canister for this method.");
    }
    
    if method_name()[..].starts_with("controller") {
        if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
            trap("Caller must be a controller for this method.");
        }
    }

    accept_message();
}







// ----------------------------------------------------------------------------------------








#[export_name = "canister_update cycles_transfer"]
pub fn cycles_transfer() {
    if localkey::cell::get(&STOP_CALLS) { trap("Maintenance. try again soon."); }
    
    if arg_data_raw_size() > 100 {
        reject("arg_data_raw_size must be <= 100");
        return;
    }

    if msg_cycles_available128() < MINIMUM_CTS_CYCLES_TRANSFER_IN_CYCLES {
        reject(&format!("minimum cycles: {}", MINIMUM_CTS_CYCLES_TRANSFER_IN_CYCLES));
        return;
    }

    let (ct,): (CyclesTransfer,) = arg_data::<(CyclesTransfer,)>();
    
    match ct.memo {
        CyclesTransferMemo::Blob(b) => {
            if b == b"DONATION" {
                msg_cycles_accept128(msg_cycles_available128());
            } else {
                reject("unknown CyclesTransferMemo");
                return;
            }
        },
        _ => {
            reject("unknown CyclesTransferMemo");
            return;
        }
    }
            
}










#[derive(CandidType, Deserialize)]
pub struct Fees {
    cts_user_contract_cost_cycles: Cycles,
    cts_icp_transfer_fee: IcpTokens,
    user_burn_icp_mint_cycles_fee: Cycles
    
    
    
}

#[query]
pub fn see_fees() -> Fees {
    Fees {
        cts_user_contract_cost_cycles: NEW_USER_CONTRACT_COST_CYCLES,
        cts_icp_transfer_fee: CTS_ICP_TRANSFER_FEE,
        user_burn_icp_mint_cycles_fee: USER_BURN_ICP_MINT_CYCLES_FEE
        
        
    }
}











// save the fees in the new_user_data so the fees cant change while creating a new user

#[derive(Clone, CandidType, Deserialize)]
struct NewUserData {
    start_time_nanos: u64,
    lock: bool,    
    current_xdr_icp_rate: u64,
    new_user_quest: NewUserQuest,
    // the options and bools are for the memberance of the steps
    look_if_user_is_in_the_users_map_canisters: bool,
    referral_user_canister_id: Option<UserCanisterId>, // use if a referral
    create_user_canister_block_height: Option<IcpBlockHeight>,
    user_canister: Option<Principal>,
    users_map_canister: Option<UsersMapCanisterId>,
    user_canister_uninstall_code: bool,
    user_canister_install_code: bool,
    user_canister_status_record: Option<ManagementCanisterCanisterStatusRecord>,
    collect_icp: bool,
    collect_cycles_cmc_icp_transfer_block_height: Option<IcpBlockHeight>,
    collect_cycles_cmc_notify_cycles: Option<Cycles>,
    referral_user_referral_payment_cycles_transfer: bool,
    user_referral_payment_cycles_transfer: bool
    
}



#[derive(CandidType, Deserialize)]
pub enum NewUserMidCallError{
    UsersMapCanistersFindUserCallFails(Vec<(UsersMapCanisterId, (u32, String))>),
    PutNewUserIntoAUsersMapCanisterError(PutNewUserIntoAUsersMapCanisterError),
    CreateUserCanisterIcpTransferError(IcpTransferError),
    CreateUserCanisterIcpTransferCallError((u32, String)),
    CreateUserCanisterCmcNotifyError(CmcNotifyError),
    CreateUserCanisterCmcNotifyCallError((u32, String)),
    UserCanisterUninstallCodeCallError((u32, String)),
    UserCanisterCodeNotFound,
    UserCanisterInstallCodeCallError((u32, String)),
    UserCanisterStatusCallError((u32, String)),
    UserCanisterModuleVerificationError,
    UserCanisterStartCanisterCallError((u32, String)),
    UserCanisterUpdateSettingsCallError((u32, String)),
    CollectCyclesLedgerTopupCyclesCmcIcpTransferError(LedgerTopupCyclesCmcIcpTransferError),
    CollectCyclesLedgerTopupCyclesCmcNotifyError(LedgerTopupCyclesCmcNotifyError),
    ReferralUserReferralPaymentCyclesTransferCallError((u32, String)),
    UserReferralPaymentCyclesTransferCallError((u32, String)),
    CollectIcpTransferError(IcpTransferError),
    CollectIcpTransferCallError((u32, String)),
    
}


#[derive(CandidType, Deserialize)]
pub enum NewUserError{
    ReferralUserCannotBeTheCaller,
    CheckIcpBalanceCallError((u32, String)),
    CheckCurrentXdrPerMyriadPerIcpCmcRateError(CheckCurrentXdrPerMyriadPerIcpCmcRateError),
    UserIcpLedgerBalanceTooLow{
        cts_user_contract_cost_icp: IcpTokens,
        user_icp_ledger_balance: IcpTokens,
        icp_ledger_transfer_fee: IcpTokens
    },
    NewUserIsInTheMiddleOfAnotherNewUserCall, // in the frontcode on this error, wait 5-10 seconds and call again. if it gives back the FoundUserCanister(UserCanisterId) error, then log the user_canister and the new-user-setup is complete.
    CallWithTheAlreadySetParameters(NewUserQuest), // on this error re-try the call with the already set parameters by an earlier unfinished call.
    MaxNewUsers,
    FoundUserCanister(UserCanisterId),
    ReferralUserNotFound,
    CreateUserCanisterCmcNotifyError(CmcNotifyError),
    MidCallError(NewUserMidCallError),    // re-try the call on this sponse
}


#[derive(CandidType, Deserialize, Clone, PartialEq, Eq)]
pub struct NewUserQuest {
    opt_referral_user_id: Option<UserId>,
}


#[derive(CandidType, Deserialize)]
pub struct NewUserSuccessData {
    user_canister_id: UserCanisterId,
}


fn write_new_user_data(user_id: &Principal, new_user_data: NewUserData) {
    with_mut(&CTS_DATA, |cts_data| {
        match cts_data.new_users.get_mut(user_id) {
            Some(nud) => { *nud = new_user_data; },
            None => {}
        }
    });
}

// for the now a user must pay with the icp.
#[update]
pub async fn new_user(q: NewUserQuest) -> Result<NewUserSuccessData, NewUserError> {

    let user_id: Principal = caller();
    
    new_user_(user_id, q).await
}

async fn new_user_(user_id: UserId, q: NewUserQuest) -> Result<NewUserSuccessData, NewUserError> {

    if let Some(ref referral_user_id) = q.opt_referral_user_id {
        if *referral_user_id == user_id {
            return Err(NewUserError::ReferralUserCannotBeTheCaller);
        }
    }
    
    let optional_new_user_data: Option<NewUserData> = {
        let r: Result<Option<NewUserData>, NewUserError> = with_mut(&CTS_DATA, |cts_data| {
            match cts_data.new_users.get_mut(&user_id) {
                Some(nud) => {
                    if nud.lock == true {
                        return Err(NewUserError::NewUserIsInTheMiddleOfAnotherNewUserCall);
                    }
                    if q != nud.new_user_quest {
                        return Err(NewUserError::CallWithTheAlreadySetParameters(nud.new_user_quest.clone()));
                    }
                    nud.lock = true;
                    Ok(Some(nud.clone()))
                },
                None => {
                    if get(&STOP_CALLS) { trap("Maintenance. try again soon."); }
                    Ok(None)
                }
            }
        });
        r?
    };    
    let mut new_user_data: NewUserData = match optional_new_user_data {
        None => {
            // if icp balance good, create nud in nuds and set new_user_data else return err balance too low

            let (
                check_user_icp_ledger_balance_sponse,
                check_current_xdr_permyriad_per_icp_cmc_rate_sponse,
            ): (
                CallResult<IcpTokens>,
                CheckCurrentXdrPerMyriadPerIcpCmcRateSponse
            ) = futures::future::join(
                check_user_icp_ledger_balance(&user_id), 
                check_current_xdr_permyriad_per_icp_cmc_rate()
            ).await; 
            
            let user_icp_ledger_balance: IcpTokens = match check_user_icp_ledger_balance_sponse {
                Ok(tokens) => tokens,
                Err(check_balance_call_error) => {
                    // not in there yet // with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                    return Err(NewUserError::CheckIcpBalanceCallError((check_balance_call_error.0 as u32, check_balance_call_error.1)));
                }
            };
                    
            let current_xdr_icp_rate: u64 = match check_current_xdr_permyriad_per_icp_cmc_rate_sponse {
                Ok(rate) => rate,
                Err(check_xdr_icp_rate_error) => {
                    // not in there yet // with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                    return Err(NewUserError::CheckCurrentXdrPerMyriadPerIcpCmcRateError(check_xdr_icp_rate_error));
                }
            };
            
            let current_membership_cost_icp: IcpTokens = cycles_to_icptokens(NEW_USER_CONTRACT_COST_CYCLES, current_xdr_icp_rate); 
            
            if user_icp_ledger_balance < current_membership_cost_icp + IcpTokens::from_e8s(ICP_LEDGER_TRANSFER_DEFAULT_FEE.e8s() * 2) {
                // not in there yet // with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                return Err(NewUserError::UserIcpLedgerBalanceTooLow{
                    cts_user_contract_cost_icp: current_membership_cost_icp,
                    user_icp_ledger_balance,
                    icp_ledger_transfer_fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE
                });
            }

            let r: Result<NewUserData, NewUserError> = with_mut(&CTS_DATA, |cts_data| {
                match cts_data.new_users.get_mut(&user_id) {
                    Some(nud) => { // checking again here if Some bc this is within a different [exe]cution
                        if nud.lock == true {
                            return Err(NewUserError::NewUserIsInTheMiddleOfAnotherNewUserCall);
                        }
                        if q != nud.new_user_quest {
                            return Err(NewUserError::CallWithTheAlreadySetParameters(nud.new_user_quest.clone()));
                        }
                        nud.lock = true;
                        Ok(nud.clone())
                    },
                    None => {
                        if cts_data.new_users.len() >= MAX_NEW_USERS {
                            return Err(NewUserError::MaxNewUsers);
                        }
                        let nud: NewUserData = NewUserData{
                            start_time_nanos: time(),
                            lock: true,
                            current_xdr_icp_rate: current_xdr_icp_rate,
                            new_user_quest: q,
                            // the options and bools are for the memberance of the steps
                            look_if_user_is_in_the_users_map_canisters: false,
                            look_if_referral_user_is_in_the_users_map_canisters: false,
                            create_user_canister_block_height: None,
                            user_canister: None,
                            users_map_canister: None,
                            user_canister_uninstall_code: false,
                            user_canister_install_code: false,
                            user_canister_status_record: None,
                            collect_icp: false,
                        };
                        cts_data.new_users.insert(user_id, nud.clone());
                        Ok(nud)
                    }
                }
            });
            
            r?
            
            // or can use the '?'9 operator on the r
            /*
            match r {
                Ok(nud) => nud,
                Err(new_user_error) => return Err(new_user_error)
            }
            */
            
        },
        
        Some(nud) => nud        
        
    };
    
    
    if new_user_data.look_if_user_is_in_the_users_map_canisters == false {
        // check in the list of the users-whos cycles-balance is save but without a user-canister 
        
        match find_user_canister_of_the_specific_user(user_id).await {
            Ok(opt_user_canister_id) => match opt_user_canister_id {
                Some(user_canister_id) => {
                    with_mut(&CTS_DATA, |cts_data| { cts_data.new_users.remove(&user_id); });
                    return Err(NewUserError::FoundUserCanister(user_canister_id));
                },
                None => {
                    new_user_data.look_if_user_is_in_the_users_map_canisters = true;
                }
            },
            Err(find_user_in_the_users_map_canisters_error) => match find_user_in_the_users_map_canisters_error {
                FindUserInTheUsersMapCanistersError::UsersMapCanistersFindUserCallFails(umc_call_errors) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::UsersMapCanistersFindUserCallFails(umc_call_errors)));
                }
            }
        }
        
    }
    
    if new_user_data.new_user_quest.opt_referral_user_id.is_some() {
    
        if new_user_data.referral_user_canister_id.is_none() {
        
            match find_user_canister_of_the_specific_user(new_user_data.new_user_quest.opt_referral_user_id.as_ref().unwrap().clone()).await {
                Ok(opt_user_canister_id) => match opt_user_canister_id {
                    Some(user_canister_id) => {
                        new_user_data.referral_user_canister_id = Some(user_canister_id);
                    },
                    None => {
                        with_mut(&CTS_DATA, |cts_data| { cts_data.new_users.remove(&user_id); });
                        return Err(NewUserError::ReferralUserNotFound);
                    }
                },
                Err(find_user_in_the_users_map_canisters_error) => match find_user_in_the_users_map_canisters_error {
                    FindUserInTheUsersMapCanistersError::UsersMapCanistersFindUserCallFails(umc_call_errors) => {
                        new_user_data.lock = false;
                        write_new_user_data(&user_id, new_user_data);
                        return Err(NewUserError::MidCallError(NewUserMidCallError::UsersMapCanistersFindUserCallFails(umc_call_errors)));
                    }
                }
            }
            
        }
        
    }
    

    if new_user_data.create_user_canister_block_height.is_none() {
        let create_user_canister_block_height: IcpBlockHeight = match icp_transfer(
            MAINNET_LEDGER_CANISTER_ID,
            IcpTransferArgs {
                memo: ICP_LEDGER_CREATE_CANISTER_MEMO,
                amount: cycles_to_icptokens(NEW_USER_CANISTER_CREATION_CYCLES, new_user_data.current_xdr_icp_rate),
                fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE,
                from_subaccount: Some(principal_icp_subaccount(&user_id)),
                to: IcpId::new(&MAINNET_CYCLES_MINTING_CANISTER_ID, &principal_icp_subaccount(&id())),
                created_at_time: Some(IcpTimestamp { timestamp_nanos: time() })
            }
        ).await {
            Ok(transfer_result) => match transfer_result {
                Ok(block_height) => block_height,
                Err(transfer_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::CreateUserCanisterIcpTransferError(transfer_error)));                    
                }
            },
            Err(transfer_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::CreateUserCanisterIcpTransferCallError((transfer_call_error.0 as u32, transfer_call_error.1))));
            }
        };
    
        new_user_data.create_user_canister_block_height = Some(create_user_canister_block_height);
    }


    if new_user_data.user_canister.is_none() {
    
        let user_canister: Principal = match call::<(CmcNotifyCreateCanisterQuest,), (Result<Principal, CmcNotifyError>,)>(
            MAINNET_CYCLES_MINTING_CANISTER_ID,
            "notify_create_canister",
            (CmcNotifyCreateCanisterQuest {
                controller: id(),
                block_index: new_user_data.create_user_canister_block_height.unwrap()
            },)
        ).await {
            Ok((notify_result,)) => match notify_result {
                Ok(new_canister_id) => new_canister_id,
                Err(cmc_notify_error) => {
                    // match on the cmc_notify_error, if it failed bc of the cmc icp transfer block height expired, remove the user from the NEW_USERS map.     
                    match cmc_notify_error {
                        CmcNotifyError::TransactionTooOld(_) | CmcNotifyError::Refunded{ .. } => {
                            with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                            return Err(NewUserError::CreateUserCanisterCmcNotifyError(cmc_notify_error));
                        },
                        CmcNotifyError::InvalidTransaction(_) // 
                        | CmcNotifyError::Other{ .. }
                        | CmcNotifyError::Processing
                        => {
                            new_user_data.lock = false;
                            write_new_user_data(&user_id, new_user_data);
                            return Err(NewUserError::MidCallError(NewUserMidCallError::CreateUserCanisterCmcNotifyError(cmc_notify_error)));   
                        },
                    }                    
                }
            },
            Err(cmc_notify_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::CreateUserCanisterCmcNotifyCallError((cmc_notify_call_error.0 as u32, cmc_notify_call_error.1))));
            }      
        };
        
        new_user_data.user_canister = Some(user_canister);
        with_mut(&USER_CANISTER_CACHE, |uc_cache| { uc_cache.put(user_id, Some(user_canister)); });
        new_user_data.user_canister_uninstall_code = true; // because a fresh cmc canister is empty 
    }
        
 

    if new_user_data.user_canister_uninstall_code == false {
        
        match call::<(CanisterIdRecord,), ()>(
            MANAGEMENT_CANISTER_ID,
            "uninstall_code",
            (CanisterIdRecord { canister_id: new_user_data.user_canister.unwrap() },),
        ).await {
            Ok(_) => {},
            Err(uninstall_code_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterUninstallCodeCallError(format!("{:?}", uninstall_code_call_error))));
            }
        }
        
        new_user_data.user_canister_uninstall_code = true;
    }


    if new_user_data.user_canister_install_code == false {
    
        if with(&CTS_DATA, |cts_data| { cts_data.user_canister_code.module().len() == 0 }) {
            new_user_data.lock = false;
            write_new_user_data(&user_id, new_user_data);
            return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterCodeNotFound));
        }

        match call::<(ManagementCanisterInstallCodeQuest,), ()>(
            MANAGEMENT_CANISTER_ID,
            "install_code",
            (ManagementCanisterInstallCodeQuest {
                mode : ManagementCanisterInstallCodeMode::install,
                canister_id : new_user_data.user_canister.unwrap(),
                wasm_module : unsafe{&*with(&CTS_DATA, |cts_data| { cts_data.user_canister_code.module() as *const Vec<u8> })},
                arg : &encode_one(&UserCanisterInit{ 
                    cts_id: id(), 
                    user_id: user_id,
                    user_canister_storage_size_mib: NEW_USER_CONTRACT_STORAGE_SIZE_MiB,                         
                    user_canister_lifetime_termination_timestamp_seconds: time()/1_000_000_000 + NEW_USER_CONTRACT_LIFETIME_DURATION_SECONDS,
                    cycles_transferrer_canisters: with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canisters.clone() })
                }).unwrap()
            },),
        ).await {
            Ok(()) => {},
            Err(put_code_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterInstallCodeCallError((put_code_call_error.0 as u32, put_code_call_error.1))));
            }
        }
        
        new_user_data.user_canister_install_code = true;
    }
    
    if new_user_data.user_canister_status_record.is_none() {
        
        let canister_status_record: ManagementCanisterCanisterStatusRecord = match call(
            MANAGEMENT_CANISTER_ID,
            "canister_status",
            (CanisterIdRecord { canister_id: new_user_data.user_canister.unwrap() },),
        ).await {
            Ok((canister_status_record,)) => canister_status_record,
            Err(canister_status_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterStatusCallError((canister_status_call_error.0, canister_status_call_error.1))));
            }
        };
        
        new_user_data.user_canister_status_record = Some(canister_status_record);
    }
        
    // no async in this if-block so no NewUserData field needed. can make it for the optimization though
    if with(&CTS_DATA, |cts_data| { cts_data.user_canister_code.module().len() == 0 }) {
        new_user_data.lock = false;
        write_new_user_data(&user_id, new_user_data);
        return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterCodeNotFound));
    }
    if new_user_data.user_canister_status_record.as_ref().unwrap().module_hash.is_none() || new_user_data.user_canister_status_record.as_ref().unwrap().module_hash.as_ref().unwrap().clone() != with(&CTS_DATA, |cts_data| { cts_data.user_canister_code.module_hash().clone() }) {
        // go back a couple of steps
        new_user_data.user_canister_uninstall_code = false;                                  
        new_user_data.user_canister_install_code = false;
        new_user_data.user_canister_status_record = None;
        new_user_data.lock = false;
        write_new_user_data(&user_id, new_user_data);
        return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterModuleVerificationError));
    }
    

    if new_user_data.user_canister_status_record.as_ref().unwrap().status != ManagementCanisterCanisterStatusVariant::running {
    
        match call::<(CanisterIdRecord,), ()>(
            MANAGEMENT_CANISTER_ID,
            "start_canister",
            (CanisterIdRecord { canister_id: new_user_data.user_canister.unwrap() },)
        ).await {
            Ok(_) => {
                new_user_data.user_canister_status_record.as_mut().unwrap().status = ManagementCanisterCanisterStatusVariant::running; 
            },
            Err(start_canister_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterStartCanisterCallError((start_canister_call_error.0 as u32, start_canister_call_error.1))));
            }
        }
        
    }

    
    if new_user_data.users_map_canister.is_none() {
        
        let users_map_canister_id: UsersMapCanisterId = match put_new_user_into_a_users_map_canister(
            user_id, 
            UMCUserData{
                user_canister_id: new_user_data.user_canister.as_ref().unwrap().clone(),
                user_canister_latest_known_module_hash: new_user_data.user_canister_status_record.as_ref().unwrap().module_hash.as_ref().unwrap().clone()
            }
        ).await {
            Ok(umcid) => umcid,
            Err(put_new_user_into_a_users_map_canister_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::PutNewUserIntoAUsersMapCanisterError(put_new_user_into_a_users_map_canister_error)));
            }
        };
        
        new_user_data.users_map_canister = Some(users_map_canister_id);
    }



    //update the controller to clude the users_map_canister
    
    let put_user_canister_settings: ManagementCanisterCanisterSettings = ManagementCanisterCanisterSettings{
        controllers : vec![
            id(), 
            new_user_data.users_map_canister.as_ref().unwrap().clone(),
            new_user_data.user_canister.as_ref().unwrap().clone(),
        ],
        compute_allocation : 0,
        memory_allocation : NEW_USER_CANISTER_NETWORK_MEMORY_ALLOCATION_MiB as u128 * MiB as u128,
        freezing_threshold : 2592000 
    };
    
    if new_user_data.user_canister_status_record.as_ref().unwrap().settings != put_user_canister_settings {
                
        match call::<(ChangeCanisterSettingsRecord,), ()>(
            MANAGEMENT_CANISTER_ID,
            "update_settings",
            (ChangeCanisterSettingsRecord{
                canister_id: new_user_data.user_canister.as_ref().unwrap().clone(),
                settings: ManagementCanisterOptionalCanisterSettings{
                    controllers : Some(put_user_canister_settings.controllers),
                    compute_allocation : Some(put_user_canister_settings.compute_allocation),
                    memory_allocation : Some(put_user_canister_settings.memory_allocation),
                    freezing_threshold : Some(put_user_canister_settings.freezing_threshold),
                }
            },)
        ).await {
            Ok(()) => {
                new_user_data.user_canister_status_record.as_mut().unwrap().settings = put_user_canister_settings;
            },
            Err(update_settings_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterUpdateSettingsCallError((update_settings_call_error.0 as u32, update_settings_call_error.1))));
            }
        }
    }
    
    
    // hand out the referral-bonuses if there is.
    if new_user_data.new_user_quest.opt_referral_user_id.is_some() {
        
        if new_user_data.collect_cycles_cmc_icp_transfer_block_height.is_none() {
            match ledger_topup_cycles_cmc_icp_transfer(
                cycles_to_icptokens(NEW_USER_CONTRACT_COST_CYCLES - NEW_USER_CANISTER_CREATION_CYCLES, new_user_data.current_xdr_icp_rate), 
                Some(principal_icp_subaccount(&user_id)), 
                topup_canister: id()
            ).await {
                Ok(block_height) => {
                    new_user_data.collect_cycles_cmc_icp_transfer_block_height = Some(block_height);
                },
                Err(ledger_topup_cycles_cmc_icp_transfer_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::CollectCyclesLedgerTopupCyclesCmcIcpTransferError(ledger_topup_cycles_cmc_icp_transfer_error)));
                }
            }
        }
        
        if new_user_data.collect_cycles_cmc_notify.is_none() {
            match ledger_topup_cycles_cmc_notify(new_user_data.collect_cycles_cmc_icp_transfer_block_height.unwrap(), id()).await {
                Ok(topup_cycles) => {
                    new_user_data.collect_cycles_cmc_notify = Some(topup_cycles); 
                }, 
                Err(ledger_topup_cycles_cmc_notify_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::CollectCyclesLedgerTopupCyclesCmcNotifyError(ledger_topup_cycles_cmc_notify_error)));
                }
            }
        }
        
        if new_user_data.referral_user_referral_payment_cycles_transfer == false {
            match call_with_payment128::<(CyclesTransfer,), ()>(
                new_user_data.referral_user_canister_id.as_ref().unwrap().clone(),        
                "cycles_transfer",
                (CyclesTransfer{
                    memo: CyclesTransferMemo::Blob(b"CTS-REFERRAL-PAYMENT".to_vec())
                },),
                1_000_000_000_000
            ).await {
                Ok(()) => {
                    new_user_data.referral_user_referral_payment_cycles_transfer = true;
                }, 
                Err(referral_user_referral_payment_cycles_transfer_call_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::ReferralUserReferralPaymentCyclesTransferCallError((referral_user_referral_payment_cycles_transfer_call_error.0 as u32, referral_user_referral_payment_cycles_transfer_call_error.1))));
                }
            }
        }
        
        if new_user_data.user_referral_payment_cycles_transfer == false {
            match call_with_payment128::<(CyclesTransfer,), ()>(
                new_user_data.user_canister.as_ref().unwrap().clone(),
                "cycles_transfer",
                (CyclesTransfer{
                    memo: CyclesTransferMemo::Blob(b"CTS-REFERRAL-PAYMENT".to_vec())
                },),
                1_000_000_000_000
            ).await {
                Ok(()) => {
                    new_user_data.user_referral_payment_cycles_transfer = true;
                }, 
                Err(user_referral_payment_cycles_transfer_call_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::UserReferralPaymentCyclesTransferCallError((user_referral_payment_cycles_transfer_call_error.0 as u32, user_referral_payment_cycles_transfer_call_error.1))));
                }
            }
        }
        
    } else {
        
        if new_user_data.collect_icp == false {
            match take_user_icp_ledger(&user_id, cycles_to_icptokens(NEW_USER_CONTRACT_COST_CYCLES - NEW_USER_CANISTER_CREATION_CYCLES, new_user_data.current_xdr_icp_rate)).await {
                Ok(icp_transfer_result) => match icp_transfer_result {
                    Ok(_block_height) => {
                        new_user_data.collect_icp = true;
                    },
                    Err(icp_transfer_error) => {
                        new_user_data.lock = false;
                        write_new_user_data(&user_id, new_user_data);
                        return Err(NewUserError::MidCallError(NewUserMidCallError::CollectIcpTransferError(icp_transfer_error)));          
                    }
                }, 
                Err(icp_transfer_call_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::CollectIcpTransferCallError((icp_transfer_call_error.0 as u32, icp_transfer_call_error.1))));          
                }               
            }
        }
    
    }
    


    with_mut(&CTS_DATA, |cts_data| { cts_data.new_users.remove(&user_id); });
    
    Ok(NewUserSuccessData {
        user_canister_id: new_user_data.user_canister.unwrap()
    })
}





// ----------------------------------------------------------------------------------------------------





mod user_canister_cache {
    use super::{UserId, UserCanisterId, time};
    use std::collections::{HashMap};
    
    // private
    #[derive(Clone, Copy)]
    struct FindUserSponseCacheData {
        timestamp_nanos: u64,
        opt_user_canister_id: Option<UserCanisterId>
    }



    // cacha for this. with a max users->user-canisters
    // on a new user, put/update insert the new user into this cache
    // on a user-contract-termination, void[remove/delete] the (user,user-canister)-log in this cache
    
    pub struct UserCanisterCache {
        hashmap: HashMap<UserId, FindUserSponseCacheData>    
    }
    impl UserCanisterCache {
        
        pub const MAX_SIZE: usize = 1400;
        
        pub fn new() -> Self {
            Self {
                hashmap: HashMap::new()
            }
        }
        
        pub fn put(&mut self, user_id: UserId, opt_user_canister_id: Option<UserCanisterId>) {
            if self.hashmap.len() >= Self::MAX_SIZE {
                self.hashmap.remove(
                    &(self.hashmap.iter().min_by_key(
                        |(user_id, find_user_sponse_cache_data)| {
                            find_user_sponse_cache_data.timestamp_nanos
                        }
                    ).unwrap().0.clone())
                );
            }
            self.hashmap.insert(user_id, FindUserSponseCacheData{ opt_user_canister_id, timestamp_nanos: time() });
        }
        
        pub fn check(&self, user_id: UserId) -> Option<Option<UserCanisterId>> {
            match self.hashmap.get(&user_id) {
                None => None,
                Some(find_user_sponse_cache_data) => Some(find_user_sponse_cache_data.opt_user_canister_id)
            }
        }
    }

}

use user_canister_cache::UserCanisterCache;


#[derive(CandidType, Deserialize)]
pub enum FindUserCanisterError {
    UserIsInTheNewUsersMap, // in the frontcode on this error, make a call to finish the new_user steps
    FindUserInTheUsersMapCanistersError(FindUserInTheUsersMapCanistersError),
}

#[update]
pub async fn find_user_canister() -> Result<Option<UserCanisterId>, FindUserCanisterError> {
    if localkey::get::(&STOP_CALLS) { trap("Maintenance. try again soon."); }
    
    let user_id: UserId = caller();
    
    if with(&CTS_DATA, |cts_data| { cts_data.new_users.contains_key(&user_id) }) {
        return Err(FindUserCanisterError::UserIsInTheNewUsersMap);
    }
    
    find_user_canister_of_the_specific_user(user_id).await.map_err(
        |find_user_in_the_users_map_canisters_error| { 
            FindUserCanisterError::FindUserInTheUsersMapCanistersError(find_user_in_the_users_map_canisters_error) 
        }
    )

}



async fn find_user_canister_of_the_specific_user(user_id: UserId) -> Result<Option<UserCanisterId>, FindUserInTheUsersMapCanistersError> {
    if let Some(opt_user_canister_id) = with(&USER_CANISTER_CACHE, |uc_cache| { uc_cache.check(user_id) }) {
        return Ok(opt_user_canister_id);
    } 
    find_user_in_the_users_map_canisters(user_id).await.map(
        |opt_umc_user_data_and_umc_id| {
            let opt_user_canister_id: Option<UserCanisterId> = opt_umc_user_data_and_umc_id.map(|(umc_user_data, _umc_id)| { umc_user_data.user_canister_id });
            with_mut(&USER_CANISTER_CACHE, |uc_cache| {
                uc_cache.put(user_id, opt_user_canister_id);
            });    
            opt_user_canister_id
        }
    )   
} 






// ----------------------------------------------------------------------------------------------------




// options are for the memberance of the steps

#[derive(CandidType, Deserialize)]
struct UserBurnIcpMintCyclesData {
    start_time_nanos: u64,
    lock: bool,
    user_burn_icp_mint_cycles_quest: UserBurnIcpMintCyclesQuest, 
    user_canister_id: UserCanisterId,
    cmc_icp_transfer_block_height: Option<IcpBlockHeight>,
    cmc_cycles: Option<Cycles>,
    call_user_canister: bool
}


#[derive(CandidType, Deserialize, PartialEq, Eq, Clone)]
pub struct UserBurnIcpMintCyclesQuest {
    burn_icp: IcpTokens,    
}

#[derive(CandidType, Deserialize)]
pub enum UserBurnIcpMintCyclesError {
    UserIsInTheMiddleOfAnotherUserBurnIcpMintCyclesCall,
    CompleteCallWithTheAlreadySetParameters(UserBurnIcpMintCyclesQuest),
    MinimumUserBurnIcpMintCycles{minimum_user_burn_icp_mint_cycles: Cycles},
    IcpCheckBalanceCallError((u32, String)),
    UserIcpBalanceTooLow{user_icp_balance: IcpTokens, icp_ledger_transfer_fee: IcpTokens},
    FindUserInTheUsersMapCanistersError(FindUserInTheUsersMapCanistersError),
    UserCanisterNotFound,
    MaxUsersBurnIcpMintCycles,
    LedgerTopupCyclesCmcIcpTransferError(LedgerTopupCyclesCmcIcpTransferError), // mid call error? or remove on this error?
    MidCallError(UserBurnIcpMintCyclesMidCallError) // on this error, call with the same-parameters for the completion of this call. 
}


#[derive(CandidType, Deserialize)]
pub enum UserBurnIcpMintCyclesMidCallError {
    LedgerTopupCyclesCmcNotifyError(LedgerTopupCyclesCmcNotifyError),
    CallUserCanisterCallError((u32, String)),
}


#[update]
pub async fn user_burn_icp_mint_cycles(q: UserBurnIcpMintCyclesQuest) -> Result<Cycles, UserBurnIcpMintCyclesError> {

    let user_id: UserId = caller(); 


    let opt_user_burn_icp_mint_cycles_data: Option<UserBurnIcpMintCyclesData> = {
        let r: Result<Option<UserBurnIcpMintCyclesData>, UserBurnIcpMintCyclesError> = with_mut(&CTS_DATA, |cts_data| {
            match cts_data.users_burn_icp_mint_cycles.get_mut(&user_id) {
                Some(user_burn_icp_mint_cycles_data) => {
                    if user_burn_icp_mint_cycles_data.lock == true {
                        return Err(UserBurnIcpMintCyclesError::UserIsInTheMiddleOfAnotherUserBurnIcpMintCyclesCall);
                    }
                    if q != user_burn_icp_mint_cycles_data.user_burn_icp_mint_cycles_quest {
                        return Err(UserBurnIcpMintCyclesError::CompleteCallWithTheAlreadySetParameters(user_burn_icp_mint_cycles_data.user_burn_icp_mint_cycles_quest.clone()));
                    }
                    user_burn_icp_mint_cycles_data.lock = true;
                    Ok(Some(user_burn_icp_mint_cycles_data.clone()))
                },
                None => {
                    if get(&STOP_CALLS) { trap("Maintenance. try again soon."); }
                    Ok(None)
                }
            }
        });
        r?
    }; 
    
    let mut user_burn_icp_mint_cycles_data: UserBurnIcpMintCyclesData = match opt_user_burn_icp_mint_cycles_data {
        Some(user_burn_icp_mint_cycles_data) => user_burn_icp_mint_cycles_data,
        None => {
    
            if q.burn_icp < MINIMUM_USER_BURN_ICP_MINT_CYCLES {
                return Err(UserBurnIcpMintCyclesError::MinimumUserBurnIcpMintCycles{
                    minimum_user_burn_icp_mint_cycles: MINIMUM_USER_BURN_ICP_MINT_CYCLES
                });
            }
            
            let user_icp_balance: IcpTokens = match check_user_icp_ledger_balance(&user_id).await {
                Ok(icp_tokens) => icp_tokens,
                Err(icp_check_balance_call_error) => {
                    return Err(UserBurnIcpMintCyclesError::IcpCheckBalanceCallError((icp_check_balance_call_error.0 as u32, icp_check_balance_call_error.1)));
                }
            };
            
            if user_icp_balance < q.burn_icp + ICP_LEDGER_TRANSFER_DEFAULT_FEE {
                return Err(UserBurnIcpMintCyclesError::UserIcpBalanceTooLow{
                    user_icp_balance,
                    icp_ledger_transfer_fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE
                });
            }
            
            let user_canister_id: UserCanisterId = match find_user_canister_of_the_specific_user(user_id).await {
                Ok(opt_user_canister_id) => match opt_user_canister_id {
                    None => return Err(UserBurnIcpMintCyclesError::UserCanisterNotFound),
                    Some(user_canister_id) => user_canister_id
                },
                Err(find_user_in_the_users_map_canisters_error) => {
                    return Err(UserBurnIcpMintCyclesError::FindUserInTheUsersMapCanistersError(find_user_in_the_users_map_canisters_error));
                }
            };
            
            let r: Result<UserBurnIcpMintCyclesData, UserBurnIcpMintCyclesError> = with_mut(&CTS_DATA, |cts_data| {
                match cts_data.users_burn_icp_mint_cycles.get_mut(&user_id) {
                    Some(user_burn_icp_mint_cycles_data) => {
                        if user_burn_icp_mint_cycles_data.lock == true {
                            return Err(UserBurnIcpMintCyclesError::UserIsInTheMiddleOfAnotherUserBurnIcpMintCyclesCall);
                        }
                        if q != user_burn_icp_mint_cycles_data.user_burn_icp_mint_cycles_quest {
                            return Err(UserBurnIcpMintCyclesError::CompleteCallWithTheAlreadySetParameters(user_burn_icp_mint_cycles_data.user_burn_icp_mint_cycles_quest.clone()));
                        }
                        user_burn_icp_mint_cycles_data.lock = true;
                        Ok(user_burn_icp_mint_cycles_data.clone())
                    },
                    None => {
                        if cts_data.users_burn_icp_mint_cycles.len() >= MAX_USERS_BURN_ICP_MINT_CYCLES {
                            return Err(UserBurnIcpMintCyclesError::MaxUsersBurnIcpMintCycles);
                        }
                        let user_burn_icp_mint_cycles_data: UserBurnIcpMintCyclesData = UserBurnIcpMintCyclesData{
                            start_time_nanos: time(),
                            lock: true,
                            user_burn_icp_mint_cycles_quest: q, 
                            user_canister_id,
                            cmc_icp_transfer_block_height: None,
                            cmc_cycles: None,
                            call_user_canister: false
                        };
                        cts_data.users_burn_icp_mint_cycles.insert(user_id, user_burn_icp_mint_cycles_data.clone());
                        Ok(user_burn_icp_mint_cycles_data)
                    }
                }
            });
            r?
        }
    };
    
    // this is after the put into the state bc if this is success the block height must be save in the state
    if user_burn_icp_mint_cycles_data.cmc_icp_transfer_block_height.is_none() {
        match ledger_topup_cycles_cmc_icp_transfer(user_burn_icp_mint_cycles_data.user_burn_icp_mint_cycles_quest.burn_icp, Some(principal_icp_subaccount(&user_id)), id()).await {
            Ok(block_height) => { user_burn_icp_mint_cycles_data.cmc_icp_transfer_block_height = Some(block_height); },
            Err(ledger_topup_cycles_cmc_icp_transfer_error) => {
                with_mut(CTS_DATA, |cts_data| { cts_data.users_burn_icp_mint_cycles.remove(&user_id); }); // remove? or unlock, write_data, and mid call error?
                return Err(UserBurnIcpMintCyclesError::LedgerTopupCyclesCmcIcpTransferError(ledger_topup_cycles_cmc_icp_transfer_error));
            }
        }
    }
    
    if user_burn_icp_mint_cycles_data.cmc_cycles.is_none() {
        match ledger_topup_cycles_cmc_notify(user_burn_icp_mint_cycles_data.cmc_icp_transfer_block_height.unwrap(), id()).await {
            Ok(cmc_cycles) => { user_burn_icp_mint_cycles_data.cmc_cycles = Some(cmc_cycles); },
            Err(ledger_topup_cycles_cmc_notify_error) => {
                user_burn_icp_mint_cycles_data.lock = false;
                with_mut(&CTS_DATA, |cts_data| {
                    match cts_data.users_burn_icp_mint_cycles.get_mut(&user_id) {
                        Some(data) => { *data = user_burn_icp_mint_cycles_data; },
                        None => {}
                    }
                });
                return Err(UserBurnIcpMintCyclesError::MidCallError(UserBurnIcpMintCyclesMidCallError::LedgerTopupCyclesCmcNotifyError(ledger_topup_cycles_cmc_notify_error)));
            }
        }
    }
    
    let cycles_for_the_user_canister: Cycles = user_burn_icp_mint_cycles_data.cmc_cycles.unwrap().checked_sub(USER_BURN_ICP_MINT_CYCLES_FEE).unwrap_or(user_burn_icp_mint_cycles_data.cmc_cycles.unwrap());
    if user_burn_icp_mint_cycles_data.call_user_canister == false {
        match call_with_payment128::<(CyclesTransfer,), ()>(
            user_burn_icp_mint_cycles_data.user_canister_id,
            "cycles_transfer",
            (CyclesTransfer{
                memo: CyclesTransferMemo::Blob(b"CTS-BURN-ICP-MINT-CYCLES".to_vec())
            },),
            cycles_for_the_user_canister
        ).await {
            Ok(()) => { user_burn_icp_mint_cycles_data.call_user_canister = true; },
            Err(call_error) => {
                user_burn_icp_mint_cycles_data.lock = false;
                with_mut(&CTS_DATA, |cts_data| {
                    match cts_data.users_burn_icp_mint_cycles.get_mut(&user_id) {
                        Some(data) => { *data = user_burn_icp_mint_cycles_data; },
                        None => {}
                    }
                });
                return Err(UserBurnIcpMintCyclesError::MidCallError(UserBurnIcpMintCyclesMidCallError::CallUserCanisterCallError((call_error.0 as u32, call_error.1))));
            }
        }
    }
    
    with_mut(CTS_DATA, |cts_data| { cts_data.users_burn_icp_mint_cycles.remove(&user_id); });
    Ok(cycles_for_the_user_canister)
    
}






















// --------------------------------------------------------------------------
// :CONTROLLER-METHODS.





/*
#[update]
pub async fn controller_see_balance() -> SeeBalanceSponse {
    let cycles_balance: u128 = ic_cdk::api::canister_balance128();
    let icp_balance: IcpTokens = match icp_account_balance(
        MAINNET_LEDGER_CANISTER_ID,
        IcpAccountBalanceArgs {
            account : main_cts_icp_id()
        }
    ).await {
        Ok(tokens) => tokens,
        Err(balance_call_error) => {
            return Err(SeeBalanceError::IcpLedgerCheckBalanceCallError(format!("{:?}", balance_call_error)));
        } 
    };
    Ok(UserBalance {
        cycles_balance,
        icp_balance,
    })
}
*/






// ----- USERS_MAP_CANISTERS-METHODS --------------------------



#[update]
pub fn controller_put_umc_code(canister_code: CanisterCode) -> () {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&CTS_DATA, |cts_data| {
        cts_data.users_map_canister_code = canister_code;
    });
}




// certification? or replication-calls?
#[export_name = "canister_query controller_see_users_map_canisters"]
pub fn controller_see_users_map_canisters() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with(&CTS_DATA, |cts_data| {
        ic_cdk::api::call::reply::<(&Vec<Principal>,)>((&(cts_data.users_map_canisters),));
    });
}



pub type ControllerUpgradeUMCError = (Principal, ControllerUpgradeUMCCallErrorType, (u32, String)); 

#[derive(CandidType, Deserialize)]
pub enum ControllerUpgradeUMCCallErrorType {
    StopCanisterCallError,
    UpgradeCodeCallError,
    StartCanisterCallError
}



#[update]
pub async fn controller_upgrade_umcs(opt_upgrade_umcs: Option<Vec<UsersMapCanisterId>>, post_upgrade_arg: Vec<u8>) -> Vec<ControllerUpgradeUMCError>/*umcs that upgrade call-fail*/ {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    if with(&CTS_DATA, |cts_data| cts_data.users_map_canister_code.module().len() == 0 ) {
        trap("USERS_MAP_CANISTER_CODE.module().len() is 0.")
    }
    
    let upgrade_umcs: Vec<Principal> = {
        if let Some(upgrade_umcs) = opt_upgrade_umcs {
            with(&CTS_DATA, |cts_data| { 
                upgrade_umcs.iter().for_each(|upgrade_umc| {
                    if cts_data.users_map_canisters.contains(&upgrade_umc) == false {
                        trap(&format!("cts users_map_canisters does not contain: {:?}", upgrade_umc));
                    }
                });
            });    
            upgrade_umcs
        } else {
            with(&CTS_DATA, |cts_data| { cts_data.users_map_canisters.clone() })
        }
    };     
    
    let sponses: Vec<Result<(), ControllerUpgradeUMCError>> = futures::future::join_all(
        upgrade_umcs.iter().map(|umc_id| {
            async {
            
                match call::<(CanisterIdRecord,), ()>(
                    MANAGEMENT_CANISTER_ID,
                    "stop_canister",
                    (CanisterIdRecord{ canister_id: *umc_id/*copy*/ },)
                ).await {
                    Ok(_) => {},
                    Err(stop_canister_call_error) => {
                        return Err((*umc_id/*copy*/, ControllerUpgradeUMCCallErrorType::StopCanisterCallError, (stop_canister_call_error.0 as u32, stop_canister_call_error.1))); 
                    }
                }
            
                match call_raw128(
                    MANAGEMENT_CANISTER_ID,
                    "install_code",
                    &encode_one(&ManagementCanisterInstallCodeQuest{
                        mode : ManagementCanisterInstallCodeMode::upgrade,
                        canister_id : *umc_id/*copy*/,
                        wasm_module : unsafe {&*with(&CTS_DATA, |cts_data| { cts_data.users_map_canister_code.module() as *const Vec<u8> })},
                        arg : &post_upgrade_arg,
                    }).unwrap(),
                    0
                ).await {
                    Ok(_) => {},
                    Err(upgrade_code_call_error) => {
                        return Err((*umc_id/*copy*/, ControllerUpgradeUMCCallErrorType::UpgradeCodeCallError, (upgrade_code_call_error.0 as u32, upgrade_code_call_error.1)));
                    }
                }

                match call::<(CanisterIdRecord,), ()>(
                    MANAGEMENT_CANISTER_ID,
                    "start_canister",
                    (CanisterIdRecord{ canister_id: *umc_id/*copy*/ },)
                ).await {
                    Ok(_) => {},
                    Err(start_canister_call_error) => {
                        return Err((umc_id.clone(), ControllerUpgradeUMCCallErrorType::StartCanisterCallError, (start_canister_call_error.0 as u32, start_canister_call_error.1))); 
                    }
                }
                
                Ok(())
            }
        }).collect::<Vec<_>>()
    ).await;
    
    
    sponses.into_iter().filter_map(
        |upgrade_umc_sponse: Result<(), ControllerUpgradeUMCError>| {
            match upgrade_umc_sponse {
                Ok(_) => None,
                Err(upgrade_umc_error) => Some(upgrade_umc_error)
            }
        }
    ).collect::<Vec<ControllerUpgradeUMCError>>()
    
}




// ----- USER_CANISTERS-METHODS --------------------------


#[update]
pub fn controller_put_user_canister_code(canister_code: CanisterCode) -> () {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&CTS_DATA, |cts_data| {
        cts_data.user_canister_code = canister_code;
    });
}



pub type ControllerPutUCCodeOntoTheUMCError = (UsersMapCanisterId, (u32, String));

#[update]
pub async fn controller_put_uc_code_onto_the_umcs(opt_umcs: Option<Vec<UsersMapCanisterId>>) -> Vec<ControllerPutUCCodeOntoTheUMCError>/*umcs that the put_uc_code call fail*/ {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
        
    if with(&CTS_DATA, |cts_data| cts_data.user_canister_code.module().len() == 0 ) {
        trap("USER_CANISTER_CODE.module().len() is 0.")
    }
    
    let call_umcs: Vec<UsersMapCanisterId> = {
        if let Some(call_umcs) = opt_umcs {
            with(&CTS_DATA, |cts_data| { 
                call_umcs.iter().for_each(|call_umc| {
                    if cts_data.users_map_canisters.contains(&call_umc) == false {
                        trap(&format!("cts users_map_canisters does not contain: {:?}", call_umc));
                    }
                });
            });    
            call_umcs
        } else {
            with(&CTS_DATA, |cts_data| { cts_data.users_map_canisters.clone() })
        }
    };    
    
    let sponses: Vec<Result<(), ControllerPutUCCodeOntoTheUMCError>> = futures::future::join_all(
        call_umcs.iter().map(|call_umc| {
            async {
                match call::<(&CanisterCode,), ()>(
                    *call_umc,
                    "cts_put_user_canister_code",
                    (unsafe{&*with(&CTS_DATA, |cts_data| { &(cts_data.user_canister_code) as *const CanisterCode })},)
                ).await {
                    Ok(_) => {},
                    Err(call_error) => {
                        return Err((call_umc.clone(), (call_error.0 as u32, call_error.1)));
                    }
                }
                
                Ok(())
            }
        }).collect::<Vec<_>>()
    ).await;
    
    sponses.into_iter().filter_map(
        |call_umc_sponse: Result<(), ControllerPutUCCodeOntoTheUMCError>| {
            match call_umc_sponse {
                Ok(()) => None,
                Err(call_umc_error) => Some(call_umc_error)
            }
        }
    ).collect::<Vec<ControllerPutUCCodeOntoTheUMCError>>()
}



#[derive(CandidType, Deserialize)]
pub enum ControllerUpgradeUCSOnAUMCError {
    CTSUpgradeUCSCallError((u32, String))
}



#[update]
pub async fn controller_upgrade_ucs_on_a_umc(umc: UsersMapCanisterId, opt_upgrade_ucs: Option<Vec<UserCanisterId>>, post_upgrade_arg: Vec<u8>) -> Result<Option<Vec<UMCUpgradeUCError>>, ControllerUpgradeUCSOnAUMCError> {       // /*:chunk-0 of the ucs that upgrade-fail*/ 
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CTS_DATA, |cts_data| { cts_data.users_map_canisters.contains(&umc) }) == false {
        trap(&format!("cts users_map_canisters does not contain: {:?}", umc));
    }
    
    match call::<(Option<Vec<UserCanisterId>>, Vec<u8>/*post-upgrade-arg*/), (Option<Vec<UMCUpgradeUCError>>,)>(
        umc,
        "cts_upgrade_ucs_chunk",
        (opt_upgrade_ucs, post_upgrade_arg)
    ).await {
        Ok((opt_uc_upgrade_fails,)) => Ok(opt_uc_upgrade_fails),
        Err(call_error) => Err(ControllerUpgradeUCSOnAUMCError::CTSUpgradeUCSCallError((call_error.0 as u32, call_error.1)))
    }

}






// ----- CYCLES_TRANSFERRER_CANISTERS-METHODS --------------------------


#[update]
pub fn controller_put_ctc_code(canister_code: CanisterCode) -> () {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&CTS_DATA, |cts_data| {
        cts_data.cycles_transferrer_canister_code = canister_code;
    });
}




#[export_name = "canister_query controller_see_cycles_transferrer_canisters"]
pub fn controller_see_cycles_transferrer_canisters() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with(&CTS_DATA, |cts_data| {
        ic_cdk::api::call::reply::<(&Vec<Principal>,)>((&(cts_data.cycles_transferrer_canisters),));
    });
}




#[update]
pub fn controller_put_cycles_transferrer_canisters(mut put_cycles_transferrer_canisters: Vec<Principal>) {
    
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    with_mut(&CTS_DATA, |cts_data| {
        for put_cycles_transferrer_canister in put_cycles_transferrer_canisters.iter() {
            if cts_data.cycles_transferrer_canisters.contains(put_cycles_transferrer_canister) {
                trap(&format!("{:?} already in the cycles_transferrer_canisters list", put_cycles_transferrer_canister));
            }
        }
        cts_data.cycles_transferrer_canisters.append(&mut put_cycles_transferrer_canisters);
    });
}



#[derive(CandidType, Deserialize)]
pub enum ControllerCreateNewCyclesTransferrerCanisterError {
    GetNewCanisterError(GetNewCanisterError),
    CyclesTransferrerCanisterCodeNotFound,
    InstallCodeCallError((u32, String))
}


#[update]
pub async fn controller_create_new_cycles_transferrer_canister() -> Result<Principal, ControllerCreateNewCyclesTransferrerCanisterError> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    let new_cycles_transferrer_canister_id: Principal = match get_new_canister(
        None,
        3_000_000_000_000/*7_000_000_000_000*/
    ).await {
        Ok(new_canister_id) => new_canister_id,
        Err(get_new_canister_error) => return Err(ControllerCreateNewCyclesTransferrerCanisterError::GetNewCanisterError(get_new_canister_error))
    };
    
    // on errors after here make sure to put the new_canister into the NEW_CANISTERS list
    
    // install code
    if with(&CTS_DATA, |cts_data| cts_data.cycles_transferrer_canister_code.module().len() == 0 ) {
        put_new_canister(new_cycles_transferrer_canister_id);
        return Err(ControllerCreateNewCyclesTransferrerCanisterError::CyclesTransferrerCanisterCodeNotFound);
    }
    
    match call::<(ManagementCanisterInstallCodeQuest,), ()>(
        MANAGEMENT_CANISTER_ID,
        "install_code",
        (ManagementCanisterInstallCodeQuest{
            mode : ManagementCanisterInstallCodeMode::install,
            canister_id : new_cycles_transferrer_canister_id,
            wasm_module : unsafe{&*with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canister_code.module() as *const Vec<u8> })},
            arg : &encode_one(&CyclesTransferrerCanisterInit{
                cts_id: id()
            }).unwrap() // unwrap or return Err(candiderror); 
        },)
    ).await {
        Ok(_) => {
            with_mut(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canisters.push(new_cycles_transferrer_canister_id); }); 
            Ok(new_cycles_transferrer_canister_id)    
        },
        Err(install_code_call_error) => {
            put_new_canister(new_cycles_transferrer_canister_id);
            return Err(ControllerCreateNewCyclesTransferrerCanisterError::InstallCodeCallError((install_code_call_error.0 as u32, install_code_call_error.1)));
        }
    }
      
} 



/*
#[update]
pub fn controller_take_away_cycles_transferrer_canisters(take_away_ctcs: Vec<Principal>) {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| {
        for take_away_ctc in take_away_ctcs.iter() {
            match ctcs.binary_search(take_away_ctc) {
                Ok(take_away_ctc_i) => {
                    ctcs.remove(take_away_ctc_i);
                },
                Err(_) => {
                    trap(&format!("{:?} is not one of the cycles_transferrer canisters in the CTS", take_away_ctc)); // rollback 
                }
            }
        }
    });    
}
*/


#[update]
pub async fn controller_see_cycles_transferrer_canister_re_try_cycles_transferrer_user_transfer_cycles_callbacks(cycles_transferrer_canister_id: Principal) -> Result<Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>, (u32, String)> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canisters.contains(&cycles_transferrer_canister_id) == false }) {
        trap(&format!("cts cycles_transferrer_canisters does not contain: {:?}", cycles_transferrer_canister_id));
    }
    
    match call::<(), (Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>,)>(
        cycles_transferrer_canister_id,
        "cts_see_re_try_cycles_transferrer_user_transfer_cycles_callbacks",
        ()
    ).await {
        Ok((re_try_cycles_transferrer_user_transfer_cycles_callbacks,)) => Ok(re_try_cycles_transferrer_user_transfer_cycles_callbacks),
        Err(call_error) => Err((call_error.0 as u32, call_error.1))
    }

}


#[update]
pub async fn controller_do_cycles_transferrer_canister_re_try_cycles_transferrer_user_transfer_cycles_callbacks(cycles_transferrer_canister_id: Principal) -> Result<Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>, (u32, String)> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canisters.contains(&cycles_transferrer_canister_id) == false }) {
        trap(&format!("cts cycles_transferrer_canisters does not contain: {:?}", cycles_transferrer_canister_id))
    }
    
    match call::<(), (Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>,)>(
        cycles_transferrer_canister_id,
        "cts_re_try_cycles_transferrer_user_transfer_cycles_callbacks",
        ()
    ).await {
        Ok((re_try_cycles_transferrer_user_transfer_cycles_callbacks,)) => Ok(re_try_cycles_transferrer_user_transfer_cycles_callbacks),
        Err(call_error) => Err((call_error.0 as u32, call_error.1))
    }


}


#[update]
pub async fn controller_drain_cycles_transferrer_canister_re_try_cycles_transferrer_user_transfer_cycles_callbacks(cycles_transferrer_canister_id: Principal) -> Result<Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>, (u32, String)> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canisters.contains(&cycles_transferrer_canister_id) == false }) {
        trap(&format!("cts cycles_transferrer_canisters does not contain: {:?}", cycles_transferrer_canister_id));
    }
    
    match call::<(), (Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>,)>(
        cycles_transferrer_canister_id,
        "cts_drain_re_try_cycles_transferrer_user_transfer_cycles_callbacks",
        ()
    ).await {
        Ok((re_try_cycles_transferrer_user_transfer_cycles_callbacks,)) => Ok(re_try_cycles_transferrer_user_transfer_cycles_callbacks),
        Err(call_error) => Err((call_error.0 as u32, call_error.1))
    }

}








pub type ControllerUpgradeCTCError = (Principal, ControllerUpgradeCTCCallErrorType, (u32, String)); 

#[derive(CandidType, Deserialize)]
pub enum ControllerUpgradeCTCCallErrorType {
    StopCanisterCallError,
    UpgradeCodeCallError,
    StartCanisterCallError
}



// we upgrade the ctcs one at a time because if one of them takes too long to stop, we dont want to wait for it to come back, we will stop_calls on the cycles_transferrer, wait an hour, uninstall, and reinstall
#[update]
pub async fn controller_upgrade_ctc(upgrade_ctc: Principal, post_upgrade_arg: Vec<u8>) -> Result<(), ControllerUpgradeCTCError> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }

    if with(&CTS_DATA, |cts_data| cts_data.cycles_transferrer_canister_code.module().len() == 0 ) {
        trap("CYCLES_TRANSFERRER_CANISTER_CODE.module().len() is 0.")
    }
    
    if with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canisters.contains(&upgrade_ctc) == false }) {
        trap(&format!("cts cycles_transferrer_canisters does not contain: {:?}", upgrade_ctc));
    }
       
    match call::<(CanisterIdRecord,), ()>(
        MANAGEMENT_CANISTER_ID,
        "stop_canister",
        (CanisterIdRecord{ canister_id: upgrade_ctc },)
    ).await {
        Ok(_) => {},
        Err(stop_canister_call_error) => {
                
            // set stop_calls_flag , wait an hour, then re-try the [re]maining re_try-cycles_transferrer_user_transfer_cycles_callbacks till 0 left, then uninstall the canister and install . 

            
            return Err((upgrade_ctc, ControllerUpgradeCTCCallErrorType::StopCanisterCallError, (stop_canister_call_error.0 as u32, stop_canister_call_error.1))); 
        }
    }

    match call_raw128(
        MANAGEMENT_CANISTER_ID,
        "install_code",
        &encode_one(&ManagementCanisterInstallCodeQuest{
            mode : ManagementCanisterInstallCodeMode::upgrade,
            canister_id : upgrade_ctc,
            wasm_module : unsafe{&*with(&CTS_DATA, |cts_data| { cts_data.cycles_transferrer_canister_code.module() as *const Vec<u8> })},
            arg : &post_upgrade_arg,
        }).unwrap(),
        0
    ).await {
        Ok(_) => {},
        Err(upgrade_code_call_error) => {
            return Err((upgrade_ctc, ControllerUpgradeCTCCallErrorType::UpgradeCodeCallError, (upgrade_code_call_error.0 as u32, upgrade_code_call_error.1)));
        }
    }

    match call::<(CanisterIdRecord,), ()>(
        MANAGEMENT_CANISTER_ID,
        "start_canister",
        (CanisterIdRecord{ canister_id: upgrade_ctc },)
    ).await {
        Ok(_) => {},
        Err(start_canister_call_error) => {
            return Err((upgrade_ctc, ControllerUpgradeCTCCallErrorType::StartCanisterCallError, (start_canister_call_error.0 as u32, start_canister_call_error.1))); 
        }
    }
    
    Ok(())
    
}










// ----- NEW_USERS-METHODS --------------------------

#[export_name = "canister_query controller_see_new_users"]
pub fn controller_see_new_users() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with(&CTS_DATA, |cts_data| {
        ic_cdk::api::call::reply::<(Vec<(&UserId, &NewUserData)>,)>((cts_data.new_users.iter().collect::<Vec<(&UserId, &NewUserData)>>(),));
    });
}

// put new user data
#[update]
pub fn put_new_user_data(new_user_id: UserId, put_new_user_data: NewUserData, override_lock: bool) -> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    with_mut(&CTS_DATA, |cts_data| {
        if let Some(new_user_data) = cts_data.get(&new_user_id) {
            if new_user_data.lock == true {
                if override_lock == false {
                    trap("user is with the lock == true in the new_users. set the override_lock flag if want override.")
                }
            }
        }
        cts_data.insert(new_user_id, put_new_user_data);
    });

}
// remove new user
#[update]
pub fn remove_new_user(new_user_id: UserId, override_lock: bool) {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    with_mut(&CTS_DATA, |cts_data| {
        if let Some(new_user_data) = cts_data.get(&new_user_id) {
            if new_user_data.lock == true {
                if override_lock == false {
                    trap("user is with the lock == true in the new_users. set the override_lock flag if want override.")
                }
            }
        }
        cts_data.new_users.remove(&new_user_id);
    });
}

// complete new users
#[update]
pub async fn complete_new_users(opt_complete_new_users_ids: Option<Vec<UserId>>,) -> Vec<((UserId, NewUserQuest), Result<NewUserSuccessData, NewUserError>)> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }

    let complete_new_users: Vec<(UserId, NewUserQuest)> = match opt_complete_new_users_ids {
        Some(complete_new_users_ids) => {
            with(&CTS_DATA, |cts_data| {
                complete_new_users_ids.into_iter().map( 
                    |complete_new_user_id| {
                        match cts_data.new_users.get(&complete_new_user_id) {
                            Some(new_user_data) => {
                                (complete_new_user_id, new_user_data.new_user_quest.clone())
                            },
                            None => trap(&format!("new_users.get({:?}) == None", complete_new_user_id))
                        }    
                    }
                ).collect::<Vec<(UserId, NewUserQuest)>>()  
            })
        },
        None => {
            with(&CTS_DATA, |cts_data| { 
                cts_data.new_users.iter().map(
                    |(new_user_id_ref, new_user_data_ref): (&UserId, &NewUserData)| { 
                        ((*new_user_id_ref).clone(), (*new_user_data_ref).new_user_quest.clone()) 
                    }
                ).collect::<Vec<(UserId, NewUserQuest)>>() 
            })
        }
    };
    
    let rs: Vec<Result<NewUserSuccessData, NewUserError>> = futures::future::join_all(
        complete_new_users.iter().map(
            |complete_new_user: &(UserId, NewUserQuest)| {
                new_user_(complete_new_user.0.clone(), complete_new_user.1.clone())
            }
        ).collect::<Vec<_>>()
    ).await;
    
    complete_new_users.into_iter().zip(rs.into_iter()).collect::<Vec<((UserId,NewUserQuest),Result<NewUserSuccessData,NewUserError>)>>()

}




// ------ UserBurnIcpMintCycles-METHODS -----------------




// ----- NEW_CANISTERS-METHODS --------------------------

#[update]
pub fn controller_put_new_canisters(mut put_new_canisters: Vec<Principal>) {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&NEW_CANISTERS, |new_canisters| {
        for put_new_canister in put_new_canisters.iter() {
            if new_canisters.contains(put_new_canister) {
                trap(&format!("{:?} already in the new_canisters list", put_new_canister));
            }
        }
        new_canisters.append(&mut VecDeque::from(put_new_canisters)); // .extend_from_slice(&new_canisters) also works but it clones each item. .append moves each item
    });
}

#[export_name = "canister_query controller_see_new_canisters"]
pub fn controller_see_new_canisters() -> () {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with(&NEW_CANISTERS, |ncs| {
        ic_cdk::api::call::reply::<(Vec<&Principal>,)>((ncs.iter().collect::<Vec<&Principal>>(),));
    });

}







// ----- STOP_CALLS-METHODS --------------------------

#[update]
pub fn controller_set_stop_calls_flag(stop_calls_flag: bool) {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    STOP_CALLS.with(|stop_calls| { stop_calls.set(stop_calls_flag); });
}

#[query]
pub fn controller_see_stop_calls_flag() -> bool {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    STOP_CALLS.with(|stop_calls| { stop_calls.get() })
}







// ----- STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES-METHODS --------------------------

#[update]
pub fn controller_create_state_snapshot() -> u64/*len of the state_snapshot_candid_bytes*/ {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
        *state_snapshot_cts_data_candid_bytes = create_cts_data_candid_bytes();
        state_snapshot_cts_data_candid_bytes.len() as u64
    })
}


// chunk_size = 1mib
#[export_name = "canister_query controller_download_state_snapshot"]
pub fn controller_download_state_snapshot() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    let chunk_size: usize = 1024*1024;
    with(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
        let (chunk_i,): (u32,) = arg_data::<(u32,)>(); // starts at 0
        reply::<(Option<&[u8]>,)>((state_snapshot_cts_data_candid_bytes.chunks(chunk_size).nth(chunk_i as usize),));
    });
}



#[update]
pub fn controller_clear_state_snapshot() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
        *state_snapshot_cts_data_candid_bytes = Vec::new();
    });    
}

#[update]
pub fn controller_append_state_snapshot_candid_bytes(mut append_bytes: Vec<u8>) {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
        state_snapshot_cts_data_candid_bytes.append(&mut append_bytes);
    });
}

#[update]
pub fn controller_re_store_cts_data_out_of_the_state_snapshot() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    re_store_cts_data_candid_bytes(
        with_mut(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
            let mut v: Vec<u8> = Vec::new();
            v.append(state_snapshot_cts_data_candid_bytes);  // moves the bytes out of the state_snapshot vec
            v
        })
    );

}




// ----- SET_&_SEE_CTS_CONTROLLERS-METHODS --------------------------

#[query(manual_reply = true)]
pub fn controller_see_controllers() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with(&CONTROLLERS, |controllers| { 
        reply::<(&Vec<Principal>,)>((controllers,)); 
    })
}


#[update]
pub fn controller_set_controllers(set_controllers: Vec<Principal>) {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&CONTROLLERS, |controllers| { *controllers = set_controllers; });
}









// ----- CONTROLLER_CALL_CANISTER-METHOD --------------------------

#[derive(CandidType, Deserialize)]
pub struct ControllerCallCanisterQuest {
    callee: Principal,
    method_name: String,
    arg_raw: Vec<u8>,
    cycles: Cycles
}

#[update(manual_reply = true)]
pub async fn controller_call_canister() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    let (q,): (ControllerCallCanisterQuest,) = arg_data::<(ControllerCallCanisterQuest,)>(); 
    
    match call_raw128(
        q.callee,
        &q.method_name,
        &q.arg_raw,
        q.cycles   
    ).await {
        Ok(raw_sponse) => {
            reply::<(Result<Vec<u8>, (u32, String)>,)>((Ok(raw_sponse),));
        }, 
        Err(call_error) => {
            reply::<(Result<Vec<u8>, (u32, String)>,)>((Err((call_error.0 as u32, call_error.1)),));
        }
    }
}







// ----- METRICS --------------------------

#[derive(CandidType, Deserialize)]
pub struct Metrics {
    global_allocator_counter: u64,
    stable_size: u64,
    cycles_balance: u128,
    new_canisters_count: u64,
    users_map_canister_code_hash: Option<[u8; 32]>,
    user_canister_code_hash: Option<[u8; 32]>,
    cycles_transferrer_canister_code_hash: Option<[u8; 32]>,
    users_map_canisters_count: u64,
    cycles_transferrer_canisters_count: u64,
    latest_known_cmc_rate: IcpXdrConversionRate,
    new_users_count: u64,
    cycles_transfers_count: u64,
}


#[query]
pub fn controller_see_metrics() -> Metrics {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    Metrics {
        global_allocator_counter: get_allocated_bytes_count() as u64,
        stable_size: ic_cdk::api::stable::stable64_size(),
        cycles_balance: ic_cdk::api::canister_balance128(),
        new_canisters_count: with(&NEW_CANISTERS, |ncs| ncs.len() as u64),
        users_map_canister_code_hash: with(&USERS_MAP_CANISTER_CODE, |umcc| { if umcc.module().len() != 0 { Some(*umcc.module_hash()) } else { None } }),
        user_canister_code_hash: with(&USER_CANISTER_CODE, |ucc| { if ucc.module().len() != 0 { Some(*ucc.module_hash()) } else { None } }),
        cycles_transferrer_canister_code_hash: with(&CYCLES_TRANSFERRER_CANISTER_CODE, |ctcc| { if ctcc.module().len() != 0 { Some(*ctcc.module_hash()) } else { None } }),
        users_map_canisters_count: with(&USERS_MAP_CANISTERS, |umcs| umcs.len() as u64),
        cycles_transferrer_canisters_count: with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| ctcs.len() as u64),
        latest_known_cmc_rate: LATEST_KNOWN_CMC_RATE.with(|cr| cr.get()),
        new_users_count: with(&NEW_USERS, |new_users| { new_users.len() as u64 }),
        cycles_transfers_count: get(&CYCLES_TRANSFERS_COUNT)
        
    }
}





// ---------------------------- :FRONTCODE. -----------------------------------


#[update]
pub fn controller_upload_frontcode_file_chunks(file_path: String, file: File) -> () {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    // let mut file_hashes: FileHashes = get_file_hashes();
    // file_hashes.insert(file_path.clone(), sha256(&file.content));
    // put_file_hashes(&file_hashes);
    // set_root_hash(&file_hashes);

    // let mut files: Files = get_files();
    // files.insert(file_path, file);
    // put_files(&files);
    
    with_mut(&FRONTCODE_FILES_HASHES, |ffhs| {
        ffhs.insert(file_path.clone(), sha256(&file.content));
        set_root_hash(ffhs);
    });
    
    with_mut(&FRONTCODE_FILES, |ffs| {
        ffs.insert(file_path, file); 
    });
}


#[update]
pub fn controller_clear_frontcode_files() {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    
    with_mut(&FRONTCODE_FILES, |ffs| {
        *ffs = Files::new();
    });

    with_mut(&FRONTCODE_FILES_HASHES, |ffhs| {
        *ffhs = FilesHashes::default();
        set_root_hash(ffhs);
    });
}


#[query]
pub fn controller_get_file_hashes() -> Vec<(String, [u8; 32])> {
    if with(&CTS_DATA, |cts_data| { cts_data.controllers.contains(&caller()) }) == false {
        trap("Caller must be a controller for this method.")
    }
    
    with(&FRONTCODE_FILES_HASHES, |file_hashes| { 
        let mut vec = Vec::<(String, [u8; 32])>::new();
        file_hashes.for_each(|k,v| {
            vec.push((std::str::from_utf8(k).unwrap().to_string(), *v));
        });
        vec
    })
}



#[query]
pub fn http_request(quest: HttpRequest) -> HttpResponse {
    if STOP_CALLS.with(|stop_calls| { stop_calls.get() }) { trap("Maintenance. try again soon.") }
    
    let file_name: String = quest.url;
    
    with(&FRONTCODE_FILES, |ffs| {
        match ffs.get(&file_name) {
            None => {
                return HttpResponse {
                    status_code: 404,
                    headers: vec![],
                    body: vec![],
                    streaming_strategy: None
                }
            }, 
            Some(file) => {                 
                HttpResponse {
                    status_code: 200,
                    headers: vec![
                        make_file_certificate_header(&file_name), 
                        ("content-type".to_string(), file.content_type.clone()),
                        ("content-encoding".to_string(), file.content_encoding.clone())
                    ],
                    body: file.content.to_vec(),
                    streaming_strategy: None
                }
            }
        }
    })
}













 // ---- FOR THE TESTS --------------

#[query]
pub fn see_caller() -> Principal {
    caller()
} 







