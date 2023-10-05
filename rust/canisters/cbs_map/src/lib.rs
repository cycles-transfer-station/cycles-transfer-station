use std::{
    cell::{Cell,RefCell},
    collections::HashMap,
};
use cts_lib::{
    ic_cdk::{
        self,
        api::{
            trap,
            caller,
            call::{
                reply,
                arg_data,
                call_raw128
            },
        },
        update, 
        query, 
        init, 
        pre_upgrade, 
        post_upgrade
    },
    tools::{
        sha256,
        localkey::{
            refcell::{
                with, 
                with_mut,
            },
        },
        upgrade_canisters::*,
        caller_is_controller_gaurd,
    },
    types::{
        Cycles,
        canister_code::CanisterCode,
        cbs_map::{
            CBSMInit,
            CBSMUserData,
            PutNewUserError,
            UpdateUserError,
            CBSMUpgradeCBError,
            //CBSMUpgradeCBErrorKind
        },
    },
    global_allocator_counter::get_allocated_bytes_count
};

use candid::{
    Principal,
    CandidType,
    Deserialize,
};
use serde::{Serialize};
      
use canister_tools::MemoryId;


type UsersMap = HashMap<Principal, CBSMUserData>;

#[derive(Serialize, Deserialize)]
struct CBSMData {
    cts_id: Principal,
    users_map: UsersMap,
    cycles_bank_canister_code: CanisterCode,
    cycles_bank_canister_upgrade_fails: Vec<CBSMUpgradeCBError>,
}
impl CBSMData {
    fn new() -> Self {
        Self {
            cts_id: Principal::from_slice(&[]),
            users_map: UsersMap::new(),
            cycles_bank_canister_code: CanisterCode::empty(),
            cycles_bank_canister_upgrade_fails: Vec::new(),
        }
    }
}




const CBSM_DATA_UPGRADE_MEMORY_ID: MemoryId = MemoryId::new(0);


const MAX_USERS: usize = 20_000; 

const VIEW_CYCLES_BANK_CANISTER_UPGRADE_FAILS_CHUNK_SIZE: usize = 500;



thread_local! {
    static CBSM_DATA: RefCell<CBSMData> = RefCell::new(CBSMData::new());
    
    // not save in a CBSMData
    static     STOP_CALLS: Cell<bool> = Cell::new(false);
    static     STATE_SNAPSHOT_CBSM_DATA_CANDID_BYTES: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}


// ------------------------------------------------------------------------------------



#[init]
fn init(users_map_canister_init: CBSMInit) {
    canister_tools::init(&CBSM_DATA, CBSM_DATA_UPGRADE_MEMORY_ID);
    
    with_mut(&CBSM_DATA, |cbsm_data| {
        cbsm_data.cts_id = users_map_canister_init.cts_id; 
    });
}

#[pre_upgrade]
fn pre_upgrade() {
    canister_tools::pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    canister_tools::post_upgrade(&CBSM_DATA, CBSM_DATA_UPGRADE_MEMORY_ID, None::<fn(CBSMData) -> CBSMData>)
}

#[no_mangle]
pub fn canister_inspect_message() {
    trap("This canister is talked-to by the cts-canisters");
}




// ------------------------------------------------------------------------------------




fn cts_id() -> Principal {
    with(&CBSM_DATA, |cbsm_data| { cbsm_data.cts_id })
}


fn is_full() -> bool {
    with(&CBSM_DATA, |cbsm_data| { cbsm_data.users_map.len() }) >= MAX_USERS
}



// ------------------------------------------------------------------------------------




