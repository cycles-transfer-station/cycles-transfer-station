use std::collections::HashMap;
use sha2::Digest;
use ic_cdk::{
    api::{
        id,
        time,
        trap,
        call::{
            CallResult,
            RejectionCode,
            call_raw128,
        },
    },
    export::{
        Principal,
        candid::{
            CandidType,
            Deserialize,
            utils::{encode_one, decode_one},
            error::Error as CandidError,
        },
    }
};

use crate::{
    USERS_DATA,
    NEW_CANISTERS,
    UserLock,
    UserData,
    IcpId,
    IcpIdSub,
    IcpTokens,
    IcpAccountBalanceArgs,
    icp_account_balance,
    ICP_DEFAULT_SUBACCOUNT,
    MAINNET_LEDGER_CANISTER_ID,
    MAINNET_CYCLES_MINTING_CANISTER_ID,
    MANAGEMENT_CANISTER_PRINCIPAL,


};




pub fn sha256(bytes: &[u8]) -> [u8; 32] { // [in]ferr[ed] lifetime on the &[u8]-param?
    let mut hasher: sha2::Sha256 = sha2::Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}



fn principal_as_thirty_bytes(p: &Principal) -> [u8; 30] {
    let mut bytes: [u8; 30] = [0; 30];
    let p_bytes: &[u8] = p.as_slice();
    bytes[0] = p_bytes.len() as u8; 
    bytes[1 .. p_bytes.len() + 1].copy_from_slice(p_bytes); 
    bytes
}

fn thirty_bytes_as_principal(bytes: &[u8; 30]) -> Principal {
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


pub fn user_cycles_balance_topup_memo_bytes(user: &Principal) -> [u8; 32] {
    let mut memo_bytes = [0u8; 32];
    memo_bytes[..2].copy_from_slice(b"TP");
    memo_bytes[2..].copy_from_slice(&principal_as_thirty_bytes(user));
    memo_bytes
}


pub async fn check_user_icp_balance(user: &Principal) -> CallResult<IcpTokens> {
    let mut icp_balance: IcpTokens = icp_account_balance(
        MAINNET_LEDGER_CANISTER_ID,
        IcpAccountBalanceArgs { account: user_icp_balance_id(user) }    
    ).await?;
    icp_balance -= USERS_DATA.with(|ud| { ud.borrow_mut().entry(*user).or_default().untaken_icp_to_collect });
    Ok(icp_balance)
}


pub fn check_user_cycles_balance(user: &Principal) -> u128 {
    USERS_DATA.with(|ud| {
        ud.borrow_mut().entry(*user).or_default().cycles_balance
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
        if user_lock.lock == true && current_time - user_lock.last_lock_time_nanos < 3*60*1_000_000_000 {
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

#[derive(CandidType, Deserialize)]
struct IcpXdrConversionRate {
    xdr_permyriad_per_icp : u64,
    timestamp_seconds : u64
}

// cache this? for a certain-mount of the time?
pub async fn check_current_xdr_permyriad_per_icp_cmc_rate() -> Result<u64, CheckCurrentXdrPerMyriadPerIcpCmcRateError> {
    let call_sponse_candid_bytes: Vec<u8> = match call_raw128(
        MAINNET_CYCLES_MINTING_CANISTER_ID,
        "get_icp_xdr_conversion_rate",
        &encode_one(()).unwrap(),
        0
    ).await {
        Ok(b) => b,
        Err(call_error) => return Err(CheckCurrentXdrPerMyriadPerIcpCmcRateError::CmcGetRateCallError(format!("{:?}", call_error)))
    };
    let icp_xdr_conversion_rate_with_certification: IcpXdrConversionRateCertifiedResponse = match decode_one(&call_sponse_candid_bytes) {
        Ok(s) => s,
        Err(candid_error) => return Err(CheckCurrentXdrPerMyriadPerIcpCmcRateError::CmcGetRateCallSponseCandidError(format!("{}", candid_error))),
    };
    Ok(icp_xdr_conversion_rate_with_certification.data.xdr_permyriad_per_icp)
}


pub const DEFAULT_CYCLES_PER_XDR: u128 = 1_000_000_000_000u128; // 1T cycles = 1 XDR

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

pub async fn get_new_canister() -> Result<Principal, GetNewCanisterError> {
    
    if let Some(principal) = NEW_CANISTERS.with(|nc| nc.borrow_mut().pop()) {
        return Ok(principal);
    } 

    let create_canister_management_call_quest_candid_bytes: Vec<u8> = match encode_one(
        &ManagementCanisterCreateCanisterQuest {
            settings: Some(ManagementCanisterOptionalCanisterSettings{
                controllers: Some(vec![ic_cdk::api::id()]),
                compute_allocation : None,
                memory_allocation : None,
                freezing_threshold : None
            })
        }
    ) {
        Ok(candid_bytes) => candid_bytes,
        Err(candid_error) => {
            return Err(GetNewCanisterError::CreateCanisterManagementCallQuestCandidError(format!("{}", candid_error)))
        }
    };

    let create_canister_management_call: CallResult<Vec<u8>> = call_raw128(
        MANAGEMENT_CANISTER_PRINCIPAL,
        "create_canister",
        &create_canister_management_call_quest_candid_bytes,
        0
    ).await;

    let canister_principal: Principal = match create_canister_management_call {
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

    Ok(canister_principal)

}