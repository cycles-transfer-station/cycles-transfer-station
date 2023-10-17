use std::{
    cell::{RefCell,Cell},
    collections::{HashMap}
};
use cts_lib::{
    self,
    ic_cdk::{
        self,
        api::{
            trap,
            caller,
            canister_balance128,
            call::{
                msg_cycles_accept128,
                msg_cycles_available128,
                msg_cycles_refunded128,
                RejectionCode,
                reject,
                reply,
                CallResult,
                arg_data,
                arg_data_raw_size,
                call,
                call_raw128
            },
        },
        update, 
        query, 
        init, 
        pre_upgrade, 
        post_upgrade
    },
    ic_ledger_types::{
        MAINNET_LEDGER_CANISTER_ID,
        IcpBlockHeight,
        IcpTokens,
        MAINNET_CYCLES_MINTING_CANISTER_ID
    },
    types::{
        Cycles,
        CTSFuel,
        CyclesTransfer,
        CyclesTransferMemo,
        CallError,
        cycles_bank::{
            CyclesBankInit,
        },
        cycles_market::{
            tc as cm_icrc1token_trade_contract,
            cm_main::Icrc1TokenTradeContract,
        },
        cts::{
            LengthenMembershipQuest,
            UserAndCB,
        }
    },
    cts_cb_authorizations::is_cts_cb_authorization_valid,
    consts::{
        MiB,
        GiB,
        NANOS_IN_A_SECOND,
        SECONDS_IN_A_DAY,
        WASM_PAGE_SIZE_BYTES,
        NETWORK_GiB_STORAGE_PER_SECOND_FEE_CYCLES,
        TRILLION,
    },
    tools::{
        time_nanos,
        time_nanos_u64,
        time_seconds,
        localkey::{
            self,
            refcell::{
                with, 
                with_mut,
            }
        },
        call_error_as_u32_and_string,
    },
    icrc::{BlockId},
    global_allocator_counter::get_allocated_bytes_count,
};
use canister_tools::{self, MemoryId};
use candid::{
    Principal,
    CandidType,
    Deserialize,
    utils::{
        encode_one,
        decode_one,
        encode_args,
    }
};

use serde::Serialize;

use futures::{poll, task::Poll};


// -------------------------------------------------------------------------


#[derive(CandidType, Serialize, Deserialize)]
struct CyclesTransferIn {
    id: u128,
    by_the_canister: Principal,
    cycles: Cycles,
    cycles_transfer_memo: CyclesTransferMemo,       // save max 32-bytes of the memo, of a Blob or of a Text
    timestamp_nanos: u128
}

#[derive(CandidType, Serialize, Deserialize)]
struct CyclesTransferOut {
    id: u128,
    for_the_canister: Principal,
    cycles_sent: Cycles,
    cycles_refunded: Option<Cycles>,   // option cause this field is only filled in the callback and that might not come back because of the callee holding-back the callback cross-upgrades. // if/when a user deletes some CyclesTransferPurchaseLogs, let the user set a special flag to delete the still-not-come-back-user_transfer_cycles by default unset.
    cycles_transfer_memo: CyclesTransferMemo,                           // save max 32-bytes of the memo, of a Blob or of a Text
    timestamp_nanos: u128, // time sent
    opt_cycles_transfer_call_error: Option<(u32/*reject_code*/, String/*reject_message*/)>, // None means the cycles_transfer-call replied. // save max 20-bytes of the string
}

// --------


#[derive(CandidType, Serialize, Deserialize)]
struct CMMessageCyclesPositionPurchasePositorLog{
    timestamp_nanos: u128,
    cm_message_cycles_position_purchase_positor_quest: cm_icrc1token_trade_contract::CMCyclesPositionPurchasePositorMessageQuest 
}

#[derive(CandidType, Serialize, Deserialize)]
struct CMMessageCyclesPositionPurchasePurchaserLog{
    timestamp_nanos: u128,
    cycles_purchase: Cycles,
    cm_message_cycles_position_purchase_purchaser_quest: cm_icrc1token_trade_contract::CMCyclesPositionPurchasePurchaserMessageQuest
}

#[derive(CandidType, Serialize, Deserialize)]
struct CMMessageTokenPositionPurchasePositorLog{
    timestamp_nanos: u128,
    cycles_payment: Cycles,
    cm_message_token_position_purchase_positor_quest: cm_icrc1token_trade_contract::CMTokenPositionPurchasePositorMessageQuest
}

#[derive(CandidType, Serialize, Deserialize)]
struct CMMessageTokenPositionPurchasePurchaserLog{
    timestamp_nanos: u128,
    cm_message_token_position_purchase_purchaser_quest: cm_icrc1token_trade_contract::CMTokenPositionPurchasePurchaserMessageQuest
}

#[derive(CandidType, Serialize, Deserialize)]
struct CMMessageVoidCyclesPositionPositorLog{
    timestamp_nanos: u128,
    void_cycles: Cycles,
    cm_message_void_cycles_position_positor_quest: cm_icrc1token_trade_contract::CMVoidCyclesPositionPositorMessageQuest
}

#[derive(CandidType, Serialize, Deserialize)]
struct CMMessageVoidTokenPositionPositorLog{
    timestamp_nanos: u128,
    cm_message_void_token_position_positor_quest: cm_icrc1token_trade_contract::CMVoidTokenPositionPositorMessageQuest
}

#[derive(Serialize, Deserialize)]
struct CMMessageLogs{
    cm_message_cycles_position_purchase_positor_logs: Vec<CMMessageCyclesPositionPurchasePositorLog>,
    cm_message_cycles_position_purchase_purchaser_logs: Vec<CMMessageCyclesPositionPurchasePurchaserLog>,
    cm_message_token_position_purchase_positor_logs: Vec<CMMessageTokenPositionPurchasePositorLog>,
    cm_message_token_position_purchase_purchaser_logs: Vec<CMMessageTokenPositionPurchasePurchaserLog>,
    cm_message_void_cycles_position_positor_logs: Vec<CMMessageVoidCyclesPositionPositorLog>,
    cm_message_void_token_position_positor_logs: Vec<CMMessageVoidTokenPositionPositorLog>,    
}
impl CMMessageLogs {
    fn new() -> Self {
        Self{
            cm_message_cycles_position_purchase_positor_logs: Vec::new(),
            cm_message_cycles_position_purchase_purchaser_logs: Vec::new(),
            cm_message_token_position_purchase_positor_logs: Vec::new(),
            cm_message_token_position_purchase_purchaser_logs: Vec::new(),
            cm_message_void_cycles_position_positor_logs: Vec::new(),
            cm_message_void_token_position_positor_logs: Vec::new(),                
        }
    }
}

#[derive(Serialize, Deserialize)]
struct CMTradeContractLogs {
    cm_message_logs: CMMessageLogs,
}
impl CMTradeContractLogs {
    fn new() -> Self {
        Self {
            cm_message_logs: CMMessageLogs::new(),
        }
    }
}
impl Default for CMTradeContractLogs {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
struct UserData {
    cycles_balance: Cycles,
    cycles_transfers_in: Vec<CyclesTransferIn>,
    cycles_transfers_out: Vec<CyclesTransferOut>,
    cm_trade_contracts: HashMap<Icrc1TokenTradeContract, CMTradeContractLogs>,
}

impl UserData {
    fn new() -> Self {
        Self {
            cycles_balance: 0u128,
            cycles_transfers_in: Vec::new(),
            cycles_transfers_out: Vec::new(),
            cm_trade_contracts: HashMap::new(),
        }
    }
}



#[derive(Serialize, Deserialize)]
struct CBData {
    user_canister_creation_timestamp_nanos: u128,
    cts_id: Principal,
    cbsm_id: Principal,
    user_id: Principal,
    storage_size_mib: u128,
    lifetime_termination_timestamp_seconds: u128,
    user_data: UserData,
    cycles_transfers_id_counter: u128,
    cts_cb_authorization: Vec<u8>,
    burn_icp_mint_cycles_mid_call_data: Option<BurnIcpMintCyclesData>,
}

impl CBData {
    fn new() -> Self {
        Self {
            user_canister_creation_timestamp_nanos: 0,
            cts_id: Principal::from_slice(&[]),
            cbsm_id: Principal::from_slice(&[]),
            user_id: Principal::from_slice(&[]),
            storage_size_mib: 0,       // memory-allocation/2 // is with the set in the canister_init // in the mib // starting at a 50mib-storage with a 1-year-user_canister_lifetime with a 5T-cycles-ctsfuel-balance at a cost: 10T-CYCLES   // this value is half of the user-canister-memory_allocation. for the upgrades.  
            lifetime_termination_timestamp_seconds: 0,
            user_data: UserData::new(),
            cycles_transfers_id_counter: 0,  
            cts_cb_authorization: Vec::new(),
            burn_icp_mint_cycles_mid_call_data: None,
        }
    }
}

const CYCLES_TRANSFER_MEMO_MAX_SIZE: usize = 32;
const MINIMUM_USER_TRANSFER_CYCLES: Cycles = 10_000_000_000;
const CYCLES_TRANSFER_IN_MINIMUM_CYCLES: Cycles = 10_000_000_000;



const DELETE_LOG_MINIMUM_WAIT_NANOS: u128 = NANOS_IN_A_SECOND * SECONDS_IN_A_DAY * 45;

const STABLE_MEMORY_ID_CB_DATA_SERIALIZATION: MemoryId = MemoryId::new(0);

const USER_CANISTER_BACKUP_CYCLES: Cycles = 2 * TRILLION;

const CTSFUEL_BALANCE_TOO_LOW_REJECT_MESSAGE: &'static str = "ctsfuel_balance is too low";

const CYCLES_TRANSFER_OUT_ERROR_STRING_MAX_LENGTH: usize = 40;

thread_local! {
   
    static CB_DATA: RefCell<CBData> = RefCell::new(CBData::new());

    // not save in a CBData
    static MEMORY_SIZE_AT_THE_START: Cell<usize> = Cell::new(0); 
    static STOP_CALLS: Cell<bool> = Cell::new(false);
    static STATE_SNAPSHOT_CB_DATA_CANDID_BYTES: RefCell<Vec<u8>> = RefCell::new(Vec::new());

}



// ---------------------------------------------------------------------------------


#[init]
fn canister_init(user_canister_init: CyclesBankInit) {
    
    canister_tools::init(&CB_DATA, STABLE_MEMORY_ID_CB_DATA_SERIALIZATION);
    
    with_mut(&CB_DATA, |cb_data| {
        *cb_data = CBData{
            user_canister_creation_timestamp_nanos:                 time_nanos(),
            cts_id:                                                 user_canister_init.cts_id,
            cbsm_id:                                                user_canister_init.cbsm_id,
            user_id:                                                user_canister_init.user_id,
            storage_size_mib:                                       user_canister_init.storage_size_mib,
            lifetime_termination_timestamp_seconds:                 user_canister_init.lifetime_termination_timestamp_seconds,
            ..CBData::new()    
        };
        
        cb_data.user_data.cycles_balance = user_canister_init.start_with_user_cycles_balance;
    });
   
    localkey::cell::set(&MEMORY_SIZE_AT_THE_START, core::arch::wasm32::memory_size(0)*WASM_PAGE_SIZE_BYTES);
    
}

#[pre_upgrade]
fn pre_upgrade() {
    canister_tools::pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    
    localkey::cell::set(&MEMORY_SIZE_AT_THE_START, core::arch::wasm32::memory_size(0)*WASM_PAGE_SIZE_BYTES);
    
    canister_tools::post_upgrade(&CB_DATA, STABLE_MEMORY_ID_CB_DATA_SERIALIZATION, None::<fn(CBData) -> CBData>);
}

// ---------------------------

// this is onli for ingress-messages (calls that come from outside the network)
#[no_mangle]
fn canister_inspect_message() {
    use cts_lib::ic_cdk::api::call::{method_name, accept_message};
    
    let public_methods = [
        "get_cts_cb_auth",
        "local_put_ic_root_key"
    ];
    if public_methods.contains(&&method_name()[..]) == false {
        if caller() != user_id() {
            trap("caller must be the owner");
        }
    }
    accept_message();
}



// ---------------------------------------------------------------------------------

fn cts_id() -> Principal {
    with(&CB_DATA, |cb_data| { cb_data.cts_id })
}

fn user_id() -> Principal {
    with(&CB_DATA, |cb_data| { cb_data.user_id })
}

fn new_cycles_transfer_id(id_counter: &mut u128) -> u128 {
    let id: u128 = id_counter.clone();
    *id_counter += 1;
    id
}

    
// compute the size of a CyclesTransferIn and of a CyclesTransferOut, check the length of both vectors, and compute the current storage usage. 
fn calculate_current_storage_usage(cb_data: &CBData) -> u128 {
    let mut count: u128 = (
        localkey::cell::get(&MEMORY_SIZE_AT_THE_START) 
        + 
        cb_data.user_data.cycles_transfers_in.len() * ( std::mem::size_of::<CyclesTransferIn>() + CYCLES_TRANSFER_MEMO_MAX_SIZE/*for the cycles-transfer-memo-heap-size*/ )
        + 
        cb_data.user_data.cycles_transfers_out.len() * ( std::mem::size_of::<CyclesTransferOut>() + CYCLES_TRANSFER_MEMO_MAX_SIZE/*for the cycles-transfer-memo-heap-size*/ + CYCLES_TRANSFER_OUT_ERROR_STRING_MAX_LENGTH/*for the possible-call-error-string-heap-size*/ )
        +
        cb_data.user_data.cm_trade_contracts.len() * std::mem::size_of::<Icrc1TokenTradeContract>()
        +
        cb_data.user_data.cm_trade_contracts
            .values()
            .fold(0, |c, cm_trade_contract_logs| { 
                c
                +
                cm_trade_contract_logs.cm_message_logs.cm_message_cycles_position_purchase_positor_logs.len() * std::mem::size_of::<CMMessageCyclesPositionPurchasePositorLog>()            
                +
                cm_trade_contract_logs.cm_message_logs.cm_message_cycles_position_purchase_purchaser_logs.len() * std::mem::size_of::<CMMessageCyclesPositionPurchasePurchaserLog>()
                +
                cm_trade_contract_logs.cm_message_logs.cm_message_token_position_purchase_positor_logs.len() * std::mem::size_of::<CMMessageTokenPositionPurchasePositorLog>()
                +
                cm_trade_contract_logs.cm_message_logs.cm_message_token_position_purchase_purchaser_logs.len() * std::mem::size_of::<CMMessageTokenPositionPurchasePurchaserLog>()
                +
                cm_trade_contract_logs.cm_message_logs.cm_message_void_cycles_position_positor_logs.len() * std::mem::size_of::<CMMessageVoidCyclesPositionPositorLog>()
                +
                cm_trade_contract_logs.cm_message_logs.cm_message_void_token_position_positor_logs.len() * std::mem::size_of::<CMMessageVoidTokenPositionPositorLog>()
            })          
    ) as u128;
    if cb_data.burn_icp_mint_cycles_mid_call_data.is_some() {
        count += (std::mem::size_of::<CyclesTransferIn>() + CYCLES_TRANSFER_MEMO_MAX_SIZE) as u128;
    }
    count
}

fn calculate_free_storage(cb_data: &CBData) -> u128 {
    ( cb_data.storage_size_mib * MiB as u128 ).saturating_sub(calculate_current_storage_usage(cb_data))
}


fn ctsfuel_balance(cb_data: &CBData) -> CTSFuel {
    canister_balance128()
    .saturating_sub(cb_data.user_data.cycles_balance)
    .saturating_sub(USER_CANISTER_BACKUP_CYCLES)
    .saturating_sub(
        (
            cb_data.lifetime_termination_timestamp_seconds.saturating_sub(time_seconds()) 
            * 
            cts_lib::consts::cb_storage_size_mib_as_cb_network_memory_allocation_mib(cb_data.storage_size_mib) * MiB as u128 // canister-memory-allocation in the mib
        )
        *
        NETWORK_GiB_STORAGE_PER_SECOND_FEE_CYCLES
        /
        GiB as u128 /*network storage charge per byte per second*/
    )
}

fn truncate_cycles_transfer_memo(mut cycles_transfer_memo: CyclesTransferMemo) -> CyclesTransferMemo {
    match cycles_transfer_memo {
        CyclesTransferMemo::Nat(_n) => {},
        CyclesTransferMemo::Int(_int) => {},
        CyclesTransferMemo::Blob(ref mut b) => {
            b.truncate(CYCLES_TRANSFER_MEMO_MAX_SIZE);
            b.shrink_to_fit();
        },
         CyclesTransferMemo::Text(ref mut s) => {
            s.truncate(CYCLES_TRANSFER_MEMO_MAX_SIZE);
            s.shrink_to_fit();
        }
    }
    cycles_transfer_memo
}

fn maintenance_check() {
    if localkey::cell::get(&STOP_CALLS) == true { 
        trap("Maintenance, try soon."); 
    }
}


// -------------- DOWNLOAD-LOGS-MECHANISM ------------------

#[derive(CandidType, Deserialize)]
pub struct DownloadCBLogsQuest {
    pub opt_start_before_i: Option<u64>,
    pub chunk_size: u64
}

#[derive(CandidType)]
pub struct DownloadCBLogsSponse<'a, T: 'a> {
    pub logs_len: u64,
    pub logs: Option<&'a [T]>
}

fn download_logs<T: CandidType>(q: DownloadCBLogsQuest, data: &Vec<T>/*not a slice here bc want to make sure we always pass the whole vec into this function*/) -> DownloadCBLogsSponse<T> {
    DownloadCBLogsSponse{
        logs_len: data.len() as u64,
        logs: data[..q.opt_start_before_i.map(|i| i as usize).unwrap_or(data.len())].rchunks(q.chunk_size as usize).next()
    }
}

// ---------------------------------------------


#[export_name = "canister_update cycles_transfer"]
pub fn cycles_transfer() { // (ct: CyclesTransfer) -> ()

    maintenance_check();

    if with(&CB_DATA, |cb_data| { calculate_free_storage(cb_data) }) < std::mem::size_of::<CyclesTransferIn>() as u128 + 32 {
        if caller() == cts_id() {
            with_mut(&CB_DATA, |cb_data| { cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(msg_cycles_accept128(msg_cycles_available128())); });
            reply::<()>(());
            return;            
        }
        trap("Canister memory is full, cannot create a log of the cycles-transfer.");
    }

    if arg_data_raw_size() > 150 {
        trap("arg_data_raw_size can be max 150 bytes");
    }

    if msg_cycles_available128() < CYCLES_TRANSFER_IN_MINIMUM_CYCLES {
        trap(&format!("minimum cycles transfer cycles: {}", CYCLES_TRANSFER_IN_MINIMUM_CYCLES));
    }
        
    let cycles_cept: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    let (ct_memo, by_the_canister): (CyclesTransferMemo, Principal) = {
        let (ct,): (CyclesTransfer,) = arg_data::<(CyclesTransfer,)>();
        (ct.memo, caller())    
    };
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(cycles_cept);
        cb_data.user_data.cycles_transfers_in.push(
            CyclesTransferIn{
                id: new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter),
                by_the_canister,
                cycles: cycles_cept,
                cycles_transfer_memo: truncate_cycles_transfer_memo(ct_memo),
                timestamp_nanos: time_nanos()
            }
        );
    });
    
    reply::<()>(());
    return;
    
}



#[query(manual_reply = true)]
pub fn download_cycles_transfers_in(q: DownloadCBLogsQuest) {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    if with(&CB_DATA, |cb_data| { ctsfuel_balance(cb_data) }) < 10_000_000_000 {
        reject(CTSFUEL_BALANCE_TOO_LOW_REJECT_MESSAGE);
        return;
    }    
    
    maintenance_check();
    
    with(&CB_DATA, |cb_data| {
        reply((download_logs(q, &cb_data.user_data.cycles_transfers_in),)); 
    });
} 

#[update(manual_reply = true)]
pub fn delete_cycles_transfers_in(delete_cycles_transfers_in_ids: Vec<u128>) {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    if with(&CB_DATA, |cb_data| { ctsfuel_balance(cb_data) }) < 10_000_000_000 {
        reject(CTSFUEL_BALANCE_TOO_LOW_REJECT_MESSAGE);
        return;
    }
    
    maintenance_check();
    
    with_mut(&CB_DATA, |cb_data| {
        for delete_cycles_transfer_in_id in delete_cycles_transfers_in_ids.into_iter() {
            match cb_data.user_data.cycles_transfers_in.binary_search_by_key(&delete_cycles_transfer_in_id, |cycles_transfer_in| { cycles_transfer_in.id }) {
                Ok(i) => {
                    if time_nanos().saturating_sub(cb_data.user_data.cycles_transfers_in[i].timestamp_nanos) < DELETE_LOG_MINIMUM_WAIT_NANOS {
                        trap(&format!("cycles_transfer_in id: {} is too new to delete. must be at least {} days in the past to delete.", delete_cycles_transfer_in_id, DELETE_LOG_MINIMUM_WAIT_NANOS/NANOS_IN_A_SECOND/SECONDS_IN_A_DAY));
                    }
                    cb_data.user_data.cycles_transfers_in.remove(i);
                },
                Err(_) => {
                    trap(&format!("cycles_transfer_in id: {} not found.", delete_cycles_transfer_in_id))
                }
            }
        }
    });
    
    reply::<()>(());
}






#[derive(CandidType, Deserialize, Clone)]
pub struct UserTransferCyclesQuest {
    for_the_canister: Principal,
    cycles: Cycles,
    cycles_transfer_memo: CyclesTransferMemo
}


#[derive(CandidType)]
pub enum UserTransferCyclesError {
    MemoryIsFull,
    InvalidCyclesTransferMemoSize{max_size_bytes: u128},
    InvalidTransferCyclesAmount{ minimum_user_transfer_cycles: Cycles },
    CyclesBalanceTooLow { cycles_balance: Cycles },
    CyclesTransferCallPerformError(CallError)
}

#[derive(CandidType, Deserialize, Clone)]
pub struct UserTransferCyclesSponse {
    cycles_refund: Cycles,
    cycles_transfer_id: u128,
    opt_cycles_transfer_call_error: Option<CallError>,
}

#[update]
pub async fn transfer_cycles(mut q: UserTransferCyclesQuest, (user_of_the_cb, cts_cb_auth): (Principal/*user_id*/, Vec<u8>/*cts_cb_authorization*/)) -> Result<UserTransferCyclesSponse, UserTransferCyclesError> {

    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    maintenance_check();
    
    if is_cts_cb_authorization_valid(
        cts_id(),
        UserAndCB{user_id: user_of_the_cb, cb_id: q.for_the_canister },
        cts_cb_auth,
    ) == false {
        trap("For the now, must transfer cycles with the CTS cycles-banks.");
    }
    
    if with(&CB_DATA, |cb_data| { calculate_free_storage(cb_data) }) < (std::mem::size_of::<CyclesTransferOut>() + CYCLES_TRANSFER_MEMO_MAX_SIZE + CYCLES_TRANSFER_OUT_ERROR_STRING_MAX_LENGTH) as u128 {
        return Err(UserTransferCyclesError::MemoryIsFull);
    }
    
    if q.cycles < MINIMUM_USER_TRANSFER_CYCLES {
        return Err(UserTransferCyclesError::InvalidTransferCyclesAmount{ minimum_user_transfer_cycles: MINIMUM_USER_TRANSFER_CYCLES });
    }
    
    if q.cycles > with(&CB_DATA, |cb_data| { cb_data.user_data.cycles_balance }) {
        return Err(UserTransferCyclesError::CyclesBalanceTooLow{ cycles_balance: with(&CB_DATA, |cb_data| { cb_data.user_data.cycles_balance }) });
    }
    
    // check memo size
    match q.cycles_transfer_memo {
        CyclesTransferMemo::Blob(ref mut b) => {
            if b.len() > CYCLES_TRANSFER_MEMO_MAX_SIZE {
                return Err(UserTransferCyclesError::InvalidCyclesTransferMemoSize{max_size_bytes: CYCLES_TRANSFER_MEMO_MAX_SIZE as u128}); 
            }
            b.shrink_to_fit();
            if b.capacity() > b.len() { trap("check this out"); }
        },
        CyclesTransferMemo::Text(ref mut s) => {
            if s.len() > CYCLES_TRANSFER_MEMO_MAX_SIZE {
                return Err(UserTransferCyclesError::InvalidCyclesTransferMemoSize{max_size_bytes: CYCLES_TRANSFER_MEMO_MAX_SIZE as u128}); 
            }
            s.shrink_to_fit();
            if s.capacity() > s.len() { trap("check this out"); }
        },
        CyclesTransferMemo::Nat(_n) => {},
        CyclesTransferMemo::Int(_int) => {} 
    }
 
    let mut call_future = call_raw128(
        q.for_the_canister,
        "cycles_transfer",
        encode_one(&CyclesTransfer{ memo: q.cycles_transfer_memo.clone() }).unwrap(),
        q.cycles
    );
    
    if let Poll::Ready(x) = poll!(&mut call_future) {
        return Err(UserTransferCyclesError::CyclesTransferCallPerformError(call_error_as_u32_and_string(x.unwrap_err())));
    }
    
    let cycles_transfer_id: u128 = with_mut(&CB_DATA, |cb_data| {
        let cycles_transfer_id: u128 = new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter);        
        // take the user-cycles before the transfer, and refund in the callback 
        cb_data.user_data.cycles_balance -= q.cycles;
        cb_data.user_data.cycles_transfers_out.push(
            CyclesTransferOut{
                id: cycles_transfer_id,
                for_the_canister: q.for_the_canister,
                cycles_sent: q.cycles,
                cycles_refunded: None,   // None means the cycles_transfer-call-callback did not come back yet(did not give-back a reply-or-reject-sponse) 
                cycles_transfer_memo: q.cycles_transfer_memo,
                timestamp_nanos: time_nanos(), // time sent
                opt_cycles_transfer_call_error: None,
            }
        );
        cycles_transfer_id
    });
        
    let call_result: CallResult<Vec<u8>> = call_future.await;
    
    let cycles_refund: Cycles = msg_cycles_refunded128(); 
    
    let opt_cycles_transfer_call_error: Option<CallError> = call_result.err().map(call_error_as_u32_and_string);
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(cycles_refund);
        
        if let Ok(i) = cb_data.user_data.cycles_transfers_out.binary_search_by_key(&cycles_transfer_id, |ct_out| ct_out.id) {
            let ct_out: &mut CyclesTransferOut = &mut cb_data.user_data.cycles_transfers_out[i];
            ct_out.cycles_refunded = Some(cycles_refund);
            ct_out.opt_cycles_transfer_call_error = opt_cycles_transfer_call_error.clone().map(|(ec, mut es)| { es.truncate(CYCLES_TRANSFER_OUT_ERROR_STRING_MAX_LENGTH); (ec, es) });
        };
    });    
    
    Ok(UserTransferCyclesSponse {
        cycles_refund,
        cycles_transfer_id,
        opt_cycles_transfer_call_error,
    })
}




#[query(manual_reply = true)]
pub fn download_cycles_transfers_out(q: DownloadCBLogsQuest) {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    if with(&CB_DATA, |cb_data| { ctsfuel_balance(cb_data) }) < 10_000_000_000 {
        reject(CTSFUEL_BALANCE_TOO_LOW_REJECT_MESSAGE);
        return;
    }
    
    maintenance_check();

    with(&CB_DATA, |cb_data| {
        reply((download_logs(q, &cb_data.user_data.cycles_transfers_out),)); 
    });
}


#[update(manual_reply = true)]
pub fn delete_cycles_transfers_out(delete_cycles_transfers_out_ids: Vec<u128>) {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    if with(&CB_DATA, |cb_data| { ctsfuel_balance(cb_data) }) < 10_000_000_000 {
        reject(CTSFUEL_BALANCE_TOO_LOW_REJECT_MESSAGE);
        return;
    }
    
    maintenance_check();
    
    with_mut(&CB_DATA, |cb_data| {
        for delete_cycles_transfer_out_id in delete_cycles_transfers_out_ids.into_iter() {
            match cb_data.user_data.cycles_transfers_out.binary_search_by_key(&delete_cycles_transfer_out_id, |cycles_transfer_out| { cycles_transfer_out.id }) {
                Ok(i) => {
                    if time_nanos().saturating_sub(cb_data.user_data.cycles_transfers_out[i].timestamp_nanos) < DELETE_LOG_MINIMUM_WAIT_NANOS {
                        trap(&format!("cycles_transfer_out id: {} is too new to delete. must be at least {} days in the past to delete.", delete_cycles_transfer_out_id, DELETE_LOG_MINIMUM_WAIT_NANOS/NANOS_IN_A_SECOND/SECONDS_IN_A_DAY));
                    }
                    cb_data.user_data.cycles_transfers_out.remove(i);
                },
                Err(_) => {
                    trap(&format!("cycles_transfer_out id: {} not found.", delete_cycles_transfer_out_id))
                }
            }
        }
    });
    
    reply::<()>(());
}



// --------------------------
// bank-token-methods


#[update(manual_reply = true)]
pub async fn transfer_icrc1(icrc1_ledger: Principal, icrc1_transfer_arg_raw: Vec<u8>) {//-> CallResult<Vec<u8>> {
    if caller() != user_id() { trap("Caller must be the user"); }
    
    let call_result: CallResult<Vec<u8>> = call_raw128(
        icrc1_ledger,
        "icrc1_transfer",
        &icrc1_transfer_arg_raw,//&encode_one(&icrc1_transfer_arg).unwrap(),
        0
    ).await;
    
    reply::<(CallResult<Vec<u8>>,)>((call_result,));
}


// because the first icp account ids are not possible to use with the icrc1_transfer function.
#[update(manual_reply = true)]
pub async fn transfer_icp(transfer_arg_raw: Vec<u8>) {
    if caller() != user_id() { trap("Caller must be the user"); }
    
    let s: CallResult<Vec<u8>> = call_raw128(
        MAINNET_LEDGER_CANISTER_ID,
        "transfer",
        &transfer_arg_raw,
        0
    ).await;
    
    reply::<(CallResult<Vec<u8>>,)>((s,));

}

// ----------------------------------
// user is in the middle of a different call
 
#[derive(CandidType, Deserialize)]
pub enum UserIsInTheMiddleOfADifferentCall {
    BurnIcpMintCyclesCall{ must_call_complete: bool },
}


fn check_if_user_is_in_the_middle_of_a_different_call(cb_data: &CBData) -> Result<(), UserIsInTheMiddleOfADifferentCall> {
    if let Some(ref burn_icp_mint_cycles_data) = cb_data.burn_icp_mint_cycles_mid_call_data {
        return Err(UserIsInTheMiddleOfADifferentCall::BurnIcpMintCyclesCall{ must_call_complete: !burn_icp_mint_cycles_data.lock });
    }
    Ok(())           
}



// ---------------------------------------
// burn-icp-mint-cycles

use cts_lib::cmc::*;


// options are for the memberance of the steps
#[derive(Serialize, Deserialize, Clone)]
pub struct BurnIcpMintCyclesData {
    start_time_nanos: u64,
    lock: bool,
    burn_icp_mint_cycles_quest: BurnIcpMintCyclesQuest, 
    cmc_icp_transfer_block_height: Option<IcpBlockHeight>,
    cmc_cycles: Option<Cycles>,
}


#[derive(CandidType, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BurnIcpMintCyclesQuest {
    burn_icp: IcpTokens,    
}

#[derive(CandidType, Deserialize)]
pub enum BurnIcpMintCyclesError {
    UserIsInTheMiddleOfADifferentCall(UserIsInTheMiddleOfADifferentCall),
    LedgerTopupCyclesCmcIcpTransferError(LedgerTopupCyclesCmcIcpTransferError),
    LedgerTopupCyclesCmcNotifyRefund{ block_index: u64, reason: String},
    MidCallError(BurnIcpMintCyclesMidCallError)
}


#[derive(CandidType, Deserialize)]
pub enum BurnIcpMintCyclesMidCallError {
    LedgerTopupCyclesCmcNotifyError(LedgerTopupCyclesCmcNotifyError),
}

#[derive(CandidType, Deserialize, PartialEq, Eq, Clone)]
pub struct BurnIcpMintCyclesSuccess {
    mint_cycles: Cycles
}



#[update]
pub async fn burn_icp_mint_cycles(q: BurnIcpMintCyclesQuest) -> Result<BurnIcpMintCyclesSuccess, BurnIcpMintCyclesError> {
    if caller() != user_id() { trap("Caller must be the user"); }

    if with(&CB_DATA, |cb_data| { calculate_free_storage(cb_data) }) < (std::mem::size_of::<CyclesTransferIn>() + CYCLES_TRANSFER_MEMO_MAX_SIZE) as u128 {
        trap("Bank memory is full.");
    }

    let mid_call_data: BurnIcpMintCyclesData = with_mut(&CB_DATA, |cb_data| {
        check_if_user_is_in_the_middle_of_a_different_call(cb_data).map_err(|e| BurnIcpMintCyclesError::UserIsInTheMiddleOfADifferentCall(e))?;
        let mid_call_data: BurnIcpMintCyclesData = BurnIcpMintCyclesData{
            start_time_nanos: time_nanos_u64(),
            lock: true,
            burn_icp_mint_cycles_quest: q, 
            cmc_icp_transfer_block_height: None,
            cmc_cycles: None,
        };
        cb_data.burn_icp_mint_cycles_mid_call_data = Some(mid_call_data.clone());
        Ok(mid_call_data)
    })?;

    burn_icp_mint_cycles_(mid_call_data).await
}

async fn burn_icp_mint_cycles_(mut burn_icp_mint_cycles_data: BurnIcpMintCyclesData) -> Result<BurnIcpMintCyclesSuccess, BurnIcpMintCyclesError> {   
    
    if burn_icp_mint_cycles_data.cmc_icp_transfer_block_height.is_none() {
        match ledger_topup_cycles_cmc_icp_transfer(burn_icp_mint_cycles_data.burn_icp_mint_cycles_quest.burn_icp, None, ic_cdk::api::id()).await {
            Ok(block_height) => { burn_icp_mint_cycles_data.cmc_icp_transfer_block_height = Some(block_height); },
            Err(ledger_topup_cycles_cmc_icp_transfer_error) => {
                with_mut(&CB_DATA, |cb_data| { cb_data.burn_icp_mint_cycles_mid_call_data = None; });
                return Err(BurnIcpMintCyclesError::LedgerTopupCyclesCmcIcpTransferError(ledger_topup_cycles_cmc_icp_transfer_error));
            }
        }
    }
    
    if burn_icp_mint_cycles_data.cmc_cycles.is_none() {
        match ledger_topup_cycles_cmc_notify(burn_icp_mint_cycles_data.cmc_icp_transfer_block_height.unwrap(), ic_cdk::api::id()).await {
            Ok(cmc_cycles) => { 
                burn_icp_mint_cycles_data.cmc_cycles = Some(cmc_cycles); 
            },
            Err(ledger_topup_cycles_cmc_notify_error) => {
                if let LedgerTopupCyclesCmcNotifyError::CmcNotifyError(CmcNotifyError::Refunded{ block_index: Some(b), reason: r }) = ledger_topup_cycles_cmc_notify_error {
                    with_mut(&CB_DATA, |cb_data| { cb_data.burn_icp_mint_cycles_mid_call_data = None; });
                    return Err(BurnIcpMintCyclesError::LedgerTopupCyclesCmcNotifyRefund{ block_index: b, reason: r});
                } else {
                    burn_icp_mint_cycles_data.lock = false;
                    with_mut(&CB_DATA, |cb_data| {
                        cb_data.burn_icp_mint_cycles_mid_call_data = Some(burn_icp_mint_cycles_data); 
                    });
                    return Err(BurnIcpMintCyclesError::MidCallError(BurnIcpMintCyclesMidCallError::LedgerTopupCyclesCmcNotifyError(ledger_topup_cycles_cmc_notify_error)));
                }
            }
        }
    }
    let cmc_cycles: Cycles = burn_icp_mint_cycles_data.cmc_cycles.unwrap();
    
    with_mut(&CB_DATA, |cb_data| { 
        cb_data.burn_icp_mint_cycles_mid_call_data = None;
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(cmc_cycles); 
        cb_data.user_data.cycles_transfers_in.push(
            CyclesTransferIn{
                id: new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter),
                by_the_canister: MAINNET_CYCLES_MINTING_CANISTER_ID,
                cycles: cmc_cycles,
                cycles_transfer_memo: CyclesTransferMemo::Blob([*b"CMC-MINT", burn_icp_mint_cycles_data.cmc_icp_transfer_block_height.unwrap().to_be_bytes()].concat()),
                timestamp_nanos: time_nanos()
            }
        );
    });
    Ok(BurnIcpMintCyclesSuccess{
        mint_cycles: burn_icp_mint_cycles_data.cmc_cycles.unwrap(),
    })
    
}



#[derive(CandidType, Deserialize)]
pub enum CompleteBurnIcpMintCyclesError{
    UserIsNotInTheMiddleOfABurnIcpMintCyclesCall,
    BurnIcpMintCyclesError(BurnIcpMintCyclesError)
}

#[update]
pub async fn complete_burn_icp_mint_cycles() -> Result<BurnIcpMintCyclesSuccess, CompleteBurnIcpMintCyclesError> {
    if caller() != user_id() { trap("Caller must be the user"); }

    complete_burn_icp_mint_cycles_().await
}


async fn complete_burn_icp_mint_cycles_() -> Result<BurnIcpMintCyclesSuccess, CompleteBurnIcpMintCyclesError> {
    
    let burn_icp_mint_cycles_data: BurnIcpMintCyclesData = with_mut(&CB_DATA, |cb_data| {
        match cb_data.burn_icp_mint_cycles_mid_call_data {
            Some(ref mut burn_icp_mint_cycles_data) => {
                if burn_icp_mint_cycles_data.lock == true {
                    return Err(CompleteBurnIcpMintCyclesError::BurnIcpMintCyclesError(BurnIcpMintCyclesError::UserIsInTheMiddleOfADifferentCall(UserIsInTheMiddleOfADifferentCall::BurnIcpMintCyclesCall{ must_call_complete: false })));
                }
                burn_icp_mint_cycles_data.lock = true;
                Ok(burn_icp_mint_cycles_data.clone())
            },
            None => {
                return Err(CompleteBurnIcpMintCyclesError::UserIsNotInTheMiddleOfABurnIcpMintCyclesCall);
            }
        }
    })?;

    burn_icp_mint_cycles_(burn_icp_mint_cycles_data).await
        .map_err(|burn_icp_mint_cycles_error| { 
            CompleteBurnIcpMintCyclesError::BurnIcpMintCyclesError(burn_icp_mint_cycles_error) 
        })
}


// ---------------------------------------------------
// cycles-market methods


use cts_lib::types::cycles_market::tc as cm_tc;

#[derive(CandidType)]
pub enum CBTradeCyclesError {
    MemoryIsFull,
    CyclesBalanceTooLow{ cycles_balance: Cycles },
    CMTradeCyclesCallError((u32, String)),
    CMTradeCyclesCallSponseCandidDecodeError{candid_error: String, sponse_bytes: Vec<u8> },
}

type CBTradeCyclesResult = Result<cm_tc::BuyTokensResult, CBTradeCyclesError>;

#[update]
pub async fn cm_trade_cycles(icrc1token_trade_contract: Icrc1TokenTradeContract, q: cm_tc::BuyTokensQuest) -> CBTradeCyclesResult {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    let put_call_cycles: Cycles = q.cycles;
    
    with(&CB_DATA, |cb_data| { 
        if calculate_free_storage(cb_data) < 200 {
            return Err(CBTradeCyclesError::MemoryIsFull);
        }
        if cb_data.user_data.cycles_balance < put_call_cycles {
            return Err(CBTradeCyclesError::CyclesBalanceTooLow{ cycles_balance: cb_data.user_data.cycles_balance });
        }
        Ok(())
    })?;
    
    let mut call_future = with(&CB_DATA, |cb_data| { 
        call_raw128(
            icrc1token_trade_contract.trade_contract_canister_id,
            "trade_cycles",
            encode_args((&q, (cb_data.user_id, &cb_data.cts_cb_authorization))).unwrap(),
            put_call_cycles
        )
    });
    
    if let futures::task::Poll::Ready(call_result_with_an_error) = futures::poll!(&mut call_future) {
        let call_error: (RejectionCode, String) = call_result_with_an_error.unwrap_err();
        return Err(CBTradeCyclesError::CMTradeCyclesCallError((call_error.0 as u32, "call_perform error".to_string())));
    }
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_sub(put_call_cycles);
        cb_data.user_data.cm_trade_contracts.entry(icrc1token_trade_contract).or_default();
    });
    
    let call_result: CallResult<Vec<u8>> = call_future.await;

    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(msg_cycles_refunded128());
    });
    
    match call_result {
        Ok(sponse_bytes) => match decode_one::<cm_tc::BuyTokensResult>(&sponse_bytes) {
            Ok(cm_buy_tokens_result) => {
                if let Ok(ref cm_buy_tokens_ok) = cm_buy_tokens_result {
                    with_mut(&CB_DATA, |cb_data| {
                        cb_data.user_data.cycles_transfers_out.push(
                            CyclesTransferOut{
                                id: new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter),
                                for_the_canister: icrc1token_trade_contract.trade_contract_canister_id,
                                cycles_sent: put_call_cycles,
                                cycles_refunded: Some(msg_cycles_refunded128()),   // None means the cycles_transfer-call-callback did not come back yet(did not give-back a reply-or-reject-sponse) 
                                cycles_transfer_memo: CyclesTransferMemo::Text(format!("cm-cycles-position-id: {}", cm_buy_tokens_ok.position_id)),
                                timestamp_nanos: time_nanos(),
                                opt_cycles_transfer_call_error: None,
                            }
                        );                        
                    });
                }
                Ok(cm_buy_tokens_result)
            },
            Err(candid_decode_error) => {
                return Err(CBTradeCyclesError::CMTradeCyclesCallSponseCandidDecodeError{candid_error: format!("{:?}", candid_decode_error), sponse_bytes: sponse_bytes });
            }
        },
        Err(call_error) => {
            return Err(CBTradeCyclesError::CMTradeCyclesCallError(call_error_as_u32_and_string(call_error)));
        }
    }
    
}



#[derive(CandidType)]
pub enum CBTradeTokensError {
    MemoryIsFull,
    CMTradeTokensCallError(CallError),
    CMTradeTokensCallSponseCandidDecodeError{candid_error: String, sponse_bytes: Vec<u8> },
}

type CBTradeTokensResult = Result<cm_tc::SellTokensResult, CBTradeTokensError>;

#[update]
pub async fn cm_trade_tokens(icrc1token_trade_contract: Icrc1TokenTradeContract, q: cm_tc::SellTokensQuest) -> CBTradeTokensResult {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
        
    with(&CB_DATA, |cb_data| { 
        if calculate_free_storage(cb_data) < 200 {
            return Err(CBTradeTokensError::MemoryIsFull);
        }
        Ok(())
    })?;    
    
    let mut call_future = with(&CB_DATA, |cb_data| {
        call_raw128(
            icrc1token_trade_contract.trade_contract_canister_id,
            "trade_tokens",
            encode_args((&q, (cb_data.user_id, &cb_data.cts_cb_authorization))).unwrap(),
            0
        )
    });
    
    if let futures::task::Poll::Ready(call_result_with_an_error) = futures::poll!(&mut call_future) {
        let call_error: (RejectionCode, String) = call_result_with_an_error.unwrap_err();
        return Err(CBTradeTokensError::CMTradeTokensCallError((call_error.0 as u32, "call_perform error".to_string())));
    }
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cm_trade_contracts.entry(icrc1token_trade_contract).or_default();
    });
    
    let call_result: CallResult<Vec<u8>> = call_future.await;
    
    match call_result {
        Ok(sponse_bytes) => match decode_one::<cm_tc::SellTokensResult>(&sponse_bytes) {
            Ok(cm_sell_tokens_result) => {
                Ok(cm_sell_tokens_result)
            },
            Err(candid_decode_error) => {
                return Err(CBTradeTokensError::CMTradeTokensCallSponseCandidDecodeError{candid_error: format!("{:?}", candid_decode_error), sponse_bytes: sponse_bytes });
            }
        },
        Err(call_error) => {
            return Err(CBTradeTokensError::CMTradeTokensCallError(call_error_as_u32_and_string(call_error)));
        }
    }
    
}







// ---------------------

#[derive(CandidType)]
pub enum UserCMVoidPositionError {
    CTSFuelTooLow,
    CyclesMarketVoidPositionCallError((u32, String)),
    CyclesMarketVoidPositionError(cm_icrc1token_trade_contract::VoidPositionError)
}


#[update]
pub async fn cm_void_position(icrc1token_trade_contract: Icrc1TokenTradeContract, q: cm_icrc1token_trade_contract::VoidPositionQuest) -> Result<(), UserCMVoidPositionError> {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    if with(&CB_DATA, |cb_data| { ctsfuel_balance(cb_data) }) < 30_000_000_000 {
        return Err(UserCMVoidPositionError::CTSFuelTooLow);
    }
    
    match call::<(cm_icrc1token_trade_contract::VoidPositionQuest,), (Result<(), cm_icrc1token_trade_contract::VoidPositionError>,)>(
        icrc1token_trade_contract.trade_contract_canister_id,
        "void_position",
        (q,)
    ).await {
        Ok((cm_void_position_result,)) => match cm_void_position_result {
            Ok(()) => Ok(()),
            Err(cm_void_position_error) => {
                return Err(UserCMVoidPositionError::CyclesMarketVoidPositionError(cm_void_position_error));
            }
        },
        Err(call_error) => {
            return Err(UserCMVoidPositionError::CyclesMarketVoidPositionCallError((call_error.0 as u32, call_error.1)));
        }
    
    }
    
}


// -------


#[derive(CandidType)]
pub enum UserCMTransferTokenBalanceError {
    CTSFuelTooLow,
    MemoryIsFull,
    CyclesMarketTransferTokenBalanceCallError((u32, String)),
    CyclesMarketTransferTokenBalanceCallSponseCandidDecodeError{candid_error: String, sponse_bytes: Vec<u8> },
    CyclesMarketTransferTokenBalanceError(cm_icrc1token_trade_contract::TransferTokenBalanceError)
}

#[update]
pub async fn cm_transfer_token_balance(icrc1token_trade_contract: Icrc1TokenTradeContract, q: cm_icrc1token_trade_contract::TransferTokenBalanceQuest) -> Result<BlockId, UserCMTransferTokenBalanceError> {
    if caller() != user_id() {
        trap("Caller must be the user for this method.");
    }
    
    if with(&CB_DATA, |cb_data| { ctsfuel_balance(cb_data) }) < 30_000_000_000 {
        return Err(UserCMTransferTokenBalanceError::CTSFuelTooLow);
    }

    let mut call_future = call_raw128(
        icrc1token_trade_contract.trade_contract_canister_id,
        "transfer_token_balance",
        encode_one(&q).unwrap(),
        0,
    );
    
    if let futures::task::Poll::Ready(call_result_with_an_error) = futures::poll!(&mut call_future) {
        let call_error: (RejectionCode, String) = call_result_with_an_error.unwrap_err();
        return Err(UserCMTransferTokenBalanceError::CyclesMarketTransferTokenBalanceCallError((call_error.0 as u32, "call_perform error".to_string())));
    }
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cm_trade_contracts.entry(icrc1token_trade_contract).or_default();
    });
    
    let call_result: CallResult<Vec<u8>> = call_future.await;


    match call_result {
        Ok(sponse_bytes) => match decode_one::<cm_icrc1token_trade_contract::TransferTokenBalanceResult>(&sponse_bytes) {
            Ok(cm_transfer_token_balance_result) => match cm_transfer_token_balance_result {
                Ok(block_height) => {
                    Ok(block_height)
                },
                Err(cm_transfer_token_balance_error) => {
                    return Err(UserCMTransferTokenBalanceError::CyclesMarketTransferTokenBalanceError(cm_transfer_token_balance_error));
                }
            },
            Err(candid_decode_error) => {
                return Err(UserCMTransferTokenBalanceError::CyclesMarketTransferTokenBalanceCallSponseCandidDecodeError{candid_error: format!("{:?}", candid_decode_error), sponse_bytes: sponse_bytes });                
            }
        },
        Err(call_error) => {
            return Err(UserCMTransferTokenBalanceError::CyclesMarketTransferTokenBalanceCallError((call_error.0 as u32, call_error.1)));
        }
    }

}



// -------------------------------

fn get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data: &mut CBData) -> &mut CMTradeContractLogs {
    let caller: Principal = caller();
    cb_data.user_data.cm_trade_contracts
        .iter_mut()
        .find(|(k,_v): &(&Icrc1TokenTradeContract, &mut CMTradeContractLogs)| {
            k.trade_contract_canister_id == caller
        })
        .map(|(_k,v): (&Icrc1TokenTradeContract, &mut CMTradeContractLogs)| {
            v
        })
        .unwrap_or_else(|| trap("Unknown caller"))
}

#[update]
pub fn cm_message_cycles_position_purchase_positor(q: cm_icrc1token_trade_contract::CMCyclesPositionPurchasePositorMessageQuest) {
    
    with_mut(&CB_DATA, |cb_data| {
        get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data).cm_message_logs.cm_message_cycles_position_purchase_positor_logs.push(
            CMMessageCyclesPositionPurchasePositorLog{
                timestamp_nanos: time_nanos(),
                cm_message_cycles_position_purchase_positor_quest: q
            }
        );
    });

}

#[update]
pub fn cm_message_cycles_position_purchase_purchaser(q: cm_icrc1token_trade_contract::CMCyclesPositionPurchasePurchaserMessageQuest) {
    
    let cycles_purchase: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(cycles_purchase); 
        cb_data.user_data.cycles_transfers_in.push(
            CyclesTransferIn{
                id: new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter),
                by_the_canister: caller(),
                cycles: cycles_purchase,
                cycles_transfer_memo: CyclesTransferMemo::Text(format!("cm-sell-{}-{}", q.cycles_position_id, q.purchase_id)),
                timestamp_nanos: time_nanos()
            }
        );
        get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data).cm_message_logs.cm_message_cycles_position_purchase_purchaser_logs.push(
            CMMessageCyclesPositionPurchasePurchaserLog{
                timestamp_nanos: time_nanos(),
                cycles_purchase,
                cm_message_cycles_position_purchase_purchaser_quest: q
            }
        );
    });    

}

#[update]
pub fn cm_message_token_position_purchase_positor(q: cm_icrc1token_trade_contract::CMTokenPositionPurchasePositorMessageQuest) {
    
    let cycles_payment: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(cycles_payment); 
        cb_data.user_data.cycles_transfers_in.push(
            CyclesTransferIn{
                id: new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter),
                by_the_canister: caller(),
                cycles: cycles_payment,
                cycles_transfer_memo: CyclesTransferMemo::Text(format!("cm-sell-{}-{}", q.token_position_id, q.purchase_id)),
                timestamp_nanos: time_nanos()
            }
        );
        get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data).cm_message_logs.cm_message_token_position_purchase_positor_logs.push(
            CMMessageTokenPositionPurchasePositorLog{
                timestamp_nanos: time_nanos(),
                cycles_payment,
                cm_message_token_position_purchase_positor_quest: q
            }
        );
    });
    
}

#[update]
pub fn cm_message_token_position_purchase_purchaser(q: cm_icrc1token_trade_contract::CMTokenPositionPurchasePurchaserMessageQuest) {
    
    with_mut(&CB_DATA, |cb_data| {
        get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data).cm_message_logs.cm_message_token_position_purchase_purchaser_logs.push(
            CMMessageTokenPositionPurchasePurchaserLog{
                timestamp_nanos: time_nanos(),
                cm_message_token_position_purchase_purchaser_quest: q
            }
        );
    });
    
}

#[update]
pub fn cm_message_void_cycles_position_positor(q: cm_icrc1token_trade_contract::CMVoidCyclesPositionPositorMessageQuest) {
    
    let void_cycles: Cycles = msg_cycles_accept128(msg_cycles_available128());
        
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(void_cycles); 
        cb_data.user_data.cycles_transfers_in.push(
            CyclesTransferIn{
                id: new_cycles_transfer_id(&mut cb_data.cycles_transfers_id_counter),
                by_the_canister: caller(),
                cycles: void_cycles,
                cycles_transfer_memo: CyclesTransferMemo::Text(format!("cm-buy-void-{}", q.position_id)),
                timestamp_nanos: time_nanos()
            }
        );
        get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data).cm_message_logs.cm_message_void_cycles_position_positor_logs.push(
            CMMessageVoidCyclesPositionPositorLog{
                timestamp_nanos: time_nanos(),
                void_cycles,
                cm_message_void_cycles_position_positor_quest: q
            }
        );
    });

}

#[update]
pub fn cm_message_void_token_position_positor(q: cm_icrc1token_trade_contract::CMVoidTokenPositionPositorMessageQuest) {

    with_mut(&CB_DATA, |cb_data| {
        get_mut_cm_trade_contract_logs_of_the_cm_caller_or_trap(cb_data).cm_message_logs.cm_message_void_token_position_positor_logs.push(
            CMMessageVoidTokenPositionPositorLog{
                timestamp_nanos: time_nanos(),
                cm_message_void_token_position_positor_quest: q
            }
        );
    });


}






// ----------------------------------------------------------




#[derive(CandidType)]
pub struct UserUCMetrics<'a> {
    global_allocator_counter: u64,
    cycles_balance: Cycles,
    ctsfuel_balance: CTSFuel,
    storage_size_mib: u128,
    lifetime_termination_timestamp_seconds: u128,
    user_id: Principal,
    user_canister_creation_timestamp_nanos: u128,
    storage_usage: u128,
    cycles_transfers_id_counter: u128,
    cycles_transfers_in_len: u128,
    cycles_transfers_out_len: u128,
    cm_trade_contracts: Vec<&'a Icrc1TokenTradeContract>,   
    cts_cb_authorization: bool, 
}




#[query(manual_reply = true)]
pub fn metrics() { //-> UserUCMetrics {
    if caller() != user_id() && caller() != cts_id() {
        trap("Caller must be the user for this method.");
    }
    
    with(&CB_DATA, |cb_data| {
        reply::<(UserUCMetrics,)>((UserUCMetrics{
            global_allocator_counter: get_allocated_bytes_count() as u64,
            cycles_balance: cb_data.user_data.cycles_balance,
            ctsfuel_balance: ctsfuel_balance(cb_data),
            storage_size_mib: cb_data.storage_size_mib,
            lifetime_termination_timestamp_seconds: cb_data.lifetime_termination_timestamp_seconds,
            user_id: cb_data.user_id,
            user_canister_creation_timestamp_nanos: cb_data.user_canister_creation_timestamp_nanos,
            storage_usage: calculate_current_storage_usage(cb_data),
            cycles_transfers_id_counter: cb_data.cycles_transfers_id_counter,
            cycles_transfers_in_len: cb_data.user_data.cycles_transfers_in.len() as u128,
            cycles_transfers_out_len: cb_data.user_data.cycles_transfers_out.len() as u128,
            cm_trade_contracts: cb_data.user_data.cm_trade_contracts.keys().collect(),
            cts_cb_authorization: cb_data.cts_cb_authorization.len() != 0
        },));
    });
}





// --------------------------------------------------------



#[update]
pub async fn user_lengthen_membership_cb_cycles_payment(q: LengthenMembershipQuest, msg_cycles: Cycles) -> Result<Vec<u8>/*cts reply*/, CallError> {
    if with(&CB_DATA, |cb_data| caller() != cb_data.user_id ) {
        trap("Caller must be the user for this method");
    }
    
    let mut call_future = with(&CB_DATA, |cb_data| {
        if cb_data.user_data.cycles_balance < msg_cycles {
            trap(&format!(
                "current cycles-balance: {}T, msg_cycles: {}T",
                cb_data.user_data.cycles_balance / TRILLION,
                msg_cycles / TRILLION 
            ));
        }
        
        call_raw128(
            cb_data.cts_id,
            "lengthen_membership_cb_cycles_payment",
            encode_args((q, cb_data.user_id)).unwrap(),
            msg_cycles
        )
    });
    
    if let Poll::Ready(err_result) = poll!(&mut call_future) {
        return Err(call_error_as_u32_and_string(err_result.unwrap_err()));
    }
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_sub(msg_cycles);
    });
    
    let call_result: CallResult<Vec<u8>> = call_future.await;
    
    with_mut(&CB_DATA, |cb_data| {
        cb_data.user_data.cycles_balance = cb_data.user_data.cycles_balance.saturating_add(msg_cycles_refunded128());
    });
    
    call_result.map_err(|call_error| call_error_as_u32_and_string(call_error))
    
}





#[update]
pub fn cts_update_lifetime_termination_timestamp_seconds(new_lifetime_termination_timestamp_seconds: u128) {
    with_mut(&CB_DATA, |cb_data| {
        if [cb_data.cts_id, cb_data.cbsm_id].contains(&caller()) == false {
            trap("Caller not authorized");
        }
        cb_data.lifetime_termination_timestamp_seconds = new_lifetime_termination_timestamp_seconds; 
    });
}


// make pub fn for the user for the upload of the cb-auth. check the auth validity before cepting it. if valid auth is in the cb, no need to accept a new one.
#[update]
pub fn user_upload_cts_cb_authorization(auth: Vec<u8>) {
    if caller() != with(&CB_DATA, |cb_data| cb_data.user_id) {
        trap("caller not authorized");
    }
    // if current auth, trap,
    with_mut(&CB_DATA, |cb_data| {
        if cb_data.cts_cb_authorization.len() != 0 {
            trap("Already with an authorization.")
        }
        // check auth,
        if is_cts_cb_authorization_valid(
            cb_data.cts_id,
            UserAndCB{ user_id: caller(), cb_id: ic_cdk::api::id() },
            auth.clone(),
        ) == false {
            trap("Void-Authorization.");
        }
        cb_data.cts_cb_authorization = auth;
    });    
}


// anyone can call this method for the verification of the authentication of this cts-cycles-bank.
#[query(manual_reply = true)]
pub fn get_cts_cb_auth() { //-> (Principal/*UserId*/, Vec<u8>/*auth*/) {
    with(&CB_DATA, |cb_data| {
       reply(((cb_data.user_id, &cb_data.cts_cb_authorization),)); 
    });
}








#[update]
pub fn cts_set_stop_calls_flag(stop_calls_flag: bool) {
    if caller() != cts_id() {
        trap("Caller must be the CTS for this method.");
    }
    localkey::cell::set(&STOP_CALLS, stop_calls_flag);
}

#[query]
pub fn cts_see_stop_calls_flag() -> bool {
    if caller() != cts_id() {
        trap("Caller must be the CTS for this method.");
    }
    localkey::cell::get(&STOP_CALLS)
}




// -------------------------------------------------------------------------

#[derive(CandidType)]
pub struct CTSUCMetrics {
    canister_cycles_balance: Cycles,
    cycles_balance: Cycles,
    ctsfuel_balance: CTSFuel,
    wasm_memory_size_bytes: u128,
    stable_memory_size_bytes: u64,
    storage_size_mib: u128,
    lifetime_termination_timestamp_seconds: u128,
    user_id: Principal,
    user_canister_creation_timestamp_nanos: u128,
    cycles_transfers_id_counter: u128,
    cycles_transfers_out_len: u128,
    cycles_transfers_in_len: u128,
    memory_size_at_the_start: u128,
    storage_usage: u128,
    free_storage: u128,
}


#[query]
pub fn cts_see_metrics() -> CTSUCMetrics {
    if caller() != cts_id() {
        trap("Caller must be the CTS for this method.");
    }
    
    with(&CB_DATA, |cb_data| {
        CTSUCMetrics{
            canister_cycles_balance: canister_balance128(),
            cycles_balance: cb_data.user_data.cycles_balance,
            ctsfuel_balance: ctsfuel_balance(cb_data),
            wasm_memory_size_bytes: ( core::arch::wasm32::memory_size(0)*WASM_PAGE_SIZE_BYTES ) as u128,
            stable_memory_size_bytes: ic_cdk::api::stable::stable64_size() * WASM_PAGE_SIZE_BYTES as u64,
            storage_size_mib: cb_data.storage_size_mib,
            lifetime_termination_timestamp_seconds: cb_data.lifetime_termination_timestamp_seconds,
            user_id: cb_data.user_id,
            user_canister_creation_timestamp_nanos: cb_data.user_canister_creation_timestamp_nanos,
            cycles_transfers_id_counter: cb_data.cycles_transfers_id_counter,
            cycles_transfers_in_len: cb_data.user_data.cycles_transfers_in.len() as u128,
            cycles_transfers_out_len: cb_data.user_data.cycles_transfers_out.len() as u128,
            memory_size_at_the_start: localkey::cell::get(&MEMORY_SIZE_AT_THE_START) as u128,
            storage_usage: calculate_current_storage_usage(cb_data),
            free_storage: calculate_free_storage(cb_data)
        }
    })
}