#[export_name = "canister_update put_new_user"]
pub fn put_new_user() {             // Result<(), PutNewUserError> 
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    if is_full() {
        reply::<(Result<(), PutNewUserError>,)>((Err(PutNewUserError::CanisterIsFull),));
        return;
    }
    let (user_id, umc_user_data): (Principal, CBSMUserData) = arg_data::<(Principal, CBSMUserData)>();
    with_mut(&CBSM_DATA, |cbsm_data| {
        match cbsm_data.users_map.get(&user_id) {
            Some(found_umc_user_data) => {
                reply::<(Result<(), PutNewUserError>,)>((Err(PutNewUserError::FoundUser((*found_umc_user_data).clone())),));
            },
            None => {
                cbsm_data.users_map.insert(user_id, umc_user_data);
                reply::<(Result<(), PutNewUserError>,)>((Ok(()),));
            }
        }
    });
}




#[export_name = "canister_query find_user"]
pub fn find_user() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    let (user_id,): (Principal,) = arg_data::<(Principal,)>();
    with(&CBSM_DATA, |cbsm_data| {
        reply::<(Option<&CBSMUserData>,)>((cbsm_data.users_map.get(&user_id),));
    });
}



#[export_name = "canister_update void_user"]
pub fn void_user() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    let (user_id,): (Principal,) = arg_data::<(Principal,)>();
    with_mut(&CBSM_DATA, |cbsm_data| {
        reply::<(Option<CBSMUserData>,)>((cbsm_data.users_map.remove(&user_id),));
    });
}





#[export_name = "canister_update update_user"]
pub fn update_user() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    let (user_id, umc_user_data): (Principal, CBSMUserData) = arg_data::<(Principal, CBSMUserData)>();
    with_mut(&CBSM_DATA, |cbsm_data| {
        match cbsm_data.users_map.get(&user_id) {
            Some(_found_umc_user_data) => {
                cbsm_data.users_map.insert(user_id, umc_user_data);
                reply::<(Result<(), UpdateUserError>,)>((Ok(()),));  
            },
            None => {
                reply::<(Result<(), UpdateUserError>,)>((Err(UpdateUserError::UserNotFound),));
            }
        }
    });
}



  
  
 
// ----- STOP_CALLS-METHODS --------------------------
 

#[update]
pub fn cts_set_stop_calls_flag(stop_calls_flag: bool) {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    STOP_CALLS.with(|stop_calls| { stop_calls.set(stop_calls_flag); });
}

#[query]
pub fn cts_view_stop_calls_flag() -> bool {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    STOP_CALLS.with(|stop_calls| { stop_calls.get() })
}






// ---------------------------------------------------------------

// ------ Upgrade user_canisters methods -----------------------------



#[update]
pub fn cts_put_user_canister_code(canister_code: CanisterCode) -> () {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    
    if sha256(canister_code.module()) != *canister_code.module_hash() {
        trap("Given canister_code.module_hash is different than the manual compute module hash");
    }
    
    with_mut(&CBSM_DATA, |cbsm_data| {
        cbsm_data.cycles_bank_canister_code = canister_code;
    });
}

#[query(manual_reply = true)]
pub fn cts_see_uc_code_module_hash() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    with(&CBSM_DATA, |cbsm_data| {
        reply::<(&[u8; 32],)>((cbsm_data.cycles_bank_canister_code.module_hash(),));
    });
}






#[update]
pub async fn controller_upgrade_cbs_chunk(q: ControllerUpgradeCSQuest) -> Vec<(Principal, UpgradeOutcome)> {
    caller_is_controller_gaurd(&caller());
    
    let cc: CanisterCode = with_mut(&CBSM_DATA, |cbsm_data| {
        if let Some(new_canister_code) = q.new_canister_code {
            new_canister_code.verify_module_hash().unwrap();
            cbsm_data.cycles_bank_canister_code = new_canister_code; 
        }
        cbsm_data.cycles_bank_canister_code.clone()
    });
    
    let users_cbs: Vec<(Principal, Principal)> = match q.specific_cs {
        Some(specific_cs) => {
            with(&CBSM_DATA, |cbsm_data| {
                cbsm_data.users_map.iter()
                .filter_map(|(user_id, d)| {
                    if specific_cs.contains(&d.cycles_bank_canister_id) {
                        Some((user_id.clone(), d.cycles_bank_canister_id.clone()))
                    } else {
                        None
                    }
                })
                .collect()                
            })
        }
        None => {
            with(&CBSM_DATA, |cbsm_data| {
                cbsm_data.users_map.iter()
                .filter_map(|(user_id, d)| {
                    if &d.cycles_bank_latest_known_module_hash != cc.module_hash() {
                        Some((user_id.clone(), d.cycles_bank_canister_id.clone()))
                    } else {
                        None
                    }
                })
                .take(200)
                .collect()
            })
        }
    };
    
    let (users, cbs): (Vec<Principal>, Vec<Principal>) = users_cbs.into_iter().unzip();
    
    let rs: Vec<(Principal, UpgradeOutcome)> = upgrade_canisters_(cbs, &cc, &q.post_upgrade_quest).await;
    
    with_mut(&CBSM_DATA, |cbsm_data| {
        for (user_id, (_cb, uo)) in users.into_iter().zip(rs.iter()) {
            if let Some(ref r) = uo.install_code_result {
                if r.is_ok() {
                    if let Some(d) = cbsm_data.users_map.get_mut(&user_id) {
                        d.cycles_bank_latest_known_module_hash = cc.module_hash().clone();
                    } else {
                        ic_cdk::print("check this");
                    } 
                }
            }
        } 
    });
    
    return rs;
    
}




