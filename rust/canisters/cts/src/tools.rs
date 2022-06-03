use std::collections::HashMap;
// use sha2::Digest;
use ic_cdk::{
    api::{
        id,
        time,
        trap,
        call::{
            CallResult,
            call_raw128,
            call,
        },
    },
    export::{
        Principal,
        candid::{
            CandidType,
            Deserialize,
            utils::{encode_one, decode_one},
        },
    }
};

use crate::{
    USERS_DATA,
    NEW_CANISTERS,
    LATEST_KNOWN_CMC_RATE,
    MAX_USERS_MAP_CANISTERS
    LatestKnownCmcRate
};

use cts_lib::{
    types::{
        UserLock,
        UserData,
        Cycles,
        UsersMapCanisterInit,
        UserId,
        UserCanisterId
    },
    consts::{
        MANAGEMENT_CANISTER_ID
    },
    tools::{
        sha256,
        localkey_refcell::{self, with, with_mut}
    },
    ic_ledger_types::{
        IcpId,
        IcpIdSub,
        IcpTokens,
        IcpAccountBalanceArgs,
        IcpBlockHeight,
        IcpTimestamp,
        IcpTransferArgs,
        icp_transfer,
        IcpTransferResult,
        IcpTransferError,
        IcpMemo,
        icp_account_balance,
        ICP_DEFAULT_SUBACCOUNT,
        MAINNET_LEDGER_CANISTER_ID,
        MAINNET_CYCLES_MINTING_CANISTER_ID,
        ICP_LEDGER_TRANSFER_DEFAULT_FEE
    }
};





pub const ICP_LEDGER_CREATE_CANISTER_MEMO: IcpMemo = IcpMemo(0x41455243); // == 'CREA'
pub const ICP_LEDGER_TOP_UP_CANISTER_MEMO: IcpMemo = IcpMemo(0x50555054); // == 'TPUP'
pub const DEFAULT_CYCLES_PER_XDR: u128 = 1_000_000_000_000; // 1T cycles = 1 XDR



pub const CYCLES_BALANCE_TOPUP_MEMO_START: &'static [u8] = b"TP";




fn principal_as_thirty_bytes(p: &Principal) -> [u8; 30] {
    let mut bytes: [u8; 30] = [0; 30];
    let p_bytes: &[u8] = p.as_slice();
    bytes[0] = p_bytes.len() as u8; 
    bytes[1 .. p_bytes.len() + 1].copy_from_slice(p_bytes); 
    bytes
}

pub fn thirty_bytes_as_principal(bytes: &[u8; 30]) -> Principal {
    Principal::from_slice(&bytes[1..1 + bytes[0] as usize])
} 



pub fn principal_icp_subaccount(user: &Principal) -> IcpIdSub {
    let mut sub_bytes = [0u8; 32];
    sub_bytes[..30].copy_from_slice(&principal_as_thirty_bytes(user));
    IcpIdSub(sub_bytes)
}



pub fn user_icp_balance_id(user: &Principal) -> IcpId {
    IcpId::new(&id(), &principal_icp_subaccount(user))
}


pub fn check_user_icp_ledger_balance(user_id: &Principal) -> CallResult<IcpTokens> {
    icp_account_balance(
        MAINNET_LEDGER_CANISTER_ID,
        IcpAccountBalanceArgs { account: user_icp_balance_id(user_id) }    
    ).await
}

pub async fn check_user_icp_balance(user: &Principal) -> CallResult<IcpTokens> {
    let mut icp_balance: IcpTokens = check_user_icp_ledger_balance(user).await?;
    with(&USERS_DATA, |ud| { 
        if let Some(u) = ud.get(user) {
            *&mut icp_balance -= u.untaken_icp_to_collect;
        } 
    });
    //icp_balance -= USERS_DATA.with(|ud| { ud.borrow_mut().entry(*user).or_default().untaken_icp_to_collect });
    Ok(icp_balance)
}


pub async fn take_user_icp_ledger(user_id: &Principal, icp: IcpTokens) -> CallResult<IcpTransferResult> {
    icp_transfer(
        MAINNET_LEDGER_CANISTER_ID,
        IcpTransferArgs {
            memo: ICP_FEE_MEMO,
            amount: icp,
            fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE,
            from_subaccount: Some(principal_icp_subaccount(user_id)),
            to: main_cts_icp_id(),                        
            created_at_time: Some(IcpTimestamp { timestamp_nanos: time() })
        }
    ).await
}



