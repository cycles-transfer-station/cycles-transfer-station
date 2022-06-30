
// lock each user from making other calls on each async call that awaits, like the collect_balance call, lock the user at the begining and unlock the user at the end. or better can take the funds within the first [exe]cution and if want can give back
// will callbacks (the code after an await) get dropped if the subnet is under heavy load?
// when calling canisters that i dont know if they can possible give-back unexpected candid, use call_raw and dont panic on the candid-decode, return an error.
// dont want to implement From<(RejectionCode, String)> for the return errors in the calls async that call other canisters because if the function makes more than one call then the ? with the from can give-back a wrong error type 
// always check user lock before any awaits (or maybe after the first await if not fective?). 
// in the cycles-market, let a seller set a minimum-purchase-quantity. which can be the full-mount that is up for the sale or less 
// always unlock the user af-ter the last await-call()
// does dereferencing a borrow give the ownership? try on a non-copy type. error[E0507]: cannot move out of `*cycles_transfer_purchase_log` which is behind a mutable reference
// sending cycles to a canister is the same risk as sending icp to a canister. 
// put a max_fee on a cycles-transfer-purchase & on a cycles-bank-purchase?
// 5xdr first-time-user-fee, valid for one year. with 100mbs of storage for the year and standard-call-rate-limits. after the year, if the user doesn't pay for more space, the user-storage gets deleted and the user cycles balance and icp balance stays for another 3 years.
// if a user.user_canister == None: means the user must pay for some storage minimum 5xdr see^, if it wants to do something
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
            UMCUserTransferCyclesQuest,
            UMCUserTransferCyclesError,
            CyclesTransferrerUserTransferCyclesCallbackQuest
        },
        users_map_canister::{
            UMCUserData,
            UMCUpgradeUCError,
            UMCUpgradeUCCallErrorType
        },
        user_canister::{
            UserCanisterInit,
            CTSCyclesTransferIntoUser,
            CTSUserTransferCyclesCallbackQuest,
            CTSUserTransferCyclesCallbackError
        },
        cycles_transferrer::{
            CyclesTransferRefund,
            CTSUserTransferCyclesQuest,
            CTSUserTransferCyclesError,
            ReTryCyclesTransferrerUserTransferCyclesCallback
        },
    },
    consts::{
        MANAGEMENT_CANISTER_ID,
        WASM_PAGE_SIZE_BYTES
    },
    fees::{
        CYCLES_BANK_COST,
        CYCLES_BANK_UPGRADE_COST,
        CYCLES_TRANSFER_FEE,
        CONVERT_ICP_FOR_THE_CYCLES_WITH_THE_CMC_RATE_FEE,
        
        
        
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
            call::{
                arg_data,
                arg_data_raw,
                arg_data_raw_size,
                call_raw128,
                CallRawFuture,
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
    USER_CYCLES_BALANCE_TOPUP_MEMO_START,
    ledger_topup_cycles,
    LedgerTopupCyclesError,
    IcpXdrConversionRate,
    take_user_icp_ledger,
    ICP_LEDGER_CREATE_CANISTER_MEMO,
    CmcNotifyError,
    CmcNotifyCreateCanisterQuest,
    PutNewUserIntoAUsersMapCanisterError,
    put_new_user_into_a_users_map_canister,
    FindUserInTheUsersMapCanistersError,
    find_user_in_the_users_map_canisters
    
};


mod frontcode;
use frontcode::{File, Files, FilesHashes, HttpRequest, HttpResponse, set_root_hash, make_file_certificate_header};




pub type ReTryCTSUserTransferCyclesCallback = (ReTryCTSUserTransferCyclesCallbackErrorKind/*the error of the last try*/, CTSUserTransferCyclesCallbackQuest, UserCanisterId);

#[derive(CandidType, Deserialize, Clone)]
pub enum ReTryCTSUserTransferCyclesCallbackErrorKind {
    CTSUserTransferCyclesCallbackError(CTSUserTransferCyclesCallbackError),
    CTSUserTransferCyclesCallbackCallError((u32, String))
}








pub const MINIMUM_CYCLES_TRANSFER_INTO_USER: Cycles = 50_000_000_000; // enough to pay for a find_and_lock_user-call.
pub const CYCLES_TRANSFER_INTO_USER_USER_NOT_FOUND_FEE: Cycles = (100_000 + 260_000 + 590_000 + 1_000_000_000); // * with(&USERS_MAP_CANISTERS, |umcs| umcs.len() as u128); // :do: clude wasm-instructions-counts 1000000000 placeholder
pub const CYCLES_PER_USER_PER_103_MiB_PER_YEAR: Cycles = /*TEST-VALUE*/1_000_000_000_000; //5_000_000_000_000;
pub const CYCLES_FOR_A_USER_CANISTER_PER_103_MiB_PER_YEAR_STANDARD_CALL_RATE: Cycles = /*TEST-VALUE*/1_000_000_000_000; //3_000_000_000_000; // MAKE SURE THIS IS < CYCLES_PER_USER_PER_103_MiB_PER_YEAR


pub const MAX_NEW_USERS: usize = 5000; // the max number of entries in the NEW_USERS-hashmap at the same-time
pub const MAX_USERS_MAP_CANISTERS: usize = 4; // can be 30-million at 1-gb, or 3-million at 0.1-gb,
pub const MAX_RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS: usize = 100;


const STABLE_MEMORY_HEADER_SIZE_BYTES: u64 = 1024;


thread_local! {

    static     NEW_USERS: RefCell<HashMap<Principal, NewUserData>> = RefCell::new(HashMap::new());
    pub static USERS_MAP_CANISTERS: RefCell<Vec<Principal>> = RefCell::new(Vec::new());
    pub static CREATE_NEW_USERS_MAP_CANISTER_LOCK: Cell<bool> = Cell::new(false);
    pub static LATEST_KNOWN_CMC_RATE: Cell<IcpXdrConversionRate> = Cell::new(IcpXdrConversionRate{ xdr_permyriad_per_icp: 0, timestamp_seconds: 0 });
    
    pub static USER_CANISTER_CODE           : RefCell<CanisterCode> = RefCell::new(CanisterCode::new(Vec::new()));
    pub static USERS_MAP_CANISTER_CODE      : RefCell<CanisterCode> = RefCell::new(CanisterCode::new(Vec::new()));
    pub static CYCLES_TRANSFERRER_CANISTER_CODE: RefCell<CanisterCode> = RefCell::new(CanisterCode::new(Vec::new()));
    
    pub static NEW_CANISTERS: RefCell<VecDeque<Principal>> = RefCell::new(VecDeque::new());
    
    static     CYCLES_TRANSFERRER_CANISTERS : RefCell<Vec<Principal>> = RefCell::new(Vec::new());
    static     CYCLES_TRANSFERRER_CANISTERS_ROUND_ROBIN_COUNTER: Cell<usize> = Cell::new(0);
    static     RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS/*_LOGS*/: RefCell<Vec<ReTryCTSUserTransferCyclesCallback>> = RefCell::new(Vec::new());

    static     FRONTCODE_FILES:        RefCell<Files>       = RefCell::new(Files::new());
    static     FRONTCODE_FILES_HASHES: RefCell<FilesHashes> = RefCell::new(FilesHashes::default());

    static     CONTROLLERS: RefCell<Vec<Principal>> = RefCell::new(Vec::new());
    
    
    
    // not save in a CTSData
    static     STOP_CALLS: Cell<bool> = Cell::new(false);
    static     STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}






#[derive(CandidType, Deserialize)]
struct CTSInit {
    controllers: Vec<Principal>
} 

#[init]
fn init(cts_init: CTSInit) {
    with_mut(&CONTROLLERS, |controllers| { *controllers = cts_init.controllers; });
} 

#[derive(CandidType, Deserialize)]
struct CTSData {
    new_users: Vec<(Principal, NewUserData)>,
    users_map_canisters: Vec<Principal>,
    create_new_users_map_canister_lock: bool,
    latest_known_cmc_rate: IcpXdrConversionRate,
    user_canister_code: CanisterCode,
    users_map_canister_code: CanisterCode,
    cycles_transferrer_canister_code: CanisterCode,
    new_canisters: Vec<Principal>,
    cycles_transferrer_canisters: Vec<Principal>,
    cycles_transferrer_canisters_round_robin_counter: usize,
    re_try_cts_user_transfer_cycles_callbacks: Vec<ReTryCTSUserTransferCyclesCallback>,
    frontcode_files: Vec<(String, File)>,
    frontcode_files_hashes: Vec<(String, [u8; 32])>,
    controllers: Vec<Principal>
}



fn create_cts_data_candid_bytes() -> Vec<u8> {
    let mut cts_data_candid_bytes: Vec<u8> = encode_one(
        &CTSData {
            new_users: with(&NEW_USERS, |new_users| { (*new_users).clone().into_iter().collect::<Vec<(Principal, NewUserData)>>() }),  //Vec<(Principal, NewUserData)>,
            users_map_canisters: with(&USERS_MAP_CANISTERS, |users_map_canisters| { (*users_map_canisters).clone() }), // Vec<Principal>,
            create_new_users_map_canister_lock: CREATE_NEW_USERS_MAP_CANISTER_LOCK.with(|create_new_users_map_canister_lock| { create_new_users_map_canister_lock.get() }), //bool, // the cts main canister can stop safe before upgrading. this value should always be false when upgrading for now becouse the canister stops and finishes ongoing callbacks before upgrading. leaving this here for when there is name-callbacks.
            latest_known_cmc_rate: LATEST_KNOWN_CMC_RATE.with(|latest_known_cmc_rate| { latest_known_cmc_rate.get() }),    // IcpXdrConversionRate,
            user_canister_code: with(&USER_CANISTER_CODE, |user_canister_code| { (*user_canister_code).clone() }),
            users_map_canister_code: with(&USERS_MAP_CANISTER_CODE, |users_map_canister_code| { (*users_map_canister_code).clone() }),
            cycles_transferrer_canister_code: with(&CYCLES_TRANSFERRER_CANISTER_CODE, |cycles_transferrer_canister_code| { (*cycles_transferrer_canister_code).clone() }),
            new_canisters: with(&NEW_CANISTERS, |new_canisters| { Vec::from((*new_canisters).clone()) }), // Vec<Principal>,
            cycles_transferrer_canisters: with(&CYCLES_TRANSFERRER_CANISTERS, |cycles_transferrer_canisters| { (*cycles_transferrer_canisters).clone() }), // Vec<Principal>,
            cycles_transferrer_canisters_round_robin_counter: CYCLES_TRANSFERRER_CANISTERS_ROUND_ROBIN_COUNTER.with(|cycles_transferrer_canisters_round_robin_counter| { cycles_transferrer_canisters_round_robin_counter.get() }), // usize,
            re_try_cts_user_transfer_cycles_callbacks: with(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |re_try_cts_user_transfer_cycles_callbacks| { (*re_try_cts_user_transfer_cycles_callbacks).clone() }), // Vec<ReTryCTSUserTransferCyclesCallback>,
            frontcode_files: with(&FRONTCODE_FILES, |frontcode_files| { (*frontcode_files).clone().into_iter().collect::<Vec<(String, File)>>() }), //Vec<(String, File)>,
            frontcode_files_hashes: with(&FRONTCODE_FILES_HASHES, |frontcode_files_hashes| { frontcode_files_hashes.iter().map(|(k, v): (&String, &[u8; 32])| { (k.clone(), *v/*copy*/) }).collect::<Vec<(String, [u8; 32])>>() }), //Vec<(String, [u8; 32])>, 
            controllers: with(&CONTROLLERS, |controllers| { (*controllers).clone() }), // Vec<Principal>      
        }
    ).unwrap();
    cts_data_candid_bytes.shrink_to_fit();
    cts_data_candid_bytes
}

fn re_store_cts_data_candid_bytes(cts_data_candid_bytes: Vec<u8>) {
    let cts_upgrade_data: CTSData = decode_one::<CTSData>(&cts_data_candid_bytes).unwrap();
    // std::mem::drop(cts_data_candid_bytes);
    with_mut(&NEW_USERS, |new_users| { *new_users = cts_upgrade_data.new_users.into_iter().collect::<HashMap<Principal, NewUserData>>(); });
    with_mut(&USERS_MAP_CANISTERS, |users_map_canisters| { *users_map_canisters = cts_upgrade_data.users_map_canisters; });
    CREATE_NEW_USERS_MAP_CANISTER_LOCK.with(|create_new_users_map_canister_lock| { create_new_users_map_canister_lock.set(cts_upgrade_data.create_new_users_map_canister_lock); });
    LATEST_KNOWN_CMC_RATE.with(|latest_known_cmc_rate| { latest_known_cmc_rate.set(cts_upgrade_data.latest_known_cmc_rate); });
    with_mut(&USER_CANISTER_CODE, |user_canister_code| { *user_canister_code = cts_upgrade_data.user_canister_code; });
    with_mut(&USERS_MAP_CANISTER_CODE, |users_map_canister_code| { *users_map_canister_code = cts_upgrade_data.users_map_canister_code; });
    with_mut(&CYCLES_TRANSFERRER_CANISTER_CODE, |cycles_transferrer_canister_code| { *cycles_transferrer_canister_code = cts_upgrade_data.cycles_transferrer_canister_code; });
    with_mut(&NEW_CANISTERS, |new_canisters| { *new_canisters = VecDeque::from(cts_upgrade_data.new_canisters); });
    with_mut(&CYCLES_TRANSFERRER_CANISTERS, |cycles_transferrer_canisters| { *cycles_transferrer_canisters = cts_upgrade_data.cycles_transferrer_canisters; });
    CYCLES_TRANSFERRER_CANISTERS_ROUND_ROBIN_COUNTER.with(|cycles_transferrer_canisters_round_robin_counter| { cycles_transferrer_canisters_round_robin_counter.set(cts_upgrade_data.cycles_transferrer_canisters_round_robin_counter); });
    with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |re_try_cts_user_transfer_cycles_callbacks| { *re_try_cts_user_transfer_cycles_callbacks = cts_upgrade_data.re_try_cts_user_transfer_cycles_callbacks; });
    with_mut(&FRONTCODE_FILES, |frontcode_files| { *frontcode_files = cts_upgrade_data.frontcode_files.into_iter().collect::<HashMap<String, File>>(); });
    with_mut(&FRONTCODE_FILES_HASHES, |frontcode_files_hashes| {
        cts_upgrade_data.frontcode_files_hashes.into_iter().for_each(|file_hash_pair| {
            frontcode_files_hashes.insert(file_hash_pair.0, file_hash_pair.1);    
        });
        set_root_hash(frontcode_files_hashes);
    });
    with_mut(&CONTROLLERS, |controllers| { *controllers = cts_upgrade_data.controllers; });
    
    
}


#[pre_upgrade]
fn pre_upgrade() {
    
    let cts_upgrade_data_candid_bytes: Vec<u8> = create_cts_data_candid_bytes();
    
    let current_stable_size_wasm_pages: u64 = stable64_size();
    let current_stable_size_bytes: u64 = current_stable_size_wasm_pages * WASM_PAGE_SIZE_BYTES;
    
    let want_stable_memory_size_bytes: u64 = STABLE_MEMORY_HEADER_SIZE_BYTES + 8/*len of the cts_upgrade_data_candid_bytes*/ + cts_upgrade_data_candid_bytes.len() as u64; 
    if current_stable_size_bytes < want_stable_memory_size_bytes {
        stable64_grow(((want_stable_memory_size_bytes - current_stable_size_bytes) / WASM_PAGE_SIZE_BYTES) + 1).unwrap();
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
        trap("caller cannot be anonymous for this method.")
    }
    
    // check the size of the arg_data_raw_size()

    if &method_name()[..] == "cycles_transfer" {
        trap("caller must be a canister for this method.")
    }
    
    if method_name()[..].starts_with("controller") {
        if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
            trap("Caller must be a controller for this method.")
        }
    }

    accept_message();
}







// ----------------------------------------------------------------------------------------







// if a user for the topup is not found, the cycles-transfer-station takes a fee for the user-lookup(:fee is with the base on how many users_map_canisters there are) and refunds the rest of the cycles. 
// make sure the minimum-in-cycles-transfer is more than the find_and_plus_user_cycles_balance_user_not_found_fee

#[update(manual_reply = true)]
pub async fn cycles_transfer() {
    if STOP_CALLS.with(|stop_calls| { stop_calls.get() }) { trap("Maintenance. try again soon.") }
    
    let cycles_available: Cycles = msg_cycles_available128();
    
    if cycles_available < MINIMUM_CYCLES_TRANSFER_INTO_USER {
        trap(&format!("minimum cycles transfer into a user: {}", MINIMUM_CYCLES_TRANSFER_INTO_USER))
    }

    if arg_data_raw_size() > 100 {
        trap("arg_data_raw_size can be max 100 bytes")
    }
    
    let (ct,): (CyclesTransfer,) = arg_data::<(CyclesTransfer,)>();

    let user_id: UserId = match ct.memo {
        CyclesTransferMemo::Blob(memo_bytes) => {
            if memo_bytes.len() != 32 || &memo_bytes[..2] != USER_CYCLES_BALANCE_TOPUP_MEMO_START {
                trap("unknown cycles transfer memo")
            }
            thirty_bytes_as_principal(&memo_bytes[2..32].try_into().unwrap())
        },
        _ => trap("CyclesTransferMemo must be the Blob variant")
    };
    
    let original_caller: Principal = caller(); // before the first await
    let timestamp_nanos: u64 = time(); // before the first await

    let user_canister_id: UserCanisterId = match find_user_in_the_users_map_canisters(user_id).await {
        Ok((umc_user_data, users_map_canister_id)) => umc_user_data.user_canister_id,
        Err(find_user_in_the_users_map_canisters_error) => match find_user_in_the_users_map_canisters_error {
            FindUserInTheUsersMapCanistersError::UserNotFound => {
                msg_cycles_accept128(CYCLES_TRANSFER_INTO_USER_USER_NOT_FOUND_FEE); // test that the cycles are taken on the reject.
                reject(&format!("User for the top up not found. {} cycles taken for a cycles_transfer-into-a-nonexistentuser-fee", CYCLES_TRANSFER_INTO_USER_USER_NOT_FOUND_FEE));
                return;
            },
            FindUserInTheUsersMapCanistersError::UsersMapCanistersFindUserCallFails(umc_call_errors) => {
                reject(&format!("User lookup error. umc_call_errors: {:?}", umc_call_errors)); // reject not trap because we are after an await here
                return;
            }
        }
    };
    
    // take a fee for the cycles_transfer_into_user? 
    
    match call::<(CTSCyclesTransferIntoUser,), ()>(
        user_canister_id,
        "cts_cycles_transfer_into_user",
        (CTSCyclesTransferIntoUser{ 
            canister: original_caller,
            cycles: cycles_available,
            timestamp_nanos: timestamp_nanos
        },),
    ).await {
        Ok(()) => {
            msg_cycles_accept128(cycles_available);
            reply::<()>(());
            return;
        },
        Err(call_error) => {
            reject(&format!("user-canister call-error. user_canister: {}, call-error: {:?}", user_canister_id, call_error)); // reject not trap becouse after an await
            return;
        }
    }
            
}










#[derive(CandidType, Deserialize)]
pub struct Fees {
    purchase_cycles_bank_cost_cycles: Cycles,
    purchase_cycles_bank_upgrade_cost_cycles: Cycles,
    purchase_cycles_transfer_cost_cycles: Cycles,
    convert_icp_for_the_cycles_with_the_cmc_rate_cost_cycles: Cycles,
    minimum_cycles_transfer_into_user: Cycles,
    cycles_transfer_into_user_user_not_found_fee_cycles: Cycles,
    cycles_per_user_per_103_mib_per_year: Cycles,
    
    
}

#[query]
pub fn see_fees() -> Fees {
    Fees {
        purchase_cycles_bank_cost_cycles: CYCLES_BANK_COST,
        purchase_cycles_bank_upgrade_cost_cycles: CYCLES_BANK_UPGRADE_COST,
        purchase_cycles_transfer_cost_cycles: CYCLES_TRANSFER_FEE,
        convert_icp_for_the_cycles_with_the_cmc_rate_cost_cycles: CONVERT_ICP_FOR_THE_CYCLES_WITH_THE_CMC_RATE_FEE,
        minimum_cycles_transfer_into_user: MINIMUM_CYCLES_TRANSFER_INTO_USER,
        cycles_transfer_into_user_user_not_found_fee_cycles: CYCLES_TRANSFER_INTO_USER_USER_NOT_FOUND_FEE,
        cycles_per_user_per_103_mib_per_year: CYCLES_PER_USER_PER_103_MiB_PER_YEAR
    }
}








#[derive(CandidType, Deserialize)]
pub struct TopUpCyclesBalanceData {
    topup_cycles_transfer_memo: CyclesTransferMemo,
}

#[derive(CandidType, Deserialize)]
pub struct TopUpIcpBalanceData {
    topup_icp_id: IcpId
}

#[derive(CandidType, Deserialize)]
pub struct TopUpBalanceData {
    topup_cycles_balance: TopUpCyclesBalanceData, 
    topup_icp_balance: TopUpIcpBalanceData,
}


#[query]
pub fn topup_balance() -> TopUpBalanceData {
    let user_id: Principal = caller();
    TopUpBalanceData {
        topup_cycles_balance: TopUpCyclesBalanceData {
            topup_cycles_transfer_memo: CyclesTransferMemo::Blob(user_cycles_balance_topup_memo_bytes(&user_id).to_vec())
        },
        topup_icp_balance: TopUpIcpBalanceData {
            topup_icp_id: cts_lib::tools::user_icp_id(&id(), &user_id)
        }
    }
}








// save the fees in the new_user_data so the fees cant change while creating a new user

#[derive(Clone, Default, CandidType, Deserialize)]
struct NewUserData {
    lock_start_time_nanos: u64,
    lock: bool,
    
    current_xdr_icp_rate: u64,
    
    // the options and bools are for the memberance of the steps
    look_if_user_is_in_the_users_map_canisters: bool,
    create_user_canister_block_height: Option<IcpBlockHeight>,
    user_canister: Option<UserId>,
    user_canister_uninstall_code: bool,
    user_canister_install_code: bool,
    user_canister_status_record: Option<ManagementCanisterCanisterStatusRecord>,
    users_map_canister: Option<UsersMapCanisterId>,    
    collect_icp: bool,
    
    

}

impl NewUserData {
    pub fn new() -> Self {
        Self {
            lock_start_time_nanos: time(),
            lock: true,
            ..Default::default()
        }
    }
}



#[derive(CandidType, Deserialize)]
pub enum NewUserMidCallError{
    UsersMapCanistersFindUserCallFails(Vec<(UsersMapCanisterId, (u32, String))>),
    PutNewUserIntoAUsersMapCanisterError(PutNewUserIntoAUsersMapCanisterError),
    CreateUserCanisterIcpTransferError(IcpTransferError),
    CreateUserCanisterIcpTransferCallError(String),
    CreateUserCanisterCmcNotifyError(CmcNotifyError),
    CreateUserCanisterCmcNotifyCallError(String),
    IcpTransferCallError(String),
    IcpTransferError(IcpTransferError),
    UserCanisterUninstallCodeCallError(String),
    UserCanisterCodeNotFound,
    UserCanisterInstallCodeCallError(String),
    UserCanisterStatusCallError(String),
    UserCanisterModuleVerificationError,
    UserCanisterStartCanisterCallError(String),
    UserCanisterUpdateSettingsCallError(String),
}


#[derive(CandidType, Deserialize)]
pub enum NewUserError{
    CheckIcpBalanceCallError(String),
    CheckCurrentXdrPerMyriadPerIcpCmcRateError(CheckCurrentXdrPerMyriadPerIcpCmcRateError),
    UserIcpLedgerBalanceTooLow{
        membership_cost_icp: IcpTokens,
        user_icp_ledger_balance: IcpTokens,
        icp_ledger_transfer_fee: IcpTokens
    },
    NewUserIsInTheMiddleOfAnotherNewUserCall, // in the frontcode on this error, wait 5-10 seconds and call again. if it gives back the FoundUserCanister(UserCanisterId) error, then log the user_canister and the new-user-setup is complete.
    MaxNewUsers,
    FoundUserCanister(UserCanisterId),
    CreateUserCanisterCmcNotifyError(CmcNotifyError),
    MidCallError(NewUserMidCallError),    // re-try the call on this sponse
}

#[derive(CandidType, Deserialize)]
pub struct NewUserSuccessData {
    user_canister: UserCanisterId,
}


fn write_new_user_data(user_id: &Principal, new_user_data: NewUserData) {
    with_mut(&NEW_USERS, |new_users| {
        match new_users.get_mut(user_id) {
            Some(nud) => { *nud = new_user_data; },
            None => {}
        }
    });
    
}

// for the now a user must sign-up/register with the icp.
#[update]
pub async fn new_user() -> Result<NewUserSuccessData, NewUserError> {

    let user_id: Principal = caller();
    
    let optional_new_user_data: Option<NewUserData> = {
        let r: Result<Option<NewUserData>, NewUserError> = with_mut(&NEW_USERS, |new_users| {
            match new_users.get_mut(&user_id) {
                Some(nud) => {
                    if nud.lock == true {
                        //trap("new user is in the middle of another call")
                        return Err(NewUserError::NewUserIsInTheMiddleOfAnotherNewUserCall);
                    }
                    nud.lock = true;
                    // update nud.lock_time_start_nanos?
                    Ok(Some(nud.clone()))
                },
                None => {
                    if STOP_CALLS.with(|stop_calls| { stop_calls.get() }) { trap("Maintenance. try again soon.") }
                    Ok(None)
                }
            }
        });
        
        r?
        /*
        match r {
            Ok(opt_new_user_data) => opt_new_user_data,
            Err(e) => return Err(e)
        }
        */
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
                    with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                    return Err(NewUserError::CheckIcpBalanceCallError(format!("{:?}", check_balance_call_error)));
                }
            };
                    
            let current_xdr_icp_rate: u64 = match check_current_xdr_permyriad_per_icp_cmc_rate_sponse {
                Ok(rate) => rate,
                Err(check_xdr_icp_rate_error) => {
                    with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                    return Err(NewUserError::CheckCurrentXdrPerMyriadPerIcpCmcRateError(check_xdr_icp_rate_error));
                }
            };
            
            let current_membership_cost_icp: IcpTokens = cycles_to_icptokens(CYCLES_PER_USER_PER_103_MiB_PER_YEAR, current_xdr_icp_rate); 
            
            if user_icp_ledger_balance < current_membership_cost_icp + IcpTokens::from_e8s(ICP_LEDGER_TRANSFER_DEFAULT_FEE.e8s() * 2) {
                with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                return Err(NewUserError::UserIcpLedgerBalanceTooLow{
                    membership_cost_icp: current_membership_cost_icp,
                    user_icp_ledger_balance,
                    icp_ledger_transfer_fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE
                });
            }

            let r: Result<NewUserData, NewUserError> = with_mut(&NEW_USERS, |new_users| {
                match new_users.get_mut(&user_id) {
                    Some(nud) => { // checking again here if Some bc this is within a different [exe]cution
                        if nud.lock == true {
                            //trap("new user is in the middle of another call")
                            return Err(NewUserError::NewUserIsInTheMiddleOfAnotherNewUserCall);
                        }
                        nud.lock = true;
                        // update nud.lock_time_start_nanos?
                        Ok(nud.clone())
                    },
                    None => {
                        if new_users.len() >= MAX_NEW_USERS {
                            //trap("max limit of creating new users at the same-time. try your call in a couple of seconds.")
                            return Err(NewUserError::MaxNewUsers);
                        }
                        let mut nud: NewUserData = NewUserData::new();
                        nud.current_xdr_icp_rate = current_xdr_icp_rate;
                        new_users.insert(user_id, nud.clone());
                        Ok(nud)
                    }
                }
            });
            
            // or can use the '?'9 operator on the r
            match r {
                Ok(nud) => nud,
                Err(new_user_error) => return Err(new_user_error)
            }
            
        },
        
        Some(nud) => nud        
        
    };

    
    if new_user_data.look_if_user_is_in_the_users_map_canisters == false {
        // check in the list of the users-whos cycles-balance is save but without a user-canister 
        
        match find_user_in_the_users_map_canisters(user_id).await {
            Ok((umc_user_data, users_map_canister_id)) => {
                // take a fee for this?
                with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
                return Err(NewUserError::FoundUserCanister(umc_user_data.user_canister_id));
            },
            Err(find_user_error) => match find_user_error {
                FindUserInTheUsersMapCanistersError::UserNotFound => {
                    new_user_data.look_if_user_is_in_the_users_map_canisters = true;                    
                },
                FindUserInTheUsersMapCanistersError::UsersMapCanistersFindUserCallFails(umc_call_errors) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::UsersMapCanistersFindUserCallFails(umc_call_errors)));
                }
            }
        };
        
    }
    

    if new_user_data.create_user_canister_block_height.is_none() {
        let create_user_canister_block_height: IcpBlockHeight = match icp_transfer(
            MAINNET_LEDGER_CANISTER_ID,
            IcpTransferArgs {
                memo: ICP_LEDGER_CREATE_CANISTER_MEMO,
                amount: cycles_to_icptokens(CYCLES_FOR_A_USER_CANISTER_PER_103_MiB_PER_YEAR_STANDARD_CALL_RATE, new_user_data.current_xdr_icp_rate),
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
                // match on the transfer_call_error?
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::CreateUserCanisterIcpTransferCallError(format!("{:?}", transfer_call_error))));
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
                // match on the call errors?
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::CreateUserCanisterCmcNotifyCallError(format!("{:?}", cmc_notify_call_error))));
            }      
        };
        
        new_user_data.user_canister = Some(user_canister);
        new_user_data.user_canister_uninstall_code = true; // because a fresh cmc canister is empty 
    }
        
        
    if new_user_data.users_map_canister.is_none() {
        
        let users_map_canister_id: UsersMapCanisterId = match put_new_user_into_a_users_map_canister(
            user_id, 
            UMCUserData{
                user_canister_id: *new_user_data.user_canister.as_ref().unwrap(),
                user_canister_latest_known_module_hash: [0u8; 32] // 0s cause we are putting the user_canister_id onto the users_map_canister before install_code on the user_canister, cause we install_code with the umc_id in the user-canister-init-arg. we can update the umc_user_data on the umc after we install the code, but for now we will let it get upgraded
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
    
    
    if new_user_data.collect_icp == false {
        match take_user_icp_ledger(&user_id, cycles_to_icptokens(CYCLES_PER_USER_PER_103_MiB_PER_YEAR - CYCLES_FOR_A_USER_CANISTER_PER_103_MiB_PER_YEAR_STANDARD_CALL_RATE, new_user_data.current_xdr_icp_rate)).await {
            Ok(icp_transfer_result) => match icp_transfer_result {
                Ok(_block_height) => {
                    new_user_data.collect_icp = true;
                },
                Err(icp_transfer_error) => {
                    new_user_data.lock = false;
                    write_new_user_data(&user_id, new_user_data);
                    return Err(NewUserError::MidCallError(NewUserMidCallError::IcpTransferError(icp_transfer_error)));          
                }
            }, 
            Err(icp_transfer_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::IcpTransferCallError(format!("{:?}", icp_transfer_call_error))));          
            }               
        }
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
    
        if with(&USER_CANISTER_CODE, |ucc| { ucc.module().len() == 0 }) {
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
                wasm_module : unsafe{&*with(&USER_CANISTER_CODE, |uc_code| { uc_code.module() as *const Vec<u8> })},
                arg : &encode_one(&UserCanisterInit{ 
                    cts_id: id(), 
                    user_id: user_id,
                    umc_id: new_user_data.users_map_canister.unwrap()
                }).unwrap() 
            },),
        ).await {
            Ok(()) => {},
            Err(put_code_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterInstallCodeCallError(format!("{:?}", put_code_call_error))));
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
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterStatusCallError(format!("{:?}", canister_status_call_error))));
            }
        };
        
        new_user_data.user_canister_status_record = Some(canister_status_record);
    }
        
    // no async in this if-block so no NewUserData field needed. can make it for the optimization though
    if with(&USER_CANISTER_CODE, |ucc| { ucc.module().len() == 0 }) {
        new_user_data.lock = false;
        write_new_user_data(&user_id, new_user_data);
        return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterCodeNotFound));
    }
    if new_user_data.user_canister_status_record.as_ref().unwrap().module_hash.is_none() || *(new_user_data.user_canister_status_record.as_ref().unwrap().module_hash.as_ref().unwrap()) != with(&USER_CANISTER_CODE, |ucc| { *(ucc.module_hash()) }) {
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
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterStartCanisterCallError(format!("{:?}", start_canister_call_error))));
            }
        }
        
    }

    //update the controller to clude the users_map_canister
    if new_user_data.user_canister_status_record.as_ref().unwrap().settings.controllers.contains(new_user_data.users_map_canister.as_ref().unwrap()) == false {
        
        let user_canister_controllers: Vec<Principal> = vec![
            id(), 
            *new_user_data.users_map_canister.as_ref().unwrap()
        ];
        
        match call::<(ChangeCanisterSettingsRecord,), ()>(
            MANAGEMENT_CANISTER_ID,
            "update_settings",
            (ChangeCanisterSettingsRecord{
                canister_id: *new_user_data.user_canister.as_ref().unwrap(),
                settings: ManagementCanisterOptionalCanisterSettings{
                    controllers : Some(user_canister_controllers.clone()),
                    compute_allocation : None,
                    memory_allocation : None,
                    freezing_threshold : None,
                }
            },)
        ).await {
            Ok(()) => {
                new_user_data.user_canister_status_record.as_mut().unwrap().settings.controllers = user_canister_controllers;
            },
            Err(update_settings_call_error) => {
                new_user_data.lock = false;
                write_new_user_data(&user_id, new_user_data);
                return Err(NewUserError::MidCallError(NewUserMidCallError::UserCanisterUpdateSettingsCallError(format!("{:?}", update_settings_call_error))));
            }
        }
    }
    


    with_mut(&NEW_USERS, |nus| { nus.remove(&user_id); });
    
    Ok(NewUserSuccessData {
        user_canister: new_user_data.user_canister.unwrap()
    })
}
















#[derive(CandidType, Deserialize)]
pub enum FindUserCanisterError {
    UserIsInTheNewUsersMap, // in the frontcode on this error, make a call to finish the new_user steps
    FindUserInTheUsersMapCanistersError(FindUserInTheUsersMapCanistersError),
}

#[update]
pub async fn find_user_canister() -> Result<UserCanisterId, FindUserCanisterError> {
    if STOP_CALLS.with(|stop_calls| { stop_calls.get() }) { trap("Maintenance. try again soon.") }
    
    let user_id: UserId = caller();
    
    if with(&NEW_USERS, |new_users| { new_users.contains_key(&user_id) }) {
        return Err(FindUserCanisterError::UserIsInTheNewUsersMap);
    }
    
    match find_user_in_the_users_map_canisters(user_id).await {
        Ok((umc_user_data, _users_map_canister_id)) => Ok(umc_user_data.user_canister_id),
        Err(e) => Err(FindUserCanisterError::FindUserInTheUsersMapCanistersError(e))
    }
    
}















// round-robin on the cycles-transferrer-canisters
fn get_next_cycles_transferrer_canister_round_robin() -> Option<Principal> {
    with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| { 
        match ctcs.len() {
            0 => None,
            1 => Some(ctcs[0]),
            l => {
                CYCLES_TRANSFERRER_CANISTERS_ROUND_ROBIN_COUNTER.with(|ctcs_rrc| {
                    let c_i: usize = ctcs_rrc.get();                    
                    if c_i <= l-1 {
                        if c_i == l-1 {
                            ctcs_rrc.set(0);
                        } else {
                            ctcs_rrc.set(c_i+1);
                        }
                        Some(ctcs[c_i])
                    } else {
                        ctcs_rrc.set(1); // we check before that the len of the ctcs is at least 2 in the first match                         
                        Some(ctcs[0])
                    } 
                })
            }
        } 
    })
}

