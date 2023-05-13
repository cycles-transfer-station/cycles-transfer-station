// make limit on how many on-the-market-positions each user can have at the same time. bc each user must be a canister, it helps stop "dos attack-attempts"

use std::{
    cell::{Cell, RefCell},
    collections::{HashSet}
};
use cts_lib::{
    tools::{
        localkey::{
            self,
            refcell::{with, with_mut}
        },
        user_icp_id,
        cycles_transform_tokens,
        tokens_transform_cycles,
        principal_token_subaccount,
        round_robin,
        time_nanos,
        time_nanos_u64,
        time_seconds,
        rchunk_data
    },
    consts::{
        MiB,
        WASM_PAGE_SIZE_BYTES,
        MANAGEMENT_CANISTER_ID,
        NANOS_IN_A_SECOND,
        SECONDS_IN_A_MINUTE,
        SECONDS_IN_AN_HOUR,
        SECONDS_IN_A_DAY
    },
    types::{
        Cycles,
        CyclesTransferMemo,
        CyclesTransferRefund,
        DownloadRChunkQuest,
        cycles_transferrer,
        management_canister,
        cycles_market::icrc1_token_trade_contract::*,
        cm_caller::*,
    },
    icrc::{
        IcrcId, 
        IcrcSub,
        ICRC_DEFAULT_SUBACCOUNT,
        IcrcMemo,
        Tokens,
        TokenTransferError,
        TokenTransferArg,
        BlockId,
        icrc1_transfer,
        icrc1_balance_of
    },
    ic_cdk::{
        self,
        api::{
            trap,
            caller,
            call::{
                call,
                call_with_payment128,
                call_raw128,
                reply,
                CallResult,
                msg_cycles_refunded128,
                msg_cycles_available128,
                msg_cycles_accept128,
                arg_data,
            },
            canister_balance128,
            /*
            stable::{
                stable64_write,
                stable64_size,
                stable64_read,
                stable64_grow
           }
           */
           
        },
        export::{
            Principal,
            candid::{
                self, 
                CandidType,
                Deserialize,
                utils::{encode_one, decode_one},
                error::Error as CandidError,
            }
        }
    },
    ic_cdk_macros::{
        update,
        query,
        init,
        pre_upgrade,
        post_upgrade
    }
};

use ic_stable_structures::{
    Memory,
    DefaultMemoryImpl, 
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    
};

use serde_bytes::ByteBuf;

// -------

mod types;
use types::*;

mod payouts;
use payouts::_do_payouts;



// ---------------











#[derive(CandidType, Deserialize)]
struct CMData {
    cts_id: Principal,
    cm_main_id: Principal,
    cm_caller: Principal,
    icrc1_token_ledger: Principal,
    icrc1_token_ledger_transfer_fee: Tokens,
    id_counter: u128,
    mid_call_user_token_balance_locks: HashSet<Principal>,
    cycles_positions: Vec<CyclesPosition>,
    token_positions: Vec<TokenPosition>,
    trade_logs: Vec<TradeLog>,
    void_cycles_positions: Vec<VoidCyclesPosition>,
    void_token_positions: Vec<VoidTokenPosition>,
    do_payouts_errors: Vec<(u32, String)>,
}

impl CMData {
    fn new() -> Self {
        Self {
            cts_id: Principal::from_slice(&[]),
            cm_main_id: Principal::from_slice(&[]),
            cm_caller: Principal::from_slice(&[]),
            icrc1_token_ledger: Principal::from_slice(&[]),
            icrc1_token_ledger_transfer_fee: 0,
            id_counter: 0,
            mid_call_user_token_balance_locks: HashSet::new(),
            cycles_positions: Vec::new(),
            token_positions: Vec::new(),
            trade_logs: Vec::new(),
            void_cycles_positions: Vec::new(),
            void_token_positions: Vec::new(),
            do_payouts_errors: Vec::new()
        }
    }
}





pub const CREATE_POSITION_FEE: Cycles = 50_000_000_000;
pub const PURCHASE_POSITION_FEE: Cycles = 50_000_000_000;

pub const TRANSFER_TOKEN_BALANCE_FEE: Cycles = 50_000_000_000;

pub const CYCLES_TRANSFERRER_TRANSFER_CYCLES_FEE: Cycles = 20_000_000_000;

pub const MAX_WAIT_TIME_NANOS_FOR_A_CM_CALLER_CALLBACK: u128 = NANOS_IN_A_SECOND * SECONDS_IN_AN_HOUR * 72;
pub const VOID_POSITION_MINIMUM_WAIT_TIME_SECONDS: u128 = SECONDS_IN_AN_HOUR * 1;


pub const MINIMUM_CYCLES_POSITION: Cycles = 1_000_000_000_000;

pub const MINIMUM_TOKEN_POSITION: Tokens = 1;



const CANISTER_NETWORK_MEMORY_ALLOCATION_MiB: usize = 500; // multiple of 10
const CANISTER_DATA_STORAGE_SIZE_MiB: usize = CANISTER_NETWORK_MEMORY_ALLOCATION_MiB / 2 - 20/*memory-size at the start [re]placement*/; 

const CYCLES_POSITIONS_MAX_STORAGE_SIZE_MiB: usize = CANISTER_DATA_STORAGE_SIZE_MiB / 6 * 1;
const MAX_CYCLES_POSITIONS: usize = CYCLES_POSITIONS_MAX_STORAGE_SIZE_MiB * MiB / std::mem::size_of::<CyclesPosition>();

const TOKEN_POSITIONS_MAX_STORAGE_SIZE_MiB: usize = CANISTER_DATA_STORAGE_SIZE_MiB / 6 * 1;
const MAX_TOKEN_POSITIONS: usize = TOKEN_POSITIONS_MAX_STORAGE_SIZE_MiB * MiB / std::mem::size_of::<TokenPosition>();

const TRADE_LOGS_MAX_STORAGE_SIZE_MiB: usize = CANISTER_DATA_STORAGE_SIZE_MiB / 6 * 2;
const MAX_TRADE_LOGS: usize = TRADE_LOGS_MAX_STORAGE_SIZE_MiB * MiB / std::mem::size_of::<TradeLog>();

const VOID_CYCLES_POSITIONS_MAX_STORAGE_SIZE_MiB: usize = CANISTER_DATA_STORAGE_SIZE_MiB / 6 * 1;
const MAX_VOID_CYCLES_POSITIONS: usize = VOID_CYCLES_POSITIONS_MAX_STORAGE_SIZE_MiB * MiB / std::mem::size_of::<VoidCyclesPosition>();

const VOID_TOKEN_POSITIONS_MAX_STORAGE_SIZE_MiB: usize = CANISTER_DATA_STORAGE_SIZE_MiB / 6 * 1;
const MAX_VOID_TOKEN_POSITIONS: usize = VOID_TOKEN_POSITIONS_MAX_STORAGE_SIZE_MiB * MiB / std::mem::size_of::<VoidTokenPosition>();


const DO_VOID_CYCLES_POSITIONS_CYCLES_PAYOUTS_CHUNK_SIZE: usize = 5;
const DO_VOID_TOKEN_POSITIONS_TOKEN_PAYOUTS_CHUNK_SIZE: usize = 5;
const DO_TRADE_LOGS_CYCLES_PAYOUTS_CHUNK_SIZE: usize = 10;
const DO_TRADE_LOGS_TOKEN_PAYOUTS_CHUNK_SIZE: usize = 10;


const CM_MESSAGE_METHOD_VOID_CYCLES_POSITION_POSITOR: &'static str       = "cm_message_void_cycles_position_positor";
const CM_MESSAGE_METHOD_VOID_TOKEN_POSITION_POSITOR: &'static str          = "cm_message_void_token_position_positor";
const CM_MESSAGE_METHOD_CYCLES_POSITION_PURCHASE_POSITOR: &'static str   = "cm_message_cycles_position_purchase_positor";
const CM_MESSAGE_METHOD_CYCLES_POSITION_PURCHASE_PURCHASER: &'static str = "cm_message_cycles_position_purchase_purchaser";
const CM_MESSAGE_METHOD_TOKEN_POSITION_PURCHASE_POSITOR: &'static str      = "cm_message_token_position_purchase_positor";
const CM_MESSAGE_METHOD_TOKEN_POSITION_PURCHASE_PURCHASER: &'static str    = "cm_message_token_position_purchase_purchaser";

const CMCALLER_CALLBACK_CYCLES_POSITION_PURCHASE_PURCHASER: &'static str = "cm_message_cycles_position_purchase_purchaser_cmcaller_callback";
const CMCALLER_CALLBACK_CYCLES_POSITION_PURCHASE_POSITOR: &'static str = "cm_message_cycles_position_purchase_positor_cmcaller_callback";
const CMCALLER_CALLBACK_TOKEN_POSITION_PURCHASE_PURCHASER: &'static str = "cm_message_token_position_purchase_purchaser_cmcaller_callback";
const CMCALLER_CALLBACK_TOKEN_POSITION_PURCHASE_POSITOR: &'static str = "cm_message_token_position_purchase_positor_cmcaller_callback";
const CMCALLER_CALLBACK_VOID_CYCLES_POSITION_POSITOR: &'static str = "cm_message_void_cycles_position_positor_cmcaller_callback";
const CMCALLER_CALLBACK_VOID_TOKEN_POSITION_POSITOR: &'static str = "cm_message_void_token_position_positor_cmcaller_callback";



const TOKEN_POSITION_PURCHASE_TOKEN_TRANSFER_MEMO: &[u8; 8] = b"CM-IPP-0";
const CYCLES_POSITION_PURCHASE_TOKEN_TRANSFER_MEMO: &[u8; 8] = b"CM-CPP-0";

const TRANSFER_TOKEN_BALANCE_MEMO: &[u8; 8] = b"CMTRNSFR";

const SEE_CYCLES_POSITIONS_CHUNK_SIZE: usize = 300;
const SEE_TOKEN_POSITIONS_CHUNK_SIZE: usize = 300;
const SEE_CYCLES_POSITIONS_PURCHASES_CHUNK_SIZE: usize = 300;
const SEE_TOKEN_POSITIONS_PURCHASES_CHUNK_SIZE: usize = 300;



const MAX_MID_CALL_USER_TOKEN_BALANCE_LOCKS: usize = 500;


const STABLE_MEMORY_HEADER_SIZE_BYTES: u64 = 1024;

const STABLE_MEMORY_ID_HEAP_SERIALIZATION: MemoryId = MemoryId::new(0);
const STABLE_MEMORY_ID_: MemoryId = MemoryId::new(1);
// think bout the position matches, maker is the position-positor, taker is the position-purchaser,
// perhaps log each trade as a whole, 

/*

struct TradeLog {
    positor: Principal, //maker
    purchaser: Principal, //taker
    tokens: Tokens,
    cycles: Cycles,
    rate: CyclesPerTokenRate,
    //but then how do we know whether the position is a cycles-position or a token-position & whether this is a cycles-position-purchase or a token-position-purchase?
    position_kind: PositionKind
}

enum PositionKind {
    Cycles,
    Token
}

*/




thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    
    static CM_DATA: RefCell<CMData> = RefCell::new(CMData::new()); 
    
    
    // not save through the upgrades
    static TOKEN_LEDGER_ID: Cell<Principal> = Cell::new(Principal::from_slice(&[]));
    static TOKEN_LEDGER_TRANSFER_FEE: Cell<Tokens> = Cell::new(0);
    static STOP_CALLS: Cell<bool> = Cell::new(false);
    static STATE_SNAPSHOT: RefCell<Vec<u8>> = RefCell::new(Vec::new());    
}


// -------------------------------------------------------------


#[derive(CandidType, Deserialize)]
struct CMInit {
    cts_id: Principal,
    cm_main_id: Principal,
    cm_caller: Principal,
    icrc1_token_ledger: Principal,
    icrc1_token_ledger_transfer_fee: Tokens,
} 

#[init]
fn init(cm_init: CMInit) {
    with_mut(&CM_DATA, |cm_data| { 
        cm_data.cts_id = cm_init.cts_id; 
        cm_data.cm_main_id = cm_init.cm_main_id; 
        cm_data.cm_caller = cm_init.cm_caller;
        cm_data.icrc1_token_ledger = cm_init.icrc1_token_ledger; 
        cm_data.icrc1_token_ledger_transfer_fee = cm_init.icrc1_token_ledger_transfer_fee;
    });
    
    localkey::cell::set(&TOKEN_LEDGER_ID, cm_init.icrc1_token_ledger);
    localkey::cell::set(&TOKEN_LEDGER_TRANSFER_FEE, cm_init.icrc1_token_ledger_transfer_fee);
} 


// -------------------------------------------------------------


fn create_state_snapshot() {
    let mut cm_data_candid_bytes: Vec<u8> = with(&CM_DATA, |cm_data| { encode_one(cm_data).unwrap() });
    cm_data_candid_bytes.shrink_to_fit();
    
    with_mut(&STATE_SNAPSHOT, |state_snapshot| {
        *state_snapshot = cm_data_candid_bytes; 
    });
}

fn load_state_snapshot_data() {
    
    let cm_data_of_the_state_snapshot: CMData = with(&STATE_SNAPSHOT, |state_snapshot| {
        match decode_one::<CMData>(state_snapshot) {
            Ok(cm_data) => cm_data,
            Err(_) => {
                trap("error decode of the state-snapshot CMData");
                /*
                let old_cm_data: OldCMData = decode_one::<OldCMData>(state_snapshot).unwrap();
                let cm_data: CMData = CMData{
                    cts_id: old_cm_data.cts_id,
                    cm_caller: old_cm_data.cm_caller,
                    id_counter: old_cm_data.id_counter,
                    mid_call_user_token_balance_locks: old_cm_data.mid_call_user_token_balance_locks,
                    cycles_positions: old_cm_data.cycles_positions,
                    token_positions: old_cm_data.token_positions,
                    cycles_positions_purchases: old_cm_data.cycles_positions_purchases,
                    token_positions_purchases: old_cm_data.token_positions_purchases,
                    void_cycles_positions: old_cm_data.void_cycles_positions,
                    void_token_positions: old_cm_data.void_token_positions,
                    do_payouts_errors: old_cm_data.do_payouts_errors
                };
                cm_data
                */
            }
        }
    });

    localkey::cell::set(&TOKEN_LEDGER_ID, cm_data_of_the_state_snapshot.icrc1_token_ledger);
    localkey::cell::set(&TOKEN_LEDGER_TRANSFER_FEE, cm_data_of_the_state_snapshot.icrc1_token_ledger_transfer_fee);

    with_mut(&CM_DATA, |cm_data| {
        *cm_data = cm_data_of_the_state_snapshot;    
    });
    
}

// -------------------------------------------------------------



fn get_heap_serialization_memory() -> VirtualMemory<DefaultMemoryImpl> {
    with(&MEMORY_MANAGER, |memory_manager| { memory_manager.get(STABLE_MEMORY_ID_HEAP_SERIALIZATION) })
}


#[pre_upgrade]
fn pre_upgrade() {
    
    create_state_snapshot();
    
    let heap_serialization_memory: VirtualMemory<DefaultMemoryImpl> = get_heap_serialization_memory();
    
    let current_stable_size_wasm_pages: u64 = heap_serialization_memory.size();
    let current_stable_size_bytes: u64 = current_stable_size_wasm_pages * WASM_PAGE_SIZE_BYTES as u64;

    with(&STATE_SNAPSHOT, |state_snapshot| {
        let want_stable_memory_size_bytes: u64 = STABLE_MEMORY_HEADER_SIZE_BYTES + 8/*len of the state_snapshot*/ + state_snapshot.len() as u64; 
        if current_stable_size_bytes < want_stable_memory_size_bytes {
            let grow_result: i64 = heap_serialization_memory.grow(((want_stable_memory_size_bytes - current_stable_size_bytes) / WASM_PAGE_SIZE_BYTES as u64) + 1);
            if grow_result == -1 {
                trap("failed to grow heap_serialization_memory");
            }
        }
        heap_serialization_memory.write(STABLE_MEMORY_HEADER_SIZE_BYTES, &((state_snapshot.len() as u64).to_be_bytes()));
        heap_serialization_memory.write(STABLE_MEMORY_HEADER_SIZE_BYTES + 8, state_snapshot);
    });
}

