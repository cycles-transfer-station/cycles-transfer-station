use crate::{
    LogStorageData,
    StorageCanisterData,
};
use cm_storage_lib::{FlushQuestForward, FlushResult, FlushError};
use cts_lib::{
    types::{
        CallError,
        Cycles,
    },
    consts::{MiB, KiB},
    tools::{
        localkey::refcell::{with,with_mut},
        time_nanos_u64,
        time_nanos,
        call_error_as_u32_and_string,
    },
    ic_cdk::api::call::{
        call_raw128,
        call_with_payment128,
    }
};
use serde::{Serialize, Deserialize};
use serde_bytes::Bytes;
use candid::{Principal, encode_one, decode_one};
use std::{
    thread::LocalKey,
    cell::RefCell,
};





#[derive(Serialize, Deserialize)]
pub enum FlushLogsStorageError {
    CreateStorageCanisterError(CreateStorageCanisterError),
    StorageCanisterCallError(CallError),
    NewStorageCanisterIsFull, // when a *new* trade-log-storage-canister returns StorageIsFull on the first flush call. 
}


const FLUSH_STORAGE_BUFFER_AT_SIZE: usize = 5 * MiB;

const FLUSH_STORAGE_BUFFER_CHUNK_SIZE_BEFORE_MODULO: usize = 1*MiB+512*KiB; 

const CREATE_STORAGE_CANISTER_CYCLES: Cycles = 10_000_000_000_000;



       


pub async fn flush_logs(#[allow(non_snake_case)]LOG_STORAGE_DATA: &'static LocalKey<RefCell<LogStorageData>>) {            
    let mut go: bool = false;
    with_mut(&LOG_STORAGE_DATA, |data| {
        if data.storage_buffer.len() >= FLUSH_STORAGE_BUFFER_AT_SIZE 
        && data.storage_flush_lock == false {
            data.storage_flush_lock = true;
            go = true;
        }
    });
    
    if go == true {
        
        let storage_canister_id: Principal = {
            match with(&LOG_STORAGE_DATA, |data| { 
                data.storage_canisters
                    .last()
                    .and_then(|storage_canister| { 
                        if storage_canister.is_full { None } else { Some(storage_canister.canister_id) }
                    })
            }) {
                Some(c_id) => c_id,
                None => {
                    match create_storage_canister(LOG_STORAGE_DATA).await {
                        Ok(p) => p,
                        Err(e) => {
                            with_mut(&LOG_STORAGE_DATA, |data| {
                                data.storage_flush_lock = false;
                                data.flush_storage_errors.push((FlushLogsStorageError::CreateStorageCanisterError(e), time_nanos_u64()));
                            });
                            return;
                        }
                    }
                }
            }
        };
        
        let chunk_sizes: Vec<usize>/*vec len is the num_of_chunks*/ = with(&LOG_STORAGE_DATA, |data| {
            let max_chunk_size: usize = {
                FLUSH_STORAGE_BUFFER_CHUNK_SIZE_BEFORE_MODULO 
                - 
                (FLUSH_STORAGE_BUFFER_CHUNK_SIZE_BEFORE_MODULO % data.storage_canisters.last().unwrap().log_size as usize)
            };
            data.storage_buffer.chunks(max_chunk_size).map(|c| c.len()).collect::<Vec<usize>>()
        });
        
        for chunk_size in chunk_sizes.into_iter() {

            let chunk_future = with(&LOG_STORAGE_DATA, |data| {
                call_raw128( // <(FlushQuestForward,), (FlushResult,)>
                    storage_canister_id,
                    "flush",
                    &encode_one(&
                        FlushQuestForward{
                            bytes: Bytes::new(&data.storage_buffer[..chunk_size]),
                        }
                    ).unwrap(),
                    10_000_000_000 // put some cycles for the trade-log-storage-canister
                )
            });
            
            match chunk_future.await {
                Ok(sb) => match decode_one::<FlushResult>(&sb).unwrap() {
                    Ok(_flush_success) => {
                        with_mut(&LOG_STORAGE_DATA, |data| {
                            let storage_canister: &mut StorageCanisterData = data.storage_canisters.last_mut().unwrap(); 
                            storage_canister.length += (chunk_size / storage_canister.log_size as usize) as u64;
                            data.storage_buffer.drain(..chunk_size);
                        });
                    },
                    Err(flush_error) => match flush_error {
                        FlushError::StorageIsFull => {
                            with_mut(&LOG_STORAGE_DATA, |data| {
                                data.storage_canisters.last_mut().unwrap().is_full = true;
                            });
                            break;
                        }
                    }
                }
                Err(flush_call_error) => {
                    with_mut(&LOG_STORAGE_DATA, |data| {
                        data.flush_storage_errors.push((FlushLogsStorageError::StorageCanisterCallError(call_error_as_u32_and_string(flush_call_error)), time_nanos_u64()));
                    });
                    break;
                }
            }
        }

        with_mut(&LOG_STORAGE_DATA, |data| {
            data.storage_flush_lock = false;
        });
    }
}