#[update]
pub async fn umc_user_transfer_cycles(umc_q: UMCUserTransferCyclesQuest) -> Result<(), UMCUserTransferCyclesError> {
    if STOP_CALLS.with(|stop_calls| { stop_calls.get() }) { trap("Maintenance. try again soon.") }
    // caller-check
    if with(&USERS_MAP_CANISTERS, |umcs| { !umcs.contains(&caller()) }) {
        trap("Caller of this method must be a CTS users-map-canister.")
    }
    
    if with(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| rcs.len()) >= MAX_RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS {
        //trap("The CTS MAX_RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS limit is hit") // 
        return Err(UMCUserTransferCyclesError::MaxReTryCtsUserTransferCyclesCallbacks(MAX_RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS));
    }
    
    let cycles_transferrer_canister_id: Principal = match get_next_cycles_transferrer_canister_round_robin() { 
        Some(cycles_transferrer_canister) => cycles_transferrer_canister,
        None => return Err(UMCUserTransferCyclesError::NoCyclesTransferrerCanistersFound) 
    }; 
    
    let user_transfer_cycles_quest_cycles: Cycles = umc_q.uc_user_transfer_cycles_quest.user_transfer_cycles_quest.cycles; // copy here before the umc_q move for the CTSUserTransferCyclesQuest
    
    match call_with_payment128::<(CTSUserTransferCyclesQuest,), (Result<(), CTSUserTransferCyclesError>,)>(
        cycles_transferrer_canister_id,
        "cts_user_transfer_cycles",
        (CTSUserTransferCyclesQuest{
            users_map_canister_id: caller(),
            umc_user_transfer_cycles_quest: umc_q
        },),
        user_transfer_cycles_quest_cycles
    ).await {
        Ok((cts_user_transfer_cycles_sponse,)) => match cts_user_transfer_cycles_sponse {
            Ok(()) => return Ok(()), 
            Err(cts_user_transfer_cycles_error) => match cts_user_transfer_cycles_error {
                CTSUserTransferCyclesError::MaxOngoingCyclesTransfers(max_ongoing_cycles_transfers) => {
                    /*let a_different_possible_cycles_transferrer_canister_id: Principal = */match get_next_cycles_transferrer_canister_round_robin(){
                        Some(c_id) => {
                            if c_id != cycles_transferrer_canister_id {
                                // try this different cycles_transferrer_canister
                            }
                        },
                        None => {}
                    };
                    return Err(UMCUserTransferCyclesError::CTSUserTransferCyclesError(cts_user_transfer_cycles_error)) // take this out when finish the try this different cycles_transferrer_canister 
                },
                _ => return Err(UMCUserTransferCyclesError::CTSUserTransferCyclesError(cts_user_transfer_cycles_error))
            }
        },
        Err(cts_user_transfer_cycles_call_error) => return Err(UMCUserTransferCyclesError::CTSUserTransferCyclesCallError(format!("{:?}", cts_user_transfer_cycles_call_error)))
    }
    
}






// return () or trap back to the cycles_transferrer before the first await in the same message execution as the msg_cycles_accept of the cycles_transfer_re_fund 
#[update(manual_reply = true)]
pub async fn cycles_transferrer_user_transfer_cycles_callback() {
    if STOP_CALLS.with(|stop_calls| { stop_calls.get() }) { trap("Maintenance. try again soon.") }
    
    if with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| { !ctcs.contains(&caller()) }) {
        trap("Caller must be a cts cycles_transferrer canister.")
    }
    
    let (cycles_transferrer_q,): (CyclesTransferrerUserTransferCyclesCallbackQuest,) = arg_data::<(CyclesTransferrerUserTransferCyclesCallbackQuest,)>();
    
    let user_transfer_cycles_refund: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    // unwrap bc want to trap here if candid broken bc the cycles transferrer can handle a trap here
    // make sure and test that a trap on the unwrap will give back the cycles for this user_transfer_cycles_refund to the cycles_transferrer 
    let cts_user_transfer_cycles_callback_quest: CTSUserTransferCyclesCallbackQuest = 
        CTSUserTransferCyclesCallbackQuest{
            user_id: cycles_transferrer_q.cts_user_transfer_cycles_quest.umc_user_transfer_cycles_quest.uc_user_transfer_cycles_quest.user_id,
            cycles_transfer_purchase_log_id: cycles_transferrer_q.cts_user_transfer_cycles_quest.umc_user_transfer_cycles_quest.uc_user_transfer_cycles_quest.cycles_transfer_purchase_log_id,
            cycles_transfer_refund: user_transfer_cycles_refund,
            cycles_transfer_call_error: cycles_transferrer_q.cycles_transfer_call_error
        }
    ;
    
    reply::<()>(()); // within this first (exe)cution
    
    do_cts_user_transfer_cycles_callback(
        cts_user_transfer_cycles_callback_quest,
        cycles_transferrer_q.cts_user_transfer_cycles_quest.umc_user_transfer_cycles_quest.user_canister_id
    ).await;
    
}




async fn do_cts_user_transfer_cycles_callback(cts_user_transfer_cycles_callback_quest: CTSUserTransferCyclesCallbackQuest, user_canister_id: UserCanisterId) {
    
    match call::<(&CTSUserTransferCyclesCallbackQuest,), (Result<(), CTSUserTransferCyclesCallbackError>,)>(
        user_canister_id,
        "cts_user_transfer_cycles_callback",
        (&cts_user_transfer_cycles_callback_quest,)
    ).await {
        Ok((cts_user_transfer_cycles_callback_sponse,)) => match cts_user_transfer_cycles_callback_sponse {
            Ok(()) => (),
            Err(cts_user_transfer_cycles_callback_error) => match cts_user_transfer_cycles_callback_error {
                CTSUserTransferCyclesCallbackError::WrongUserId => {
                    match find_user_in_the_users_map_canisters(cts_user_transfer_cycles_callback_quest.user_id).await {
                        Ok((found_umc_user_data, _users_map_canister_id)) => {
                            if found_umc_user_data.user_canister_id == user_canister_id {
                                // :log and re-try in this cts-canister
                                with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| { rcs.push((ReTryCTSUserTransferCyclesCallbackErrorKind::CTSUserTransferCyclesCallbackError(cts_user_transfer_cycles_callback_error), cts_user_transfer_cycles_callback_quest, user_canister_id)); });
                                return;
                            } else {
                                // call the new-found_user_canister_id
                                match call::<(&CTSUserTransferCyclesCallbackQuest,), (Result<(), CTSUserTransferCyclesCallbackError>,)>(
                                    found_umc_user_data.user_canister_id,
                                    "cts_user_transfer_cycles_callback",
                                    (&cts_user_transfer_cycles_callback_quest,)
                                ).await {
                                    Ok((cts_user_transfer_cycles_callback_sponse,)) => match cts_user_transfer_cycles_callback_sponse {
                                        Ok(()) => (),
                                        Err(cts_user_transfer_cycles_callback_error) => {
                                            // :log and re-try in this cts-canister
                                            with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| { rcs.push((ReTryCTSUserTransferCyclesCallbackErrorKind::CTSUserTransferCyclesCallbackError(cts_user_transfer_cycles_callback_error), cts_user_transfer_cycles_callback_quest, found_umc_user_data.user_canister_id,)); });
                                            return;
                                        }
                                    },
                                    Err(cts_user_transfer_cycles_callback_call_error) => {
                                        // :log and re-try in this cts-canister
                                        with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| { rcs.push((ReTryCTSUserTransferCyclesCallbackErrorKind::CTSUserTransferCyclesCallbackCallError((cts_user_transfer_cycles_callback_call_error.0 as u32, cts_user_transfer_cycles_callback_call_error.1)), cts_user_transfer_cycles_callback_quest, found_umc_user_data.user_canister_id,)); });
                                        return;
                                    }
                                }
                            }
                        },
                        Err(find_user_in_the_users_map_canisters_error) => match find_user_in_the_users_map_canisters_error {
                            FindUserInTheUsersMapCanistersError::UserNotFound => {
                                // check the save users-cycles-balance for the (time/)space if a user-canister runs out of time. if not there either:
                                // do nothing let it drop
                                return;
                            },
                            FindUserInTheUsersMapCanistersError::UsersMapCanistersFindUserCallFails(umc_call_errors) => {
                                // :log and re-try in this cts-canister
                                with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| { rcs.push((ReTryCTSUserTransferCyclesCallbackErrorKind::CTSUserTransferCyclesCallbackError(cts_user_transfer_cycles_callback_error), cts_user_transfer_cycles_callback_quest, user_canister_id)); });
                                return;
                            }
                        }
                    }
                },
                _ => {
                    // :log and re-try in this cts-canister
                    with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| { rcs.push((ReTryCTSUserTransferCyclesCallbackErrorKind::CTSUserTransferCyclesCallbackError(cts_user_transfer_cycles_callback_error), cts_user_transfer_cycles_callback_quest, user_canister_id)); });
                    return;
                }
            }
        },
        Err(cts_user_transfer_cycles_callback_call_error) => {
            // :log and re-try in this cts-canister
            with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |rcs| { rcs.push((ReTryCTSUserTransferCyclesCallbackErrorKind::CTSUserTransferCyclesCallbackCallError((cts_user_transfer_cycles_callback_call_error.0 as u32, cts_user_transfer_cycles_callback_call_error.1)), cts_user_transfer_cycles_callback_quest, user_canister_id)); });
            return;
        }
    }

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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&USERS_MAP_CANISTER_CODE, |umcc| {
        *umcc = canister_code;
    });
}