#[post_upgrade]
fn post_upgrade() {
    
    let heap_serialization_memory: VirtualMemory<DefaultMemoryImpl> = get_heap_serialization_memory();

    let mut state_snapshot_len_u64_be_bytes: [u8; 8] = [0; 8];
    heap_serialization_memory.read(STABLE_MEMORY_HEADER_SIZE_BYTES, &mut state_snapshot_len_u64_be_bytes);
    let state_snapshot_len_u64: u64 = u64::from_be_bytes(state_snapshot_len_u64_be_bytes); 
    
    with_mut(&STATE_SNAPSHOT, |state_snapshot| {
        *state_snapshot = vec![0; state_snapshot_len_u64.try_into().unwrap()]; 
        heap_serialization_memory.read(STABLE_MEMORY_HEADER_SIZE_BYTES + 8, state_snapshot);
    });
    
    load_state_snapshot_data();
} 


// -------------------------------------------------------------

#[no_mangle]
fn canister_inspect_message() {
    use cts_lib::ic_cdk::api::call::{method_name, accept_message};
    
    if [
        "create_cycles_position",
        "create_token_position",
        "purchase_cycles_position",
        "purchase_token_position",
        "void_position",
        "see_token_lock",
        "transfer_token_balance",
        "cm_message_cycles_position_purchase_purchaser_cmcaller_callback",
        "cm_message_cycles_position_purchase_positor_cmcaller_callback",
        "cm_message_token_position_purchase_purchaser_cmcaller_callback",
        "cm_message_token_position_purchase_positor_cmcaller_callback",
        "cm_message_void_cycles_position_positor_cmcaller_callback",
        "cm_message_void_token_position_positor_cmcaller_callback",
        "see_cycles_positions",
        "see_token_positions",
        "see_cycles_positions_purchases",
        "see_token_positions_purchases",
        "download_cycles_positions_rchunks",
        "download_token_positions_rchunks",
        "download_cycles_positions_purchases_rchunks",
        "download_token_positions_purchases_rchunks"
    ].contains(&&method_name()[..]) {
        trap("this method must be call by a canister or a query call.");
    }
    
    
    accept_message();    
}


// -------------------------------------------------------------


fn new_id(cm_data_id_counter: &mut u128) -> u128 {
    let id: u128 = cm_data_id_counter.clone();
    *(cm_data_id_counter) += 1;
    id
}


async fn token_transfer(q: TokenTransferArg) -> Result<Result<BlockId, TokenTransferError>, (u32, String)> {
    icrc1_transfer(localkey::cell::get(&TOKEN_LEDGER_ID), q).await
}

async fn token_balance(count_id: IcrcId) -> Result<Tokens, (u32, String)> {
    icrc1_balance_of(localkey::cell::get(&TOKEN_LEDGER_ID), count_id).await
}




async fn check_user_cycles_market_token_ledger_balance(user_id: &Principal) -> Result<Tokens, (u32, String)> {
    token_balance(
        IcrcId{
            owner: ic_cdk::api::id(),
            subaccount: Some(principal_token_subaccount(user_id))
        }
    ).await
}


fn check_user_token_balance_in_the_lock(cm_data: &CMData, user_id: &Principal) -> Tokens {
    cm_data.token_positions.iter()
        .filter(|token_position: &&TokenPosition| { token_position.positor == *user_id })
        .fold(0, |cummulator: Tokens, user_token_position: &TokenPosition| {
            cummulator + user_token_position.tokens + ( user_token_position.tokens / user_token_position.minimum_purchase * localkey::cell::get(&TOKEN_LEDGER_TRANSFER_FEE) )
        })
    +
    cm_data.trade_logs.iter()
        .filter(|tl: &&TradeLog| {
            tl.token_payout_payor() == *user_id && tl.token_payout_data.token_transfer.is_none() 
        })
        .fold(0, |cummulator: Tokens, user_trade_log_with_unpaid_tokens: &TradeLog| {
            cummulator + user_trade_log_with_unpaid_tokens.tokens + localkey::cell::get(&TOKEN_LEDGER_TRANSFER_FEE)
        })
}




// ---------------




fn move_complete_trades_into_the_stable_memory(cm_data: &mut CMData) {
    let mut move_items_into_the_stable_memory: Vec<TradeLog> = Vec::new();
    while cm_data.trade_logs.len() > 0 {
        if cm_data.trade_logs[0].can_move_into_the_stable_memory_for_the_long_term_storage() == true {
            move_items_into_the_stable_memory.push(cm_data.trade_logs.remove(0));
        } else {
            break; // bc want to save into the stable-memory in the correct sequence.
        }
    }    
    
    if move_items_into_the_stable_memory.len() > 0 {
        // append items onto the stable-vec of the trade-logs
    }
    
}



async fn do_payouts() {
    
    if with(&CM_DATA, |cm_data| {
        cm_data.void_cycles_positions.len() == 0
        && cm_data.void_token_positions.len() == 0
        && cm_data.trade_logs.len() == 0
    }) { return; }

    match call::<(),()>(
        ic_cdk::api::id(),
        "do_payouts_public_method",
        (),
    ).await {
        Ok(()) => {
            with_mut(&CM_DATA, |cm_data| {
                move_complete_trades_into_the_stable_memory(cm_data);
            });
        },
        Err(call_error) => {
            with_mut(&CM_DATA, |cm_data| {
                cm_data.do_payouts_errors.push((call_error.0 as u32, call_error.1));
            });
        }
    }
}

#[update]
pub async fn do_payouts_public_method() {
    if [ic_cdk::api::id(), with(&CM_DATA, |cm_data| { cm_data.cts_id })].contains(&caller()) == false {
        trap("caller without the authorization.");
    }
    
    _do_payouts().await;

}


// -------------------------------------------------------------
















// -------------------------------------------------------------


type CreateCyclesPositionResult = Result<CreateCyclesPositionSuccess, CreateCyclesPositionError>;


#[update(manual_reply = true)]
pub async fn create_cycles_position(q: CreateCyclesPositionQuest) { // -> CreateCyclesPositionResult {

    let positor: Principal = caller();

    let r: CreateCyclesPositionResult = create_cycles_position_(positor, q).await;
    
    reply::<(CreateCyclesPositionResult,)>((r,));
    
    do_payouts().await;
    return;
}