/*

#[update(manual_reply = true)]
pub async fn cts_upgrade_ucs_chunk() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    
    // make sure the cycles_bank_canister_upgrade_fails vec is empty ?
    
    if with(&CBSM_DATA, |cbsm_data| { cbsm_data.cycles_bank_canister_code.module().len() == 0 }) {
        trap("No user-canister-code found on this umc");
    }

    let (opt_upgrade_ucs, post_upgrade_arg): (Option<Vec<Principal>>, Vec<u8>) = arg_data::<(Option<Vec<Principal>>, Vec<u8>)>();
    
    let upgrade_ucs: Vec<(Principal, Principal)> = {
        if let Some(q_upgrade_ucs) = opt_upgrade_ucs {
            if q_upgrade_ucs.len() > CYCLES_BANK_CANISTER_UPGRADES_CHUNK_SIZE {
                trap(&format!("Max upgrade_ucs: {:?}", CYCLES_BANK_CANISTER_UPGRADES_CHUNK_SIZE));
            }
            let mut q_upgrade_ucs_good_check_map: HashMap<Principal, Option<Principal>> = q_upgrade_ucs.into_iter().map(|q_upgrade_uc| { (q_upgrade_uc, None) }).collect::<HashMap<Principal, Option<Principal>>>();
            with(&CBSM_DATA, |cbsm_data| {
                for (user_id, umc_user_data) in cbsm_data.users_map.iter() {
                    if q_upgrade_ucs_good_check_map.contains_key(&(umc_user_data.cycles_bank_canister_id)) {
                        q_upgrade_ucs_good_check_map.insert(umc_user_data.cycles_bank_canister_id, Some(/*copy*/*user_id));
                    }
                }
            });
            for (q_upgrade_uc, is_with_a_user_id) in q_upgrade_ucs_good_check_map.iter() {
                if is_with_a_user_id.is_none() {
                    trap(&format!("umc users_map does not contain the user_canister: {:?}", q_upgrade_uc));
                }
            }
            q_upgrade_ucs_good_check_map.into_iter().map(|(q_upgrade_uc, with_the_user_id): (Principal, Option<Principal>)| { (with_the_user_id.unwrap(), q_upgrade_uc) }).collect::<Vec<(Principal, Principal)>>()
        } else {
            let mut upgrade_ucs_gather: Vec<(Principal, Principal)> = Vec::new();
            with(&CBSM_DATA, |cbsm_data| { 
                let current_uc_code_module_hash: [u8; 32] = cbsm_data.cycles_bank_canister_code.module_hash().clone();
                for (user_id, umc_user_data) in cbsm_data.users_map.iter() {
                    if upgrade_ucs_gather.len() >= CYCLES_BANK_CANISTER_UPGRADES_CHUNK_SIZE {
                        break;
                    }
                    if umc_user_data.cycles_bank_latest_known_module_hash != current_uc_code_module_hash {
                        upgrade_ucs_gather.push((user_id.clone(), umc_user_data.cycles_bank_canister_id.clone()));
                    }
                }
            });
            upgrade_ucs_gather
        }
    };    
    
    if upgrade_ucs.len() == 0 {
        reply::<(Option<&Vec<CBSMUpgradeCBError>>,)>((None,));
        return;
    }
    
    // PORTANT!! trying to do a loop here of any sort, for-loop or loop{}-block over chunks of the upgrade_ucs will cause memory corruption in a localkey.with closure within the async block  
    // Now one chunk per call.
   
    let sponses: Vec<Result<[u8; 32], CBSMUpgradeCBError>> = futures::future::join_all(
        upgrade_ucs.iter().map(|upgrade_uc| {
            async {
                let (_user_id,upgrade_uc): (Principal, Principal) = upgrade_uc.clone();
                
                match call::<(CanisterIdRecord,), ()>(
                    MANAGEMENT_CANISTER_ID,
                    "stop_canister",
                    (CanisterIdRecord{ canister_id: upgrade_uc },)
                ).await {
                    Ok(_) => {},
                    Err(stop_canister_call_error) => {
                        return Err((upgrade_uc, CBSMUpgradeCBErrorKind::StopCanisterCallError(stop_canister_call_error.0 as u32, stop_canister_call_error.1))); 
                    }
                }
                
                let install_code_quest_b: Vec<u8> = match with(&CBSM_DATA, |cbsm_data| {
                    encode_one(&ManagementCanisterInstallCodeQuest{
                        mode : ManagementCanisterInstallCodeMode::upgrade,
                        canister_id : upgrade_uc,
                        wasm_module : &(cbsm_data.cycles_bank_canister_code.module()),
                        arg : &post_upgrade_arg,
                    }) 
                }) {
                    Ok(b) => b,
                    Err(candid_error) => {
                        return Err((upgrade_uc, CBSMUpgradeCBErrorKind::UpgradeCodeCallCandidError{candid_error: format!("{:?}", candid_error)}));
                    }
                }; 
                
                let user_canister_code_module_hash: [u8; 32] = with(&CBSM_DATA, |cbsm_data| { cbsm_data.cycles_bank_canister_code.module_hash().clone() });
                match call_raw128(
                    MANAGEMENT_CANISTER_ID,
                    "install_code",
                    &install_code_quest_b,
                    0
                ).await {
                    Ok(_) => {},
                    Err(upgrade_code_call_error) => {
                        return Err((upgrade_uc, CBSMUpgradeCBErrorKind::UpgradeCodeCallError{wasm_module_hash: user_canister_code_module_hash, call_error: (upgrade_code_call_error.0 as u32, upgrade_code_call_error.1)}));
                    }
                }
                
                match call::<(CanisterIdRecord,), ()>(
                    MANAGEMENT_CANISTER_ID,
                    "start_canister",
                    (CanisterIdRecord{ canister_id: upgrade_uc },)
                ).await {
                    Ok(_) => {},
                    Err(start_canister_call_error) => {
                        return Err((upgrade_uc, CBSMUpgradeCBErrorKind::StartCanisterCallError(start_canister_call_error.0 as u32, start_canister_call_error.1))); 
                    }
                }
                
                Ok(user_canister_code_module_hash)
            }
        }).collect::<Vec<_/*anonymous-future*/>>() 
    ).await;
        
    let mut/*mut for the append*/ current_upgrade_fails: Vec<CBSMUpgradeCBError> = Vec::new();
    
    // doing this outside of the async blocks. i seen memory corruption in a localkey refcell with(&) in the async blocks. i rather keep the with_mut(&) out of it
    with_mut(&CBSM_DATA, |cbsm_data| {
        for ((user_id, _user_canister_id), sponse)/*: ((Principal,Principal),Result<[u8; 32], CBSMUpgradeCBError>)*/ in upgrade_ucs.into_iter().zip(sponses.into_iter()) {    
            match sponse {
                Ok(user_canister_code_module_hash) => {
                    match cbsm_data.users_map.get_mut(&user_id) {
                        Some(umc_user_data) => {
                            (*umc_user_data).cycles_bank_latest_known_module_hash = user_canister_code_module_hash; 
                        },
                        None => {}
                    }
                },
                Err(umc_upgrade_uc_error) => {
                    current_upgrade_fails.push(umc_upgrade_uc_error);
                }
            }
        }
    });
    
    reply::<(Option<&Vec<CBSMUpgradeCBError>>,)>((Some(&current_upgrade_fails),));
    
    with_mut(&CBSM_DATA, |cbsm_data| {
        cbsm_data.cycles_bank_canister_upgrade_fails.append(&mut current_upgrade_fails);
        std::mem::drop(current_upgrade_fails); // cause its empty by the append
    });
    
}

*/





#[query(manual_reply = true)]
pub fn cts_view_user_canister_upgrade_fails() {
    if caller() != cts_id() {
        trap("caller must be the CTS");            
    }
    
    let (chunk_i,): (u64,) = arg_data::<(u64,)>();
    
    with(&CBSM_DATA, |cbsm_data| {
        reply::<(Option<&[CBSMUpgradeCBError]>,)>((cbsm_data.cycles_bank_canister_upgrade_fails.chunks(VIEW_CYCLES_BANK_CANISTER_UPGRADE_FAILS_CHUNK_SIZE).nth(chunk_i.try_into().unwrap()),));
    });
}

#[update]
pub fn cts_clear_user_canister_upgrade_fails() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
        
    with_mut(&CBSM_DATA, |cbsm_data| {
        cbsm_data.cycles_bank_canister_upgrade_fails.clear();
        cbsm_data.cycles_bank_canister_upgrade_fails.shrink_to_fit();
    });
}


// ---------------------------------------------------------------------------------


#[derive(CandidType, Deserialize)]
pub struct CTSCallCanisterQuest {
    callee: Principal,
    method_name: String,
    arg_raw: Vec<u8>,
    cycles: Cycles
}

#[update(manual_reply = true)]
pub async fn cts_call_canister() {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }
    
    let (q,): (CTSCallCanisterQuest,) = arg_data::<(CTSCallCanisterQuest,)>(); 
    
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



// ------------ Metrics ------------------------------------

#[derive(CandidType, Deserialize)]
pub struct UMCMetrics {
    global_allocator_counter: u64,
    stable_size: u64,
    cycles_balance: u128,
    user_canister_code_hash: Option<[u8; 32]>,
    users_map_len: u64,
    user_canister_upgrade_fails_len: u64,
}

#[query]
pub fn cts_see_metrics() -> UMCMetrics {
    if caller() != cts_id() {
        trap("caller must be the CTS");
    }

    with(&CBSM_DATA, |cbsm_data| {
        UMCMetrics {
            global_allocator_counter: get_allocated_bytes_count() as u64,    
            stable_size: ic_cdk::api::stable::stable64_size(),
            cycles_balance: ic_cdk::api::canister_balance128(),
            user_canister_code_hash: if cbsm_data.cycles_bank_canister_code.module().len() != 0 { Some(cbsm_data.cycles_bank_canister_code.module_hash().clone()) } else { None },
            users_map_len: cbsm_data.users_map.len() as u64,
            user_canister_upgrade_fails_len: cbsm_data.cycles_bank_canister_upgrade_fails.len() as u64,
        }
    })
}