pub fn user_cycles_balance_topup_memo_bytes(user: &Principal) -> [u8; 32] {
    let mut memo_bytes = [0u8; 32];
    memo_bytes[..2].copy_from_slice(CYCLES_BALANCE_TOPUP_MEMO_START);
    memo_bytes[2..].copy_from_slice(&principal_as_thirty_bytes(user));
    memo_bytes
}


#[derive(CandidType, Deserialize)]
pub enum CheckUserCyclesBalanceError {
    FindUserError(FindUserError),
}

pub async fn check_user_cycles_balance(user: &Principal) -> Result<Cycles, CheckUserCyclesBalanceError> {
    with(&USERS_DATA, |users_data| {
        if let Some(u) = users_data.get(user) {
            Ok(u.cycles_balance)
        } else {
            Ok(0)
        }
        //ud.borrow_mut().entry(*user).or_default().cycles_balance
    })
}


pub fn main_cts_icp_id() -> IcpId {  // do once
    IcpId::new(&id(), &ICP_DEFAULT_SUBACCOUNT)
}


pub fn check_lock_and_lock_user(user: &Principal) {
    USERS_DATA.with(|ud| {
        let users_data: &mut HashMap<Principal, UserData> = &mut ud.borrow_mut();
        let user_lock: &mut UserLock = &mut users_data.entry(*user).or_default().user_lock;
        let current_time: u64 = time();
        if user_lock.lock == true && current_time - user_lock.last_lock_time_nanos < 30*60*1_000_000_000 {
            trap("this user is in the middle of a different call");
        }
        user_lock.lock = true;
        user_lock.last_lock_time_nanos = current_time;
    });
}

pub fn unlock_user(user: &Principal) {
    USERS_DATA.with(|ud| {
        ud.borrow_mut().get_mut(user).unwrap().user_lock.lock = false;
    });
}


#[derive(CandidType, Deserialize)]
pub enum CheckCurrentXdrPerMyriadPerIcpCmcRateError {
    CmcGetRateCallError(String),
    CmcGetRateCallSponseCandidError(String),
}

#[derive(CandidType, Deserialize)]
struct IcpXdrConversionRateCertifiedResponse {
    certificate: Vec<u8>, 
    data : IcpXdrConversionRate,
    hash_tree : Vec<u8>
}

#[derive(CandidType, Deserialize, Copy, Clone)]
pub struct IcpXdrConversionRate {
    pub xdr_permyriad_per_icp : u64,
    pub timestamp_seconds : u64
}

pub type CheckCurrentXdrPerMyriadPerIcpCmcRateSponse = Result<u64, CheckCurrentXdrPerMyriadPerIcpCmcRateError>;

// how many 1/10000-xdr for one icp
pub async fn check_current_xdr_permyriad_per_icp_cmc_rate() -> CheckCurrentXdrPerMyriadPerIcpCmcRateSponse {

    let latest_known_cmc_rate: LatestKnownCmcRate = LATEST_KNOWN_CMC_RATE.with(|r| { r.get() }); 
    if time() / 1_000_000_000 - latest_known_cmc_rate.timestamp_seconds < 10*60 {
        return Ok(latest_known_cmc_rate.xdr_permyriad_per_icp);
    }
    
    let call_sponse_candid_bytes: Vec<u8> = match call_raw128(
        MAINNET_CYCLES_MINTING_CANISTER_ID,
        "get_icp_xdr_conversion_rate",
        &encode_one(()).unwrap(),
        0
    ).await {
        Ok(b) => b,
        Err(call_error) => return Err(CheckCurrentXdrPerMyriadPerIcpCmcRateError::CmcGetRateCallError(format!("{:?}", call_error)))
    };
    let icp_xdr_conversion_rate: IcpXdrConversionRate = match decode_one::<IcpXdrConversionRateCertifiedResponse>(&call_sponse_candid_bytes) {
        Ok(s) => s.data,
        Err(candid_error) => return Err(CheckCurrentXdrPerMyriadPerIcpCmcRateError::CmcGetRateCallSponseCandidError(format!("{}", candid_error))),
    };

    LATEST_KNOWN_CMC_RATE.with(|r| { r.set(icp_xdr_conversion_rate); });
    Ok(icp_xdr_conversion_rate.xdr_permyriad_per_icp)
}