async fn create_cycles_position_(positor: Principal, q: CreateCyclesPositionQuest) -> CreateCyclesPositionResult {

    if q.minimum_purchase > q.cycles {
        return Err(CreateCyclesPositionError::MinimumPurchaseMustBeEqualOrLessThanTheCyclesPosition);
    }

    if q.cycles < MINIMUM_CYCLES_POSITION {
        return Err(CreateCyclesPositionError::MinimumCyclesPosition(MINIMUM_CYCLES_POSITION));
    }
    
    if q.minimum_purchase == 0 {
        return Err(CreateCyclesPositionError::MinimumPurchaseCannotBeZero);
    }

    if q.cycles % q.cycles_per_token_rate != 0 {
        return Err(CreateCyclesPositionError::CyclesMustBeAMultipleOfTheCyclesPerTokenRate);
    }

    if q.minimum_purchase % q.cycles_per_token_rate != 0 {
        return Err(CreateCyclesPositionError::MinimumPurchaseMustBeAMultipleOfTheCyclesPerTokenRate);
    }

    let msg_cycles_quirement: Cycles = CREATE_POSITION_FEE.saturating_add(q.cycles); 

    if msg_cycles_available128() < msg_cycles_quirement {
        return Err(CreateCyclesPositionError::MsgCyclesTooLow{ create_position_fee: CREATE_POSITION_FEE });
    }

    if canister_balance128().checked_add(msg_cycles_quirement).is_none() {
        return Err(CreateCyclesPositionError::CyclesMarketIsFull);
    }

    
    with_mut(&CM_DATA, |cm_data| {
        if cm_data.cycles_positions.len() >= MAX_CYCLES_POSITIONS {
            if cm_data.void_cycles_positions.len() >= MAX_VOID_CYCLES_POSITIONS {
                return Err(CreateCyclesPositionError::CyclesMarketIsBusy);
            }
            // new
            // :highest-cost-position of the cycles-positions.
            let (
                cycles_position_with_the_lowest_cycles_per_token_rate_cycles_positions_i,
                cycles_position_with_the_lowest_cycles_per_token_rate_ref
            ): (usize, &CyclesPosition) = { 
                cm_data.cycles_positions.iter()
                    .enumerate()
                    .min_by_key(|(_,cycles_position): &(usize,&CyclesPosition)| { cycles_position.cycles_per_token_rate })
                    .unwrap()
            };
            if q.cycles_per_token_rate > cycles_position_with_the_lowest_cycles_per_token_rate_ref.cycles_per_token_rate 
            && q.cycles >= cycles_position_with_the_lowest_cycles_per_token_rate_ref.cycles {
                // bump
                
                std::mem::drop(cycles_position_with_the_lowest_cycles_per_token_rate_ref);
                
                let cycles_position_lowest_cycles_per_token_rate: CyclesPosition = cm_data.cycles_positions.remove(cycles_position_with_the_lowest_cycles_per_token_rate_cycles_positions_i);
                
                let cycles_position_lowest_cycles_per_token_rate_void_cycles_positions_insertion_i = { 
                    cm_data.void_cycles_positions.binary_search_by_key(
                        &cycles_position_lowest_cycles_per_token_rate.id,
                        |void_cycles_position| { void_cycles_position.position_id }
                    ).unwrap_err()
                };
                cm_data.void_cycles_positions.insert(
                    cycles_position_lowest_cycles_per_token_rate_void_cycles_positions_insertion_i,
                    VoidCyclesPosition{
                        position_id:    cycles_position_lowest_cycles_per_token_rate.id,
                        positor:        cycles_position_lowest_cycles_per_token_rate.positor,
                        cycles:         cycles_position_lowest_cycles_per_token_rate.cycles,
                        cycles_payout_lock: false,
                        cycles_payout_data: CyclesPayoutData::new(),
                        timestamp_nanos: time_nanos()
                    }
                );
                Ok(())
            } else {
                return Err(CreateCyclesPositionError::CyclesMarketIsFull_MinimumRateAndMinimumCyclesPositionForABump{ minimum_rate_for_a_bump: cycles_position_with_the_lowest_cycles_per_token_rate_ref.cycles_per_token_rate + 1, minimum_cycles_position_for_a_bump: cycles_position_with_the_lowest_cycles_per_token_rate_ref.cycles });
            }
        } else {
            Ok(())
        }
    })?;
    
    let position_id: PositionId = with_mut(&CM_DATA, |cm_data| {
        let id: PositionId = new_id(&mut cm_data.id_counter); 
        cm_data.cycles_positions.push(
            CyclesPosition{
                id,   
                positor,
                cycles: q.cycles,
                minimum_purchase: q.minimum_purchase,
                cycles_per_token_rate: q.cycles_per_token_rate,
                timestamp_nanos: time_nanos(),
            }
        );
        id
    });
    
    msg_cycles_accept128(msg_cycles_quirement);

    Ok(CreateCyclesPositionSuccess{
        position_id
    })
    
}





// ------------------


type CreateTokenPositionResult = Result<CreateTokenPositionSuccess, CreateTokenPositionError>;


#[update(manual_reply = true)]
pub async fn create_token_position(q: CreateTokenPositionQuest) { // -> CreateTokenPositionResult {

    let positor: Principal = caller();

    let r: CreateTokenPositionResult = create_token_position_(positor, q).await;
    
    reply::<(CreateTokenPositionResult,)>((r,));
    
    do_payouts().await;
    return;

}

async fn create_token_position_(positor: Principal, q: CreateTokenPositionQuest) -> CreateTokenPositionResult {

    if q.minimum_purchase > q.tokens {
        return Err(CreateTokenPositionError::MinimumPurchaseMustBeEqualOrLessThanTheTokenPosition);
    }

    if q.tokens < MINIMUM_TOKEN_POSITION {
        return Err(CreateTokenPositionError::MinimumTokenPosition(MINIMUM_TOKEN_POSITION));
    }
    
    if q.minimum_purchase == 0 {
        return Err(CreateTokenPositionError::MinimumPurchaseCannotBeZero);
    }


    let msg_cycles_quirement: Cycles = CREATE_POSITION_FEE; 

    if msg_cycles_available128() < msg_cycles_quirement {
        return Err(CreateTokenPositionError::MsgCyclesTooLow{ create_position_fee: CREATE_POSITION_FEE  });
    }

    if canister_balance128().checked_add(msg_cycles_quirement).is_none() {
        return Err(CreateTokenPositionError::CyclesMarketIsFull);
    }

    
    with_mut(&CM_DATA, |cm_data| {
        if cm_data.mid_call_user_token_balance_locks.len() >= MAX_MID_CALL_USER_TOKEN_BALANCE_LOCKS {
            return Err(CreateTokenPositionError::CyclesMarketIsBusy);
        }
        if cm_data.mid_call_user_token_balance_locks.contains(&positor) {
            return Err(CreateTokenPositionError::CallerIsInTheMiddleOfACreateTokenPositionOrPurchaseCyclesPositionOrTransferTokenBalanceCall);
        }
        cm_data.mid_call_user_token_balance_locks.insert(positor);
        Ok(())
    })?;
    
    // check token balance and make sure to unlock the user on returns after here 
    let user_token_ledger_balance: Tokens = match check_user_cycles_market_token_ledger_balance(&positor).await {
        Ok(token_ledger_balance) => token_ledger_balance,
        Err(call_error) => {
            with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&positor); });
            return Err(CreateTokenPositionError::CheckUserCyclesMarketTokenLedgerBalanceError((call_error.0 as u32, call_error.1)));
        }
    };
    
    let user_token_balance_in_the_lock: Tokens = with(&CM_DATA, |cm_data| { check_user_token_balance_in_the_lock(cm_data, &positor) });
    
    let usable_user_token_balance: Tokens = user_token_ledger_balance.saturating_sub(user_token_balance_in_the_lock);
    
    if usable_user_token_balance < q.tokens + ( q.tokens / q.minimum_purchase * localkey::cell::get(&TOKEN_LEDGER_TRANSFER_FEE) ) {
        with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&positor); });
        return Err(CreateTokenPositionError::UserTokenBalanceTooLow{ user_token_balance: usable_user_token_balance });
    }
    
    
    with_mut(&CM_DATA, |cm_data| {
        if cm_data.token_positions.len() >= MAX_TOKEN_POSITIONS {            
            if cm_data.void_token_positions.len() >= MAX_VOID_TOKEN_POSITIONS {
                cm_data.mid_call_user_token_balance_locks.remove(&positor);
                return Err(CreateTokenPositionError::CyclesMarketIsBusy);
            }
            // new
            let (
                token_position_with_the_highest_cycles_per_token_rate_token_positions_i,
                token_position_with_the_highest_cycles_per_token_rate_ref
            ): (usize, &TokenPosition) = {
                cm_data.token_positions.iter()
                    .enumerate()
                    .max_by_key(|(_, token_position): &(usize, &TokenPosition)| { token_position.cycles_per_token_rate })
                    .unwrap()
            };
            if q.cycles_per_token_rate < token_position_with_the_highest_cycles_per_token_rate_ref.cycles_per_token_rate 
            && q.tokens >= token_position_with_the_highest_cycles_per_token_rate_ref.tokens {
                // bump
                
                std::mem::drop(token_position_with_the_highest_cycles_per_token_rate_ref);
                
                let token_position_highest_cycles_per_token_rate: TokenPosition = cm_data.token_positions.remove(token_position_with_the_highest_cycles_per_token_rate_token_positions_i);                
                
                let token_position_highest_cycles_per_token_rate_void_token_positions_insertion_i = { 
                    cm_data.void_token_positions.binary_search_by_key(
                        &token_position_highest_cycles_per_token_rate.id,
                        |void_token_position| { void_token_position.position_id }
                    ).unwrap_err()
                };
                cm_data.void_token_positions.insert(
                    token_position_highest_cycles_per_token_rate_void_token_positions_insertion_i,
                    VoidTokenPosition{                        
                        position_id:    token_position_highest_cycles_per_token_rate.id,
                        positor:        token_position_highest_cycles_per_token_rate.positor,
                        tokens:         token_position_highest_cycles_per_token_rate.tokens,
                        timestamp_nanos: time_nanos(),
                        token_payout_lock: false,
                        token_payout_data: TokenPayoutData{
                            token_transfer: Some(TokenTransferBlockHeightAndTimestampNanos{
                                block_height: None,
                                timestamp_nanos: time_nanos(),
                            }),
                            cm_message_call_success_timestamp_nanos: None,
                            cm_message_callback_complete: None            
                        }
                    }
                ); 
                Ok(())
            } else {
                cm_data.mid_call_user_token_balance_locks.remove(&positor);
                return Err(CreateTokenPositionError::CyclesMarketIsFull_MaximumRateAndMinimumTokenPositionForABump{ maximum_rate_for_a_bump: token_position_with_the_highest_cycles_per_token_rate_ref.cycles_per_token_rate - 1, minimum_token_position_for_a_bump: token_position_with_the_highest_cycles_per_token_rate_ref.tokens });
            }
        } else {
            Ok(())    
        }
    })?;
    
    let position_id: PositionId = with_mut(&CM_DATA, |cm_data| {
        let id: PositionId = new_id(&mut cm_data.id_counter); 
        cm_data.token_positions.push(
            TokenPosition{
                id,   
                positor,
                tokens: q.tokens,
                minimum_purchase: q.minimum_purchase,
                cycles_per_token_rate: q.cycles_per_token_rate,
                timestamp_nanos: time_nanos(),
            }
        );
        id
    });
    
    msg_cycles_accept128(msg_cycles_quirement);

    with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&positor); });
    
    Ok(CreateTokenPositionSuccess{
        position_id
    })
}