#[derive(Serialize, Deserialize)]
pub enum CreateStorageCanisterError {
    CreateCanisterCallError(CallError),
    InstallCodeCandidError(String),
    InstallCodeCallError(CallError),
}

async fn create_storage_canister(#[allow(non_snake_case)]LOG_STORAGE_DATA: &'static LocalKey<RefCell<LogStorageData>>) -> Result<Principal/*saves the trade-log-storage-canister-data in the LOG_STORAGE_DATA*/, CreateStorageCanisterError> {
    use crate::management_canister::*;
    
    
    let canister_id: Principal = match with_mut(&LOG_STORAGE_DATA, |data| { data.create_storage_canister_temp_holder.take() }) {
        Some(canister_id) => canister_id,
        None => {
            match call_with_payment128::<(ManagementCanisterCreateCanisterQuest,), (CanisterIdRecord,)>(
                Principal::management_canister(),
                "create_canister",
                (ManagementCanisterCreateCanisterQuest{
                    settings: None,
                },),
                CREATE_STORAGE_CANISTER_CYCLES, // cycles for the canister
            ).await {
                Ok(r) => r.0.canister_id,
                Err(call_error) => {
                    return Err(CreateStorageCanisterError::CreateCanisterCallError(call_error_as_u32_and_string(call_error)));
                }
            }
        }
    };
    
    let mut module_hash: [u8; 32] = [0; 32]; // can't initalize an immutable variable from within a closure because the closure mutable-borrows it.
    let mut log_size: u32 = 0;
    
    match with(&LOG_STORAGE_DATA, |data| {
        module_hash = data.storage_canister_code.module_hash().clone();
        log_size = data.storage_canister_init.log_size; 
        
        Ok(call_raw128(
            Principal::management_canister(),
            "install_code",
            &encode_one(
                ManagementCanisterInstallCodeQuest{
                    mode : ManagementCanisterInstallCodeMode::install,
                    canister_id : canister_id,
                    wasm_module : data.storage_canister_code.module(),
                    arg : &encode_one(&data.storage_canister_init)
                        .map_err(|e| { CreateStorageCanisterError::InstallCodeCandidError(format!("{:?}", e)) })?,
                }
            ).map_err(|e| { CreateStorageCanisterError::InstallCodeCandidError(format!("{:?}", e)) })?,    
            0
        ))
        
    })?.await {
        Ok(_) => {
            with_mut(&LOG_STORAGE_DATA, |data| {
                data.storage_canisters.push(
                    StorageCanisterData {
                        log_size,
                        first_log_id: data.storage_canisters.last().map(|c| c.first_log_id + c.length as u128).unwrap_or(0),
                        length: 0,
                        is_full: false,
                        canister_id: canister_id,
                        creation_timestamp: time_nanos(),
                        module_hash,
                    }
                );
            });
            Ok(canister_id)
        }
        Err(install_code_call_error) => {
            with_mut(&LOG_STORAGE_DATA, |data| { data.create_storage_canister_temp_holder = Some(canister_id); });
            return Err(CreateStorageCanisterError::InstallCodeCallError(call_error_as_u32_and_string(install_code_call_error)));
        }
    }
    
}