pub fn icptokens_to_cycles(icpts: IcpTokens, xdr_permyriad_per_icp: u64) -> u128 {
    icpts.e8s() as u128 
    * xdr_permyriad_per_icp as u128 
    * DEFAULT_CYCLES_PER_XDR 
    / (IcpTokens::SUBDIVIDABLE_BY as u128 * 10_000)
}

pub fn cycles_to_icptokens(cycles: u128, xdr_permyriad_per_icp: u64) -> IcpTokens {
    IcpTokens::from_e8s(
        ( cycles
        * (IcpTokens::SUBDIVIDABLE_BY as u128 * 10_000)
        / DEFAULT_CYCLES_PER_XDR
        / xdr_permyriad_per_icp as u128 ) as u64    
    )
}









#[derive(CandidType, Deserialize)]
pub struct ManagementCanisterCreateCanisterQuest {
    settings : Option<ManagementCanisterOptionalCanisterSettings>
}

#[derive(CandidType, Deserialize)]
pub struct ManagementCanisterOptionalCanisterSettings {
    pub controllers : Option<Vec<Principal>>,
    pub compute_allocation : Option<u128>,
    pub memory_allocation : Option<u128>,
    pub freezing_threshold : Option<u128>,
}

#[derive(CandidType, Deserialize)]
pub struct ManagementCanisterCanisterSettings {
    pub controllers : Vec<Principal>,
    pub compute_allocation : u128,
    pub memory_allocation : u128,
    pub freezing_threshold : u128
}

#[derive(CandidType, Deserialize)]
pub struct ManagementCanisterCanisterStatusRecord {
    pub status : ManagementCanisterCanisterStatusVariant,
    pub settings: ManagementCanisterCanisterSettings,
    pub module_hash: Option<[u8; 32]>,
    pub memory_size: u128,
    pub cycles: u128
}

#[derive(CandidType, Deserialize, PartialEq)]
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


#[derive(CandidType, Deserialize)]
pub enum GetNewCanisterError {
    CreateCanisterManagementCallQuestCandidError(String),
    CreateCanisterManagementCallSponseCandidError{candid_error: String, candid_bytes: Vec<u8>},
    CreateCanisterManagementCallError(String)
}

pub async fn get_new_canister(optional_canister_settings: Option<ManagementCanisterOptionalCanisterSettings>, with_cycles: Cycles) -> Result<Principal, GetNewCanisterError> {
    
    if let Some(principal) = with_mut(&NEW_CANISTERS, |new_canisters| new_canisters.pop()) {
        // get status
        
        // make sure is empty

        // make sure is running 
        
        // update settings if different
        
        // put cycles (or take cycles? later.) if not enough
        
        // if any of the above fail , create with the create_canister function and put the canister back into the new-canisters
    
        return Ok(principal);
    } 

    let create_canister_management_call_quest_candid_bytes: Vec<u8> = match encode_one(&ManagementCanisterCreateCanisterQuest { settings: optional_canister_settings }) {
        Ok(candid_bytes) => candid_bytes,
        Err(candid_error) => {
            return Err(GetNewCanisterError::CreateCanisterManagementCallQuestCandidError(format!("{}", candid_error)));
        }
    };

    let canister_id: Principal = match CallResult<Vec<u8>> = call_raw128(
        MANAGEMENT_CANISTER_PRINCIPAL,
        "create_canister",
        &create_canister_management_call_quest_candid_bytes,
        100_000_000_000/*create_canister-cost*/ + with_cycles
    ).await {
        Ok(call_sponse_candid_bytes) => match decode_one::<CanisterIdRecord>(&call_sponse_candid_bytes) {
            Ok(canister_id_record) => canister_id_record.canister_id,
            Err(candid_error) => {
                return Err(GetNewCanisterError::CreateCanisterManagementCallSponseCandidError{ candid_error: format!("{}", candid_error), candid_bytes: call_sponse_candid_bytes });
            }
        },
        Err(call_error) => {
            return Err(GetNewCanisterError::CreateCanisterManagementCallError(format!("{:?}", call_error)));
        }
    };
    
    // are new canisters running?

    Ok(canister_id)

}













#[derive(CandidType, Deserialize)]
pub struct CmcNotifyCreateCanisterQuest {
    block_index: IcpBlockHeight,
    controller: Principal,
}


#[derive(CandidType, Deserialize)]
struct NotifyTopUpArg {
    block_index: IcpBlockHeight,
    canister_id: Principal,
}

#[derive(CandidType, Deserialize)]
pub enum CmcNotifyError {
    Refunded { block_index: Option<IcpBlockHeight>, reason: String },
    InvalidTransaction(String),
    Other{ error_message: String, error_code: u64 },
    Processing,
    TransactionTooOld(IcpBlockHeight),
}

type NotifyTopUpResult = Result<Cycles, NotifyError>;



#[derive(CandidType, Deserialize)]
pub enum LedgerTopupCyclesError {
    IcpTransferCallError(String),
    IcpTransferError(IcpTransferError),
    CmcNotifyTopUpQuestCandidEncodeError{candid_error: String, topup_transfer_block_height: IcpBlockHeight },
    CmcNotifySponseCandidDecodeError{candid_error: String, candid_bytes: Vec<u8>, topup_transfer_block_height: IcpBlockHeight },
    CmcNotifyCallError{notify_call_error: String, topup_transfer_block_height: IcpBlockHeight },
    CmcNotifyError{cmc_notify_error: CmcNotifyError, topup_transfer_block_height: IcpBlockHeight},
}

// make a public method to re-try a block-height
pub async fn ledger_topup_cycles(icp: IcpTokens, from_subaccount: Option<IcpIdSub>, topup_canister: Principal) -> Result<Cycles, LedgerTopupCyclesError> {

    let topup_cycles_icp_transfer_call_block_index: IcpBlockHeight = match icp_transfer(
        MAINNET_LEDGER_CANISTER_ID,
        IcpTransferArgs {
            memo: ICP_TOP_UP_CANISTER_MEMO,
            amount: icp,                              
            fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE,
            from_subaccount: from_subaccount,
            to: IcpId::new(&MAINNET_CYCLES_MINTING_CANISTER_ID, &principal_icp_subaccount(&topup_canister)),
            created_at_time: Some(IcpTimestamp { timestamp_nanos: time() })
        }
    ).await {
        Ok(transfer_call_sponse) => match transfer_call_sponse {
            Ok(block_index) => block_index,
            Err(transfer_error) => {
                return Err(LedgerTopupCyclesError::IcpTransferError(transfer_error));
            }
        },
        Err(transfer_call_error) => {
            return Err(LedgerTopupCyclesError::IcpTransferCallError(format!("{:?}", transfer_call_error)));
        }
    };

    // :give-back: topup_cycles_icp_transfer_call_block_index on each follow[ing]-error-case.

    let topup_cycles_cmc_notify_call_candid: Vec<u8> = match encode_one(
        & NotifyTopUpArg {
            block_index: topup_cycles_icp_transfer_call_block_index,
            canister_id: topup_canister
        }
    ) {
        Ok(b) => b,
        Err(candid_error) => {
            return Err(LedgerTopupCyclesError::CmcNotifyTopUpQuestCandidEncodeError { candid_error: format!("{}", candid_error), topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index });
        }
    };

    let cycles: Cycles = match call_raw128(
        MAINNET_CYCLES_MINTING_CANISTER_ID,
        "notify_top_up",
        &topup_cycles_cmc_notify_call_candid,
        0
    ).await {
        Ok(candid_bytes) => match decode_one::<NotifyTopUpResult>(&candid_bytes) {
            Ok(notify_topup_result) => match notify_topup_result {
                Ok(cycles) => cycles,
                Err(cmc_notify_error) => {
                    return Err(LedgerTopupCyclesError::CmcNotifyError{cmc_notify_error: cmc_notify_error, topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index});
                }
            },
            Err(candid_error) => {
                return Err(LedgerTopupCyclesError::CmcNotifySponseCandidDecodeError { candid_error: format!("{}", candid_error), candid_bytes: candid_bytes, topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index });
            }
        },
        Err(notify_call_error) => {
            return Err(LedgerTopupCyclesError::CmcNotifyCallError { notify_call_error: format!("{:?}", notify_call_error), topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index });
        }
    };

    Ok(cycles)
}




/*
#[derive(CandidType, Deserialize)]
pub enum LedgerCreateCanisterError {
    IcpTransferCallError(String),
    IcpTransferError(IcpTransferError),
    CmcNotifyCallError{call_error: String, block_height: IcpBlockHeight},   //create_canister_icp_transfer_block_height    
    CmcNotifyError{error: CmcNotifyError, block_height: IcpBlockHeight}
}

pub async fn ledger_create_canister(icp: IcpTokens, from_subaccount: Option<IcpIdSub>, controller: Principal) -> Result<Principal, LedgerCreateCanisterError> {

}
*/










#[derive(CandidType, Deserialize)]
pub enum PutNewUserIntoAUsersMapCanisterError {
    UsersMapCanisterPutNewUserCallFail(UsersMapCanisterId,(RejectionCode, String)), // principal of the failiing users-map-canister
    UsersMapCanisterPutNewUserError(UsersMapCanisterPutNewUserError),
    CreateNewUsersMapCanisterError(CreateNewUsersMapCanisterError),

    
    
}

// this function as of now does not check if the user exists already in one of the users-map-canisters. use the find_user-function for that.
pub async fn put_new_user_into_a_users_map_canister(user_id: UserId, user_canister_id: UserCanisterId) -> Result<UsersMapCanisterId, PutNewUserIntoAUsersMapCanisterError> {
    
    for i in (0..with(&USERS_MAP_CANISTERS, |umcs| umcs.len())).rev() {
        let umc_id:Principal = with(&USERS_MAP_CANISTERS, |umcs| umcs[i]);
        match call<(Result<(), UsersMapCanisterPutNewUserError>,)>(
            umc_id,
            "put_new_user",
            (user_id, user_canister_id),
        ).await {
            Ok(users_map_canister_put_new_user_sponse) => match users_map_canister_put_new_user_sponse {
                Ok(_) => return Ok(umc_id),
                Err(users_map_canister_put_new_user_error) => match users_map_canister_put_new_user_error {
                    UsersMapCanisterPutNewUserError::CanisterIsFull => continue,
                    _ => return Err(PutNewUserIntoAUsersMapCanisterError::UsersMapCanisterPutNewUserError(users_map_canister_put_new_user_error)) /*error can be that the user is already in the canister. the new_user function should have locked the user and checked for the user first before calling this function.  */
                } 
            },
            Err(users_map_canister_put_new_user_call_fail) => return Err(PutNewUserIntoAUsersMapCanisterError::UsersMapCanisterPutNewUserCallFail(umc_id, users_map_canister_put_new_user_call_fail)),
        }
    }
    
    // if each users_map_canister is full,
    // create a new users_map_canister
    let umc_id: UsersMapCanisterId = match create_new_users_map_canister().await {
        Ok(new_users_map_canister_id) => new_users_map_canister_id,
        Err(create_new_users_map_canister_error) => return Err(PutNewUserIntoAUsersMapCanisterError::CreateNewUsersMapCanisterError(create_new_users_map_canister_error))
    };
    
    match call<(Result<(), UsersMapCanisterPutNewUserError>,)>(
        umc_id,
        "put_new_user",
        (user_id, user_canister_id),
    ).await {
        Ok(users_map_canister_put_new_user_sponse) => match users_map_canister_put_new_user_sponse {
            Ok(_) => return Ok(umc_id),
            Err(users_map_canister_put_new_user_error) => return Err(PutNewUserIntoAUsersMapCanisterError::UsersMapCanisterPutNewUserError(users_map_canister_put_new_user_error))
        },
        Err(users_map_canister_put_new_user_call_fail) => return Err(PutNewUserIntoAUsersMapCanisterError::UsersMapCanisterPutNewUserCallFail(umc_id, users_map_canister_put_new_user_call_fail)),
    }
    

}