// ------------------


type PurchaseCyclesPositionResult = Result<PurchaseCyclesPositionSuccess, PurchaseCyclesPositionError>;


#[update(manual_reply = true)]
pub async fn purchase_cycles_position(q: PurchaseCyclesPositionQuest) { // -> PurchaseCyclesPositionResult
    
    let purchaser: Principal = caller();
    
    let r: PurchaseCyclesPositionResult = purchase_cycles_position_(purchaser, q).await;
    
    reply::<(PurchaseCyclesPositionResult,)>((r,));

    do_payouts().await;
    return;
}

async fn purchase_cycles_position_(purchaser: Principal, q: PurchaseCyclesPositionQuest) -> PurchaseCyclesPositionResult {

    if msg_cycles_available128() < PURCHASE_POSITION_FEE {
        return Err(PurchaseCyclesPositionError::MsgCyclesTooLow{ purchase_position_fee: PURCHASE_POSITION_FEE });
    }
    
    with_mut(&CM_DATA, |cm_data| {
        if cm_data.mid_call_user_token_balance_locks.len() >= MAX_MID_CALL_USER_TOKEN_BALANCE_LOCKS {
            return Err(PurchaseCyclesPositionError::CyclesMarketIsBusy);
        }
        if cm_data.mid_call_user_token_balance_locks.contains(&purchaser) {
            return Err(PurchaseCyclesPositionError::CallerIsInTheMiddleOfACreateTokenPositionOrPurchaseCyclesPositionOrTransferTokenBalanceCall);
        }
        cm_data.mid_call_user_token_balance_locks.insert(purchaser);
        Ok(())
    })?;
    
    // check token balance and make sure to unlock the user on returns after here 
    let user_token_ledger_balance: Tokens = match check_user_cycles_market_token_ledger_balance(&purchaser).await {
        Ok(token_ledger_balance) => token_ledger_balance,
        Err(call_error) => {
            with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&purchaser); });
            return Err(PurchaseCyclesPositionError::CheckUserCyclesMarketTokenLedgerBalanceError((call_error.0 as u32, call_error.1)));            
        }
    };
    
    let user_token_balance_in_the_lock: Tokens = with(&CM_DATA, |cm_data| { check_user_token_balance_in_the_lock(cm_data, &purchaser) });
    
    let usable_user_token_balance: Tokens = user_token_ledger_balance.saturating_sub(user_token_balance_in_the_lock);

    let cycles_position_purchase_id: PurchaseId = match with_mut(&CM_DATA, |cm_data| {
        if cm_data.trade_logs.len() >= MAX_TRADE_LOGS {
            return Err(PurchaseCyclesPositionError::CyclesMarketIsBusy);
        }
        let cycles_position_cycles_positions_i: usize = match cm_data.cycles_positions.binary_search_by_key(
            &q.cycles_position_id,
            |cycles_position| { cycles_position.id }
        ) {
            Ok(i) => i,
            Err(_) => { return Err(PurchaseCyclesPositionError::CyclesPositionNotFound); }
        };
        let cycles_position_ref: &CyclesPosition = &cm_data.cycles_positions[cycles_position_cycles_positions_i];
        if q.cycles % cycles_position_ref.cycles_per_token_rate as u128 != 0 {
            return Err(PurchaseCyclesPositionError::PurchaseCyclesMustBeAMultipleOfTheCyclesPerTokenRate);
        }
        if cycles_position_ref.cycles < q.cycles {
            return Err(PurchaseCyclesPositionError::CyclesPositionCyclesIsLessThanThePurchaseQuest{ cycles_position_cycles: cycles_position_ref.cycles });
        }
        if cycles_position_ref.minimum_purchase > q.cycles {
            return Err(PurchaseCyclesPositionError::CyclesPositionMinimumPurchaseIsGreaterThanThePurchaseQuest{ cycles_position_minimum_purchase: cycles_position_ref.minimum_purchase });
        }        
        
        if usable_user_token_balance < cycles_transform_tokens(q.cycles, cycles_position_ref.cycles_per_token_rate) + localkey::cell::get(&TOKEN_LEDGER_TRANSFER_FEE) {
            return Err(PurchaseCyclesPositionError::UserTokenBalanceTooLow{ user_token_balance: usable_user_token_balance });
        }
        
        if cycles_position_ref.cycles - q.cycles < cycles_position_ref.minimum_purchase 
        && cycles_position_ref.cycles - q.cycles != 0
        && cm_data.void_cycles_positions.len() >= MAX_VOID_CYCLES_POSITIONS {
            return Err(PurchaseCyclesPositionError::CyclesMarketIsBusy);
        }
                
        let cycles_position_purchase_id: PurchaseId = new_id(&mut cm_data.id_counter);
        cm_data.trade_logs.push(
            TradeLog{
                position_id: cycles_position_ref.id,
                id: cycles_position_purchase_id,
                positor: cycles_position_ref.positor, 
                purchaser, 
                tokens: cycles_transform_tokens(q.cycles, cycles_position_ref.cycles_per_token_rate),
                cycles: q.cycles,
                rate: cycles_position_ref.cycles_per_token_rate,
                position_kind: PositionKind::Cycles,
                timestamp_nanos: time_nanos(),
                cycles_payout_lock: false,
                token_payout_lock: false,
                cycles_payout_data: CyclesPayoutData::new(),
                token_payout_data: TokenPayoutData{
                    token_transfer: None,
                    cm_message_call_success_timestamp_nanos: None,
                    cm_message_callback_complete: None    
                }
            }
        );

        std::mem::drop(cycles_position_ref);
        cm_data.cycles_positions[cycles_position_cycles_positions_i].cycles -= q.cycles;
        if cm_data.cycles_positions[cycles_position_cycles_positions_i].cycles < cm_data.cycles_positions[cycles_position_cycles_positions_i].minimum_purchase {
            let cycles_position_for_the_void: CyclesPosition = cm_data.cycles_positions.remove(cycles_position_cycles_positions_i);
            if cycles_position_for_the_void.cycles != 0 {
                let cycles_position_for_the_void_void_cycles_positions_insertion_i: usize = { 
                    cm_data.void_cycles_positions.binary_search_by_key(
                        &cycles_position_for_the_void.id,
                        |void_cycles_position| { void_cycles_position.position_id }
                    ).unwrap_err()
                };
                cm_data.void_cycles_positions.insert(
                    cycles_position_for_the_void_void_cycles_positions_insertion_i,
                    VoidCyclesPosition{
                        position_id:    cycles_position_for_the_void.id,
                        positor:        cycles_position_for_the_void.positor,
                        cycles:         cycles_position_for_the_void.cycles,
                        cycles_payout_lock: false,
                        cycles_payout_data: CyclesPayoutData::new(),
                        timestamp_nanos: time_nanos()
                    }
                );
            }
        }   
        
        Ok(cycles_position_purchase_id)
    }) {
        Ok(cycles_position_purchase_id) => cycles_position_purchase_id,
        Err(purchase_cycles_position_error) => {
            with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&purchaser); });
            return Err(purchase_cycles_position_error);
        }
    };
    
    msg_cycles_accept128(PURCHASE_POSITION_FEE);

    with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&purchaser); });
    
    Ok(PurchaseCyclesPositionSuccess{
        purchase_id: cycles_position_purchase_id
    })

}