// certification? or replication-calls?
#[export_name = "canister_query controller_see_users_map_canisters"]
pub fn controller_see_users_map_canisters() {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with(&USERS_MAP_CANISTERS, |umcs| {
        ic_cdk::api::call::reply::<(&Vec<Principal>,)>((umcs,));
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    if with(&USERS_MAP_CANISTER_CODE, |umc_code| umc_code.module().len() == 0 ) {
        trap("USERS_MAP_CANISTER_CODE.module().len() is 0.")
    }
    
    let upgrade_umcs: Vec<Principal> = {
        if let Some(upgrade_umcs) = opt_upgrade_umcs {
            with(&USERS_MAP_CANISTERS, |umcs| { 
                upgrade_umcs.iter().for_each(|upgrade_umc| {
                    if !umcs.contains(&upgrade_umc) {
                        trap(&format!("cts users_map_canisters does not contain: {:?}", upgrade_umc));
                    }
                });
            });    
            upgrade_umcs
        } else {
            with(&USERS_MAP_CANISTERS, |umcs| { umcs.clone() })
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
                        wasm_module : unsafe {&*with(&USERS_MAP_CANISTER_CODE, |umc_code| { umc_code.module() as *const Vec<u8> })},
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
                        return Err((*umc_id/*copy*/, ControllerUpgradeUMCCallErrorType::StartCanisterCallError, (start_canister_call_error.0 as u32, start_canister_call_error.1))); 
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







#[update]
pub fn controller_put_user_canister_code(canister_code: CanisterCode) -> () {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&USER_CANISTER_CODE, |user_canister_code| {
        *user_canister_code = canister_code;
    });
}



pub type ControllerPutUCCodeOntoTheUMCError = (UsersMapCanisterId, (u32, String));

#[update]
pub async fn controller_put_uc_code_onto_the_umcs(opt_umcs: Option<Vec<UsersMapCanisterId>>) -> Vec<ControllerPutUCCodeOntoTheUMCError>/*umcs that the put_uc_code call fail*/ {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
        
    if with(&USER_CANISTER_CODE, |uc_code| uc_code.module().len() == 0 ) {
        trap("USER_CANISTER_CODE.module().len() is 0.")
    }
    
    let call_umcs: Vec<UsersMapCanisterId> = {
        if let Some(call_umcs) = opt_umcs {
            with(&USERS_MAP_CANISTERS, |umcs| { 
                call_umcs.iter().for_each(|call_umc| {
                    if !umcs.contains(&call_umc) {
                        trap(&format!("cts users_map_canisters does not contain: {:?}", call_umc));
                    }
                });
            });    
            call_umcs
        } else {
            with(&USERS_MAP_CANISTERS, |umcs| { umcs.clone() })
        }
    };    
    
    let sponses: Vec<Result<(), ControllerPutUCCodeOntoTheUMCError>> = futures::future::join_all(
        call_umcs.iter().map(|call_umc| {
            async {
                match call::<(&CanisterCode,), ()>(
                    *call_umc,
                    "cts_put_user_canister_code",
                    (unsafe{&*with(&USER_CANISTER_CODE, |uc_code| { uc_code as *const CanisterCode })},)
                ).await {
                    Ok(_) => {},
                    Err(call_error) => {
                        return Err((*call_umc/*copy*/, (call_error.0 as u32, call_error.1)));
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
pub async fn controller_upgrade_ucs_on_a_umc(umc: UsersMapCanisterId, opt_upgrade_ucs: Option<Vec<UserCanisterId>>, post_upgrade_arg: Vec<u8>) -> Result<Vec<UMCUpgradeUCError>, ControllerUpgradeUCSOnAUMCError> {       // /*:chunk-0 of the ucs that upgrade-fail*/ 
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&USERS_MAP_CANISTERS, |umcs| { umcs.contains(&umc) == false }) {
        trap(&format!("cts users_map_canisters does not contain: {:?}", umc));
    }
    
    match call::<(Option<Vec<UserCanisterId>>, Vec<u8>/*post-upgrade-arg*/), (Vec<UMCUpgradeUCError>,)>(
        umc,
        "cts_upgrade_ucs_chunk",
        (opt_upgrade_ucs, post_upgrade_arg)
    ).await {
        Ok((uc_upgrade_fails,)) => Ok(uc_upgrade_fails),
        Err(call_error) => Err(ControllerUpgradeUCSOnAUMCError::CTSUpgradeUCSCallError((call_error.0 as u32, call_error.1)))
    }

}






// ----- CYCLES_TRANSFERRER_CANISTERS-METHODS --------------------------


#[update]
pub fn controller_put_ctc_code(canister_code: CanisterCode) -> () {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&CYCLES_TRANSFERRER_CANISTER_CODE, |ctc_code| {
        *ctc_code = canister_code;
    });
}




#[export_name = "canister_query controller_see_cycles_transferrer_canisters"]
pub fn controller_see_cycles_transferrer_canisters() {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| {
        ic_cdk::api::call::reply::<(&Vec<Principal>,)>((ctcs,));
    });
}




#[update]
pub fn controller_put_cycles_transferrer_canisters(mut new_cycles_transferrer_canisters: Vec<Principal>) {
    
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    with_mut(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| {
        ctcs.append(&mut new_cycles_transferrer_canisters);
    });
}




#[update]
pub async fn controller_create_new_cycles_transferrer_canister() -> Principal {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    trap("")
} 



#[update]
pub async fn controller_see_cycles_transferrer_canister_re_try_cycles_transferrer_user_transfer_cycles_callbacks(cycles_transferrer_canister_id: Principal) -> Result<Vec<ReTryCyclesTransferrerUserTransferCyclesCallback>, (u32, String)> {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| { ctcs.contains(&cycles_transferrer_canister_id) == false }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| { ctcs.contains(&cycles_transferrer_canister_id) == false }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    if with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| { ctcs.contains(&cycles_transferrer_canister_id) == false }) {
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



// we upgrade the ctcs one at a time because if one of them takes too long to stop, we dont want to wait for it to come back, we will stop_calls, uninstall, and reinstall
#[update]
pub async fn controller_upgrade_ctc(upgrade_ctc: Principal, post_upgrade_arg: Vec<u8>) -> Result<(), ControllerUpgradeCTCError> {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }

    if with(&CYCLES_TRANSFERRER_CANISTER_CODE, |ctc_code| ctc_code.module().len() == 0 ) {
        trap("CYCLES_TRANSFERRER_CANISTER_CODE.module().len() is 0.")
    }
    
    if with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| { ctcs.contains(&upgrade_ctc) == false }) {
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
            wasm_module : unsafe{&*with(&CYCLES_TRANSFERRER_CANISTER_CODE, |ctc_code| { ctc_code.module() as *const Vec<u8> })},
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with(&NEW_USERS, |new_users| {
        ic_cdk::api::call::reply::<(Vec<(&UserId, &NewUserData)>,)>((new_users.iter().collect::<Vec<(&UserId, &NewUserData)>>(),));
    });
}














// ----- RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS-METHODS --------------------------

#[export_name = "canister_query controller_see_re_try_cts_user_transfer_cycles_callbacks"]
pub fn controller_see_re_try_cts_user_transfer_cycles_callbacks() {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |re_try_cts_user_transfer_cycles_callbacks| {
        ic_cdk::api::call::reply::<(&Vec<ReTryCTSUserTransferCyclesCallback>,)>((re_try_cts_user_transfer_cycles_callbacks,));
    });

}

#[update]
pub fn controller_drain_re_try_cts_user_transfer_cycles_callbacks() -> Vec<ReTryCTSUserTransferCyclesCallback> {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, 
        |re_try_cts_user_transfer_cycles_callbacks| { 
            re_try_cts_user_transfer_cycles_callbacks.drain(..).collect::<Vec<ReTryCTSUserTransferCyclesCallback>>() 
        }
    )

}

// controller method for the loop through the RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS and .pop() and call do_cts_user_transfer_cycles_callback

#[update(manual_reply = true)]
pub async fn controller_do_re_try_cts_user_transfer_cycles_callbacks() {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    futures::future::join_all(
        with_mut(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, 
            |re_try_cts_user_transfer_cycles_callbacks| { 
                re_try_cts_user_transfer_cycles_callbacks.drain(..).map(
                    |(_re_try_cts_user_transfer_cycles_callback_error_kind, cts_user_transfer_cycles_callback_quest, user_canister_id): ReTryCTSUserTransferCyclesCallback| {
                        do_cts_user_transfer_cycles_callback(cts_user_transfer_cycles_callback_quest, user_canister_id)
                    }
                ).collect::<Vec<_/*anonymous-future*/>>() 
            }
        )
    ).await;
    
    with(&RE_TRY_CTS_USER_TRANSFER_CYCLES_CALLBACKS, |re_try_cts_user_transfer_cycles_callbacks| {
        ic_cdk::api::call::reply::<(&Vec<ReTryCTSUserTransferCyclesCallback>,)>((re_try_cts_user_transfer_cycles_callbacks,));
    });

}







// ----- NEW_CANISTERS-METHODS --------------------------

#[update]
pub fn controller_put_new_canisters(mut new_canisters: Vec<Principal>) {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    NEW_CANISTERS.with(|ncs| {
        ncs.borrow_mut().append(&mut VecDeque::from(new_canisters)); // .extend_from_slice(&new_canisters) also works but it clones each item. .append moves each item
    });
}

#[export_name = "canister_query controller_see_new_canisters"]
pub fn controller_see_new_canisters() -> () {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with(&NEW_CANISTERS, |ncs| {
        ic_cdk::api::call::reply::<(Vec<&Principal>,)>((ncs.iter().collect::<Vec<&Principal>>(),));
    });

}







// ----- STOP_CALLS-METHODS --------------------------

#[update]
pub fn controller_set_stop_calls_flag(stop_calls_flag: bool) {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    STOP_CALLS.with(|stop_calls| { stop_calls.set(stop_calls_flag); });
}

#[query]
pub fn controller_see_stop_calls_flag() -> bool {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    STOP_CALLS.with(|stop_calls| { stop_calls.get() })
}







// ----- STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES-METHODS --------------------------

#[update]
pub fn controller_create_state_snapshot() -> u64/*len of the state_snapshot_candid_bytes*/ {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
        *state_snapshot_cts_data_candid_bytes = Vec::new();
    });    
}

#[update]
pub fn controller_append_state_snapshot_candid_bytes(mut append_bytes: Vec<u8>) {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with_mut(&STATE_SNAPSHOT_CTS_DATA_CANDID_BYTES, |state_snapshot_cts_data_candid_bytes| {
        state_snapshot_cts_data_candid_bytes.append(&mut append_bytes);
    });
}

#[update]
pub fn controller_re_store_cts_data_out_of_the_state_snapshot() {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    with(&CONTROLLERS, |controllers| { 
        reply::<(&Vec<Principal>,)>((controllers,)); 
    })
}


#[update]
pub fn controller_set_controllers(set_controllers: Vec<Principal>) {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    new_users_count: u64

}


#[query]
pub fn controller_see_metrics() -> Metrics {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
        trap("Caller must be a controller for this method.")
    }
    
    Metrics {
        global_allocator_counter: get_allocated_bytes_count() as u64,
        stable_size: ic_cdk::api::stable::stable64_size(),
        cycles_balance: ic_cdk::api::canister_balance128(),
        new_canisters_count: with(&NEW_CANISTERS, |nc| nc.len() as u64),
        users_map_canister_code_hash: with(&USERS_MAP_CANISTER_CODE, |umcc| { if umcc.module().len() != 0 { Some(*umcc.module_hash()) } else { None } }),
        user_canister_code_hash: with(&USER_CANISTER_CODE, |ucc| { if ucc.module().len() != 0 { Some(*ucc.module_hash()) } else { None } }),
        cycles_transferrer_canister_code_hash: with(&CYCLES_TRANSFERRER_CANISTER_CODE, |ctcc| { if ctcc.module().len() != 0 { Some(*ctcc.module_hash()) } else { None } }),
        users_map_canisters_count: with(&USERS_MAP_CANISTERS, |umcs| umcs.len() as u64),
        cycles_transferrer_canisters_count: with(&CYCLES_TRANSFERRER_CANISTERS, |ctcs| ctcs.len() as u64),
        latest_known_cmc_rate: LATEST_KNOWN_CMC_RATE.with(|cr| cr.get()),
        new_users_count: with(&NEW_USERS, |new_users| { new_users.len() as u64 })
        
    }
}





// ---------------------------- :FRONTCODE. -----------------------------------


#[update]
pub fn controller_upload_frontcode_file_chunks(file_path: String, file: File) -> () {
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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
    if with(&CONTROLLERS, |controllers| { !controllers.contains(&caller()) }) {
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


