pub enum CreateNewUsersMapCanisterError {
    MaxUsersMapCanisters,
    CreateNewUsersMapCanisterLockIsOn,
    GetNewCanisterError(GetNewCanisterError),
    UsersMapCanisterCodeNotFound,
    InstallCodeCallError(String)
}

pub async fn create_new_users_map_canister() -> Result<UsersMapCanisterId, CreateNewUsersMapCanisterError> {
    if with(&USERS_MAP_CANISTERS, |umcs| umcs.len()) >= MAX_USERS_MAP_CANISTERS {
        return Err(CreateNewUsersMapCanisterError::MaxUsersMapCanisters);
    }
    
    if CREATE_NEW_USERS_MAP_CANISTER_LOCK.with(|l| l.get()) == true {
        return Err(CreateNewUsersMapCanisterError::CreateNewUsersMapCanisterLockIsOn);
    }
    CREATE_NEW_USERS_MAP_CANISTER_LOCK.with(|l| { l.set(true); });
    
    let new_users_map_canister_id: Principal = match get_new_canister(
        Some(ManagementCanisterOptionalCanisterSettings{
            controllers : None,
            compute_allocation : None,
            memory_allocation : 1024*1024*100,
            freezing_threshold : None,
        }),
        7_000_000_000_000
    ).await {
        Ok(canister_id) => canister_id,
        Err(get_new_canister_error) => Err(return CreateNewUsersMapCanisterError::GetNewCanisterError(get_new_canister_error))
    }    
    
    // install code
    if with(&USERS_MAP_CANISTER_CODE, |umcc| umcc.is_none()) {
        with_mut(&NEW_CANISTERS, |new_canisters| { new_canisters.push(new_users_map_canister_id); });
        return Err(CreateNewUsersMapCanisterError::UsersMapCanisterCodeNotFound);
    }
    
    match call<(,)>(
        MANAGEMENT_CANISTER_ID,
        "install_code",
        (ManagementCanisterInstallCodeQuest{
            mode : ManagementCanisterInstallCodeMode::install,
            canister_id : new_users_map_canister_id,
            wasm_module : unsafe localkey_refcell::get(&USERS_MAP_CANISTER_CODE).unwrap().module(),   // .unwrap bc we checked if .is_none() before
            arg : &encode_one(&UsersMapCanisterInit{
                cts_id: id()
            }).unwrap() // unwrap or return Err(candiderror); 
        },)
    ).await {
        Ok(_) => {
            with_mut(&USERS_MAP_CANISTERS, |users_map_canisters| { users_map_canisters.push(new_users_map_canister_id); }) 
            CREATE_NEW_USERS_MAP_CANISTER_LOCK.with(|l| { l.set(false); });
            Ok(new_users_map_canister_id)    
        },
        Err(install_code_call_error) => {
            with_mut(&NEW_CANISTERS, |new_canisters| { new_canisters.push(new_users_map_canister_id); });
            return Err(CreateNewUsersMapCanisterError::InstallCodeCallError(format!("{:?}", install_code_call_error)));
        }
    }
}







#[derive(CandidType, Deserialize)]
pub enum FindUserInTheUsersMapCanistersError {
    UserNotFound,
    UsersMapCanisterFindUserCallFail(UsersMapCanisterId,(RejectionCode,String))
}

pub async fn find_user_in_the_users_map_canisters(user_id: UserId) -> Result<(UserCanisterId, UsersMapCanisterId), FindUserInTheUsersMapCanistersError> {
    
    for i in 0..with(&USERS_MAP_CANISTERS, |umcs| umcs.len()) {
        let umc_id: UsersMapCanisterId = with(&USERS_MAP_CANISTERS, |umcs| umcs[i]);
        match call<(Option<UserData>,)>(
            umc_id,
            "find_user",
            (user_id,)
        ).await {
            Ok(optional_user_data) => match optional_user_data {
                Some(user_canister_id) => return Ok((user_canister_id, umc_id)),
                None => continue
            },
            Err(find_user_call_error) => return Err(FindUserInTheUsersMapCanistersError::UsersMapCanisterFindUserCallFail(umc_id, find_user_call_error)) 
        }
    }
    
    Err(FindUserInTheUsersMapCanistersError::UserNotFound)
}