// -------------------



type PurchaseTokenPositionResult = Result<PurchaseTokenPositionSuccess, PurchaseTokenPositionError>;


#[update(manual_reply = true)]
pub async fn purchase_token_position(q: PurchaseTokenPositionQuest) { // -> PurchaseTokenPositionResult 

    let purchaser: Principal = caller();
    
    let r: PurchaseTokenPositionResult = purchase_token_position_(purchaser, q).await;

    reply::<(PurchaseTokenPositionResult,)>((r,));
    
    do_payouts().await;
    return;
}

async fn purchase_token_position_(purchaser: Principal, q: PurchaseTokenPositionQuest) -> PurchaseTokenPositionResult {

    let token_position_purchase_id: PurchaseId = with_mut(&CM_DATA, |cm_data| {
        if cm_data.trade_logs.len() >= MAX_TRADE_LOGS {
            return Err(PurchaseTokenPositionError::CyclesMarketIsBusy);            
        }
        let token_position_token_positions_i: usize = match cm_data.token_positions.binary_search_by_key(
            &q.token_position_id,
            |token_position| { token_position.id }
        ) {
            Ok(i) => i,
            Err(_) => { return Err(PurchaseTokenPositionError::TokenPositionNotFound); }
        };
        let token_position_ref: &TokenPosition = &cm_data.token_positions[token_position_token_positions_i];
        if token_position_ref.tokens < q.tokens {
            return Err(PurchaseTokenPositionError::TokenPositionTokenIsLessThanThePurchaseQuest{ token_position_tokens: token_position_ref.tokens.clone() });
        }
        if token_position_ref.minimum_purchase > q.tokens {
            return Err(PurchaseTokenPositionError::TokenPositionMinimumPurchaseIsGreaterThanThePurchaseQuest{ token_position_minimum_purchase: token_position_ref.minimum_purchase.clone() });
        }        

        if &token_position_ref.tokens - &q.tokens < token_position_ref.minimum_purchase 
        && token_position_ref.tokens - q.tokens != 0
        && cm_data.void_token_positions.len() >= MAX_VOID_TOKEN_POSITIONS {
            return Err(PurchaseTokenPositionError::CyclesMarketIsBusy);
        }
        

        let msg_cycles_quirement: Cycles = PURCHASE_POSITION_FEE + tokens_transform_cycles(q.tokens, token_position_ref.cycles_per_token_rate); 
        if msg_cycles_available128() < msg_cycles_quirement {
            return Err(PurchaseTokenPositionError::MsgCyclesTooLow{ purchase_position_fee: PURCHASE_POSITION_FEE });
        }
        msg_cycles_accept128(msg_cycles_quirement);
                
        let token_position_purchase_id: PurchaseId = new_id(&mut cm_data.id_counter);
        
        cm_data.trade_logs.push(
            TradeLog{
                position_id: token_position_ref.id,
                id: token_position_purchase_id,
                positor: token_position_ref.positor,
                purchaser,
                tokens: q.tokens,
                cycles: tokens_transform_cycles(q.tokens, token_position_ref.cycles_per_token_rate),
                rate: token_position_ref.cycles_per_token_rate,
                position_kind: PositionKind::Token,
                timestamp_nanos: time_nanos(),
                cycles_payout_lock: false,
                token_payout_lock: false,
                cycles_payout_data: CyclesPayoutData::new(),
                token_payout_data: TokenPayoutData{
                    token_transfer: None,
                    cm_message_call_success_timestamp_nanos: None,
                    cm_message_callback_complete: None    
                }
            }
        );

        std::mem::drop(token_position_ref);
        cm_data.token_positions[token_position_token_positions_i].tokens -= q.tokens;
        if cm_data.token_positions[token_position_token_positions_i].tokens < cm_data.token_positions[token_position_token_positions_i].minimum_purchase {            
            let token_position_for_the_void: TokenPosition = cm_data.token_positions.remove(token_position_token_positions_i);
            if token_position_for_the_void.tokens != 0 {
                let token_position_for_the_void_void_token_positions_insertion_i: usize = { 
                    cm_data.void_token_positions.binary_search_by_key(
                        &token_position_for_the_void.id,
                        |void_token_position| { void_token_position.position_id }
                    ).unwrap_err()
                };
                cm_data.void_token_positions.insert(
                    token_position_for_the_void_void_token_positions_insertion_i,
                    VoidTokenPosition{
                        position_id:    token_position_for_the_void.id,
                        positor:        token_position_for_the_void.positor,
                        tokens:            token_position_for_the_void.tokens,
                        timestamp_nanos: time_nanos(),
                        token_payout_lock: false,
                        token_payout_data: TokenPayoutData{
                            token_transfer: Some(TokenTransferBlockHeightAndTimestampNanos{
                                block_height: None,
                                timestamp_nanos: time_nanos(),
                            }),
                            cm_message_call_success_timestamp_nanos: None,
                            cm_message_callback_complete: None            
                        }
                    }
                );
            }    
            
        }
        
        Ok(token_position_purchase_id)
    })?;
    
    Ok(PurchaseTokenPositionSuccess{
        purchase_id: token_position_purchase_id
    })
    
}




// --------------------------



#[update(manual_reply = true)]
pub async fn void_position(q: VoidPositionQuest) { // -> VoidPositionResult
    let caller: Principal = caller();
    
    let r: VoidPositionResult = void_position_(caller, q);
    
    reply::<(VoidPositionResult,)>((r,));
    
    do_payouts().await;
    return; 

}   
    
fn void_position_(caller: Principal, q: VoidPositionQuest) -> VoidPositionResult {
    
    with_mut(&CM_DATA, |cm_data| {
        if let Ok(cycles_position_i) = cm_data.cycles_positions.binary_search_by_key(&q.position_id, |cycles_position| { cycles_position.id }) {
            if cm_data.cycles_positions[cycles_position_i].positor != caller {
                return Err(VoidPositionError::WrongCaller);
            }
            if time_seconds().saturating_sub(cm_data.cycles_positions[cycles_position_i].timestamp_nanos/NANOS_IN_A_SECOND) < VOID_POSITION_MINIMUM_WAIT_TIME_SECONDS {
                return Err(VoidPositionError::MinimumWaitTime{ minimum_wait_time_seconds: VOID_POSITION_MINIMUM_WAIT_TIME_SECONDS, position_creation_timestamp_seconds: cm_data.cycles_positions[cycles_position_i].timestamp_nanos/NANOS_IN_A_SECOND });
            }  
            if cm_data.void_cycles_positions.len() >= MAX_VOID_CYCLES_POSITIONS {
                return Err(VoidPositionError::CyclesMarketIsBusy);
            }
            let cycles_position_for_the_void: CyclesPosition = cm_data.cycles_positions.remove(cycles_position_i);
            let cycles_position_for_the_void_void_cycles_positions_insertion_i: usize = cm_data.void_cycles_positions.binary_search_by_key(&cycles_position_for_the_void.id, |vcp| { vcp.position_id }).unwrap_err();
            cm_data.void_cycles_positions.insert(
                cycles_position_for_the_void_void_cycles_positions_insertion_i,
                VoidCyclesPosition{
                    position_id:    cycles_position_for_the_void.id,
                    positor:        cycles_position_for_the_void.positor,
                    cycles:         cycles_position_for_the_void.cycles,
                    cycles_payout_lock: false,
                    cycles_payout_data: CyclesPayoutData::new(),
                    timestamp_nanos: time_nanos()                
                }
            );
            Ok(())
        } else if let Ok(token_position_i) = cm_data.token_positions.binary_search_by_key(&q.position_id, |token_position| { token_position.id }) {
            if cm_data.token_positions[token_position_i].positor != caller {
                return Err(VoidPositionError::WrongCaller);
            }
            if time_seconds().saturating_sub(cm_data.token_positions[token_position_i].timestamp_nanos/NANOS_IN_A_SECOND) < VOID_POSITION_MINIMUM_WAIT_TIME_SECONDS {
                return Err(VoidPositionError::MinimumWaitTime{ minimum_wait_time_seconds: VOID_POSITION_MINIMUM_WAIT_TIME_SECONDS, position_creation_timestamp_seconds: cm_data.token_positions[token_position_i].timestamp_nanos/NANOS_IN_A_SECOND });
            }
            if cm_data.void_token_positions.len() >= MAX_VOID_TOKEN_POSITIONS {
                return Err(VoidPositionError::CyclesMarketIsBusy);
            }
            let token_position_for_the_void: TokenPosition = cm_data.token_positions.remove(token_position_i);
            let token_position_for_the_void_void_token_positions_insertion_i: usize = cm_data.void_token_positions.binary_search_by_key(&token_position_for_the_void.id, |vip| { vip.position_id }).unwrap_err();
            cm_data.void_token_positions.insert(
                token_position_for_the_void_void_token_positions_insertion_i,
                VoidTokenPosition{
                    position_id:    token_position_for_the_void.id,
                    positor:        token_position_for_the_void.positor,
                    tokens:            token_position_for_the_void.tokens,
                    timestamp_nanos: time_nanos(),
                    token_payout_lock: false,
                    token_payout_data: TokenPayoutData{
                        token_transfer: Some(TokenTransferBlockHeightAndTimestampNanos{
                            block_height: None,
                            timestamp_nanos: time_nanos(),
                        }),
                        cm_message_call_success_timestamp_nanos: None,
                        cm_message_callback_complete: None            
                    }
                }
            );
            Ok(())
        } else {
            return Err(VoidPositionError::PositionNotFound);
        }
    })
    
}


// ----------------

#[derive(CandidType, Deserialize)]
pub struct SeeTokenLockQuest {
    principal_id: Principal,
}

#[query]
pub fn see_token_lock(q: SeeTokenLockQuest) -> Tokens {
    with(&CM_DATA, |cm_data| { check_user_token_balance_in_the_lock(cm_data, &q.principal_id) })
}


// ----------------


#[update(manual_reply = true)]
pub async fn transfer_token_balance(q: TransferTokenBalanceQuest) { // -> TransferTokenBalanceResult {
    
    let user_id: Principal = caller();
    
    let r: TransferTokenBalanceResult = transfer_token_balance_(user_id, q).await;
    
    reply::<(TransferTokenBalanceResult,)>((r,));
    
    do_payouts().await;
    return;
}    
    
async fn transfer_token_balance_(user_id: Principal, q: TransferTokenBalanceQuest) -> TransferTokenBalanceResult {
    
    if msg_cycles_available128() < TRANSFER_TOKEN_BALANCE_FEE {
        return Err(TransferTokenBalanceError::MsgCyclesTooLow{ transfer_token_balance_fee: TRANSFER_TOKEN_BALANCE_FEE });
    }

    match with_mut(&CM_DATA, |cm_data| {
        if cm_data.mid_call_user_token_balance_locks.len() >= MAX_MID_CALL_USER_TOKEN_BALANCE_LOCKS {
            return Err(TransferTokenBalanceError::CyclesMarketIsBusy);
        }
        if cm_data.mid_call_user_token_balance_locks.contains(&user_id) {
            return Err(TransferTokenBalanceError::CallerIsInTheMiddleOfACreateTokenPositionOrPurchaseCyclesPositionOrTransferTokenBalanceCall);
        }
        cm_data.mid_call_user_token_balance_locks.insert(user_id);
        Ok(())
    }) {
        Ok(()) => {},
        Err(transfer_token_balance_error) => {
            return Err(transfer_token_balance_error);
        }
    }
    
    // check token balance and make sure to unlock the user on returns after here 
    let user_token_ledger_balance: Tokens = match check_user_cycles_market_token_ledger_balance(&user_id).await {
        Ok(token_ledger_balance) => token_ledger_balance,
        Err(call_error) => {
            with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&user_id); });
            return Err(TransferTokenBalanceError::CheckUserCyclesMarketTokenLedgerBalanceCallError((call_error.0 as u32, call_error.1)));            
        }
    };
    
    let user_token_balance_in_the_lock: Tokens = with(&CM_DATA, |cm_data| { check_user_token_balance_in_the_lock(cm_data, &user_id) });
    
    let usable_user_token_balance: Tokens = user_token_ledger_balance.saturating_sub(user_token_balance_in_the_lock);
    
    if usable_user_token_balance < q.tokens.saturating_add(q.token_fee) {
        with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&user_id); });
        return Err(TransferTokenBalanceError::UserTokenBalanceTooLow{ user_token_balance: usable_user_token_balance });          
    }

    
    let token_transfer_result = token_transfer(
        TokenTransferArg {
            memo: Some(IcrcMemo(ByteBuf::from(*TRANSFER_TOKEN_BALANCE_MEMO))),
            amount: q.tokens.into(),
            fee: Some(q.token_fee.into()),
            from_subaccount: Some(principal_token_subaccount(&user_id)),
            to: q.to,
            created_at_time: Some(q.created_at_time.unwrap_or(time_nanos_u64()))
        }   
    ).await;
    
    with_mut(&CM_DATA, |cm_data| { cm_data.mid_call_user_token_balance_locks.remove(&user_id); });

    match token_transfer_result {
        Ok(token_transfer_result) => match token_transfer_result {
            Ok(token_transfer_block_height) => {
                msg_cycles_accept128(TRANSFER_TOKEN_BALANCE_FEE);
                return Ok(token_transfer_block_height);
            },
            Err(token_transfer_error) => {
                match token_transfer_error {
                    TokenTransferError::BadFee{ .. } => {
                        msg_cycles_accept128(TRANSFER_TOKEN_BALANCE_FEE);
                    },
                    _ => {}
                }
                return Err(TransferTokenBalanceError::TokenTransferError(token_transfer_error));
            }
        },
        Err(token_transfer_call_error) => {
            return Err(TransferTokenBalanceError::TokenTransferCallError(token_transfer_call_error));
        }
    }

}



// -------------------------------

#[query(manual_reply = true)]
pub fn download_cycles_positions_rchunks(q: DownloadRChunkQuest) {
    with(&CM_DATA, |cm_data| {
        reply((rchunk_data(q, &cm_data.cycles_positions),));
    });
}

#[query(manual_reply = true)]
pub fn download_token_positions_rchunks(q: DownloadRChunkQuest) {
    with(&CM_DATA, |cm_data| {
        reply((rchunk_data(q, &cm_data.token_positions),));
    });
}




// ----------------------



#[derive(CandidType, Deserialize)]
pub struct SeeCyclesPositionsQuest {
    chunk_i: u128
}

#[query(manual_reply = true)]
pub fn see_cycles_positions(q: SeeCyclesPositionsQuest) {
    with(&CM_DATA, |cm_data| {
        reply::<(Option<&[CyclesPosition]>,)>((
            cm_data.cycles_positions.chunks(SEE_CYCLES_POSITIONS_CHUNK_SIZE).nth(q.chunk_i as usize)
        ,));
    });
}

#[derive(CandidType, Deserialize)]
pub struct SeeTokenPositionsQuest {
    chunk_i: u128
}

#[query(manual_reply = true)]
pub fn see_token_positions(q: SeeTokenPositionsQuest) {
    with(&CM_DATA, |cm_data| {
        reply::<(Option<&[TokenPosition]>,)>((
            cm_data.token_positions.chunks(SEE_TOKEN_POSITIONS_CHUNK_SIZE).nth(q.chunk_i as usize)
        ,));
    });
}





// ---------------------------------


#[update(manual_reply = true)]
pub async fn trigger_payouts() {
    reply::<()>(());
    do_payouts().await;
    return;
}


// -------------------------------------------------------------


#[update(manual_reply = true)]
pub async fn cm_message_cycles_position_purchase_purchaser_cmcaller_callback(q: CMCallbackQuest) -> () {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cm_caller }) {
        trap("this method is for the cycles_market-caller");
    }

    let cycles_transfer_refund: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    with_mut(&CM_DATA, |cm_data| {
        if let Ok(cycles_position_purchase_cycles_positions_purchases_i) = cm_data.trade_logs.binary_search_by_key(&q.cm_call_id, |cycles_position_purchase| { cycles_position_purchase.id }) {
            cm_data.trade_logs[cycles_position_purchase_cycles_positions_purchases_i]
            .cycles_payout_data
            .cmcaller_cycles_payout_callback_complete = Some((cycles_transfer_refund, q.opt_call_error));
        }
    });

    reply::<()>(());
    do_payouts().await;
    return;
}

#[update(manual_reply = true)]
pub async fn cm_message_cycles_position_purchase_positor_cmcaller_callback(q: CMCallbackQuest) {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cm_caller }) {
        trap("this method is for the cycles_market-caller");
    }
    
    with_mut(&CM_DATA, |cm_data| {
        if let Ok(cycles_position_purchase_cycles_positions_purchases_i) = cm_data.trade_logs.binary_search_by_key(&q.cm_call_id, |cycles_position_purchase| { cycles_position_purchase.id }) {
            cm_data.trade_logs[cycles_position_purchase_cycles_positions_purchases_i]
            .token_payout_data
            .cm_message_callback_complete = Some(q.opt_call_error);
        }
    });
    
    reply::<()>(());
    do_payouts().await;
    return;
}

#[update(manual_reply = true)]
pub async fn cm_message_token_position_purchase_purchaser_cmcaller_callback(q: CMCallbackQuest) {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cm_caller }) {
        trap("this method is for the cycles_market-caller");
    }
        
    with_mut(&CM_DATA, |cm_data| {
        if let Ok(token_position_purchase_token_positions_purchases_i) = cm_data.trade_logs.binary_search_by_key(&q.cm_call_id, |token_position_purchase| { token_position_purchase.id }) {
            cm_data.trade_logs[token_position_purchase_token_positions_purchases_i]
            .token_payout_data
            .cm_message_callback_complete = Some(q.opt_call_error);
        }
    });
    
    reply::<()>(());
    do_payouts().await;
    return;
}

#[update(manual_reply = true)]
pub async fn cm_message_token_position_purchase_positor_cmcaller_callback(q: CMCallbackQuest) {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cm_caller }) {
        trap("this method is for the cycles_market-caller");
    }

    let cycles_transfer_refund: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    with_mut(&CM_DATA, |cm_data| {
        if let Ok(token_position_purchase_token_positions_purchases_i) = cm_data.trade_logs.binary_search_by_key(&q.cm_call_id, |token_position_purchase| { token_position_purchase.id }) {
            cm_data.trade_logs[token_position_purchase_token_positions_purchases_i]
            .cycles_payout_data
            .cmcaller_cycles_payout_callback_complete = Some((cycles_transfer_refund, q.opt_call_error));
        }
    });
    
    reply::<()>(());
    do_payouts().await;
    return;
}

#[update(manual_reply = true)]
pub async fn cm_message_void_cycles_position_positor_cmcaller_callback(q: CMCallbackQuest) {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cm_caller }) {
        trap("this method is for the cycles_market-caller");
    }
    
    let cycles_transfer_refund: Cycles = msg_cycles_accept128(msg_cycles_available128());
    
    with_mut(&CM_DATA, |cm_data| {
        if let Ok(void_cycles_position_void_cycles_positions_i) = cm_data.void_cycles_positions.binary_search_by_key(&q.cm_call_id, |void_cycles_position| { void_cycles_position.position_id }) {
            cm_data.void_cycles_positions[void_cycles_position_void_cycles_positions_i]
            .cycles_payout_data
            .cmcaller_cycles_payout_callback_complete = Some((cycles_transfer_refund, q.opt_call_error));
        }
    });
    
    reply::<()>(());
    do_payouts().await;
    return;
}

#[update(manual_reply = true)]
pub async fn cm_message_void_token_position_positor_cmcaller_callback(q: CMCallbackQuest) {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cm_caller }) {
        trap("this method is for the cycles_market-caller");
    }

    with_mut(&CM_DATA, |cm_data| {
        if let Ok(void_token_position_void_token_positions_i) = cm_data.void_token_positions.binary_search_by_key(&q.cm_call_id, |void_token_position| { void_token_position.position_id }) {
            cm_data.void_token_positions[void_token_position_void_token_positions_i]
            .token_payout_data
            .cm_message_callback_complete = Some(q.opt_call_error);
        }
    });
    
    reply::<()>(());
    do_payouts().await;
    return;
}





// -------------------------------------------------------------


fn caller_is_cts_check() {
    if caller() != with(&CM_DATA, |cm_data| { cm_data.cts_id }) {
        trap("Caller must be the CTS for this method.");
    }
}

#[update]
pub fn cts_set_stop_calls_flag(stop_calls_flag: bool) {
    caller_is_cts_check();
    
    localkey::cell::set(&STOP_CALLS, stop_calls_flag);
}

#[query]
pub fn cts_see_stop_calls_flag() -> bool {
    caller_is_cts_check();
    
    localkey::cell::get(&STOP_CALLS)
}





#[update]
pub fn cts_create_state_snapshot() -> u64/*len of the state_snapshot*/ {
    caller_is_cts_check();
    
    create_state_snapshot();
    
    with(&STATE_SNAPSHOT, |state_snapshot| {
        state_snapshot.len() as u64
    })
}





#[export_name = "canister_query cts_download_state_snapshot"]
pub fn cts_download_state_snapshot() {
    caller_is_cts_check();
    
    let chunk_size: usize = 1 * MiB as usize;
    with(&STATE_SNAPSHOT, |state_snapshot| {
        let (chunk_i,): (u64,) = arg_data::<(u64,)>(); // starts at 0
        reply::<(Option<&[u8]>,)>((state_snapshot.chunks(chunk_size).nth(chunk_i as usize),));
    });
}



#[update]
pub fn cts_clear_state_snapshot() {
    caller_is_cts_check();
    
    with_mut(&STATE_SNAPSHOT, |state_snapshot| {
        *state_snapshot = Vec::new();
    });    
}

#[update]
pub fn cts_append_state_snapshot(mut append_bytes: Vec<u8>) {
    caller_is_cts_check();
    
    with_mut(&STATE_SNAPSHOT, |state_snapshot| {
        state_snapshot.append(&mut append_bytes);
    });
}

#[update]
pub fn cts_load_state_snapshot_data() {
    caller_is_cts_check();
    
    load_state_snapshot_data();
}



// -------------------------------------------------------------



#[query(manual_reply = true)]
pub fn cts_see_payouts_errors(chunk_i: u32) {
    caller_is_cts_check();
    
    with(&CM_DATA, |cm_data| {
        reply::<(Option<&[(u32, String)]>,)>((cm_data.do_payouts_errors.chunks(100).nth(chunk_i as usize),));
    });
}



#[update]
pub fn cts_clear_payouts_errors() {
    caller_is_cts_check();
    
    with_mut(&CM_DATA, |cm_data| {
        cm_data.do_payouts_errors = Vec::new();
    });    
}



