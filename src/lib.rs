
// lock each user from making other calls on each async call that awaits, like the collect_balance call, lock the user at the begining and unlock the user at the end. 
// will callbacks (the code after an await) get dropped if the subnet is under heavy load?
// when calling canisters that i dont know if they can possible give-back unexpected candid, use call_raw and dont panic on the candid-decode, return an error.
// dont want to implement From<(RejectionCode, String)> for the return errors in the calls async that call other canisters because if the function makes more than one call then the ? with the from can give-back a wrong error type 
// always check user lock before any awaits (or maybe after the first await if not fective?). 
// in the cycles-market, let a seller set a minimum-purchase-quantity 
// always unlock the user af-ter the last await-call()

#![allow(unused)] // take this out when done


use std::cell::RefCell;
use std::collections::HashMap;
use ic_cdk::{
    api::{
        id,
        trap,
        caller, 
        time, 
        call::{
            call_raw,
            RejectionCode,
            method_name,
            msg_cycles_refunded,
            CallResult,
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
    },
};
use ic_cdk_macros::{update, query};
use ic_ledger_types::{
    Memo as IcpMemo,
    AccountIdentifier as IcpId,
    Subaccount as IcpIdSub,
    Tokens as IcpTokens,
    BlockIndex as IcpBlockIndex,
    Timestamp as IcpTimestamp,
    DEFAULT_SUBACCOUNT as ICP_DEFAULT_SUBACCOUNT,
    DEFAULT_FEE as ICP_LEDGER_TRANSFER_DEFAULT_FEE,
    MAINNET_CYCLES_MINTING_CANISTER_ID,
    MAINNET_LEDGER_CANISTER_ID, 
    transfer as icp_transfer,
    TransferArgs as IcpTransferArgs, 
    TransferResult as IcpTransferResult, 
    TransferError as IcpTransferError


};



mod tools;
use tools::{
    principal_icp_subaccount,
    user_icp_balance_id,
    user_cycles_balance_topup_memo_bytes,
    check_user_icp_balance,
    check_user_cycles_balance,
    main_cts_icp_id,
    check_lock_and_lock_user,
    unlock_user,
    icptokens_to_cycles,

    
};



pub const ICP_PAYOUT_FEE: IcpTokens = IcpTokens::from_e8s(1000000);      // calculate through the xdr conversion rate ?                                               
pub const CYCLES_TRANSFER_FEE: u128 = 300_000_000_000;
pub const ICP_PAYOUT_MEMO: IcpMemo = IcpMemo(u64::from_be_bytes(*b"CTS-POUT"));                                     // b"CTS-POUT"
pub const ICP_TAKE_PAYOUT_FEE_MEMO: IcpMemo = IcpMemo(u64::from_be_bytes(*b"CTS-TFEE"));                            // b"CTS-TFEE"
pub const MEMO_CREATE_CANISTER: IcpMemo = IcpMemo(0x41455243); // == 'CREA'
pub const MEMO_TOP_UP_CANISTER: IcpMemo = IcpMemo(0x50555054); // == 'TPUP'



pub struct UserData {
    pub user_lock: UserLock,
    pub cycles_balance: u128,
    pub untaken_icp_to_collect: IcpTokens,
    


    //pub cycles_transfer_purchases: Vec<CyclesTransferPurchaseLog>, // 

}

pub struct UserLock {
    pub lock: bool,
    pub last_lock_time_nanos: u64 
}

impl Default for UserData {
    fn default() -> Self {
        UserData {
            user_lock: UserLock {
                lock: false,
                last_lock_time_nanos: 0
            },
            cycles_balance: 0u128,
            untaken_icp_to_collect: IcpTokens::ZERO,
            

        }
    }
}



thread_local! {
    pub static USERS_DATA: RefCell<HashMap<Principal, UserData>> = RefCell::new(HashMap::<Principal, UserData>::new());    
}










#[derive(CandidType, Deserialize)]
pub enum CyclesTransferMemo {
    Text(String),
    Nat64(u64),
    Blob(Vec<u8>)
}

#[derive(CandidType, Deserialize)]
pub struct CyclesTransfer {
    memo: CyclesTransferMemo
}



#[update]
pub fn cycles_transfer(ct: CyclesTransfer) -> () {

}









#[derive(CandidType, Deserialize)]
pub struct TopUpCyclesBalanceData {
    topup_cycles_transfer_memo: CyclesTransferMemo
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


#[update]
pub fn topup_balance() -> TopUpBalanceData {
    let user: Principal = caller();
    TopUpBalanceData {
        topup_cycles_balance: TopUpCyclesBalanceData {
            topup_cycles_transfer_memo: CyclesTransferMemo::Blob(user_cycles_balance_topup_memo_bytes(&user).to_vec())
        },
        topup_icp_balance: TopUpIcpBalanceData {
            topup_icp_id: user_icp_balance_id(&user)
        }
    }
}










#[derive(CandidType, Deserialize)]
pub struct UserBalance {
    cycles_balance: u128,
    icp_balance: IcpTokens, 
}

#[derive(CandidType, Deserialize)]
pub enum SeeBalanceError {
    IcpLedgerCheckBalanceCallError(String),
}

pub type SeeBalanceSponse = Result<UserBalance, SeeBalanceError>;

#[update]
pub async fn see_balance() -> SeeBalanceSponse {
    let user: Principal = caller();
    check_lock_and_lock_user(&user);
    let cycles_balance: u128 = check_user_cycles_balance(&user);
    let icp_balance: IcpTokens = match check_user_icp_balance(&user).await {
        Ok(tokens) => tokens,
        Err(balance_call_error) => {
            unlock_user(&user);
            return Err(SeeBalanceError::IcpLedgerCheckBalanceCallError(format!("{:?}", balance_call_error)));
        } 
    };
    unlock_user(&user);
    Ok(UserBalance {
        cycles_balance,
        icp_balance,
    })
}











#[derive(CandidType, Deserialize)]
pub struct IcpPayoutQuest {
    icp: IcpTokens,
    payout_icp_id: IcpId
}

#[derive(CandidType, Deserialize)]
pub struct CyclesPayoutQuest {
    cycles: u128,
    payout_cycles_transfer_canister: Principal         // the memo is: cts-payout    
}

#[derive(CandidType, Deserialize)]
pub enum CollectBalanceQuest {
    icp_payout(IcpPayoutQuest),
    cycles_payout(CyclesPayoutQuest)
}

#[derive(CandidType, Deserialize)]
pub enum IcpPayoutError {
    InvalidIcpAmount,
    IcpLedgerCheckBalanceCallError(String),
    BalanceTooLow { max_icp_payout: IcpTokens },
    IcpLedgerTransferError(IcpTransferError),
    IcpLedgerTransferCallError(String),


}

#[derive(CandidType, Deserialize)]
pub enum CyclesPayoutError {
    InvalidCyclesAmount,
    BalanceTooLow { max_cycles_payout: u128 },
    // CanisterDoesNotExist,
    // NoCyclesTransferMethodOnTheCanister,
    CyclesTransferCallError { call_error: String, paid_fee: bool }, // fee_paid: u128 ??
}

pub type IcpPayoutSponse = Result<IcpBlockIndex, IcpPayoutError>;

pub type CyclesPayoutSponse = Result<u128, CyclesPayoutError>;

#[derive(CandidType, Deserialize)]
pub enum CollectBalanceSponse {
    icp_payout(IcpPayoutSponse),
    cycles_payout(CyclesPayoutSponse)
}

#[update]
pub async fn collect_balance(collect_balance_quest: CollectBalanceQuest) -> CollectBalanceSponse {
    let user: Principal = caller();

    check_lock_and_lock_user(&user);

    match collect_balance_quest {

        CollectBalanceQuest::icp_payout(icp_payout_quest) => {
            
            if icp_payout_quest.icp == IcpTokens::ZERO {
                unlock_user(&user);
                return CollectBalanceSponse::icp_payout(Err(IcpPayoutError::InvalidIcpAmount));
            } 
            
            let user_icp_balance: IcpTokens = match check_user_icp_balance(&user).await {
                Ok(icp_tokens) => icp_tokens,
                Err(balance_call_error) => {
                    unlock_user(&user);
                    return CollectBalanceSponse::icp_payout(Err(IcpPayoutError::IcpLedgerCheckBalanceCallError(format!("{:?}", balance_call_error))));
                }
            };
            
            if icp_payout_quest.icp + ICP_PAYOUT_FEE + IcpTokens::from_e8s(ICP_LEDGER_TRANSFER_DEFAULT_FEE.e8s() * 2) > user_icp_balance {
                unlock_user(&user);
                return CollectBalanceSponse::icp_payout(Err(IcpPayoutError::BalanceTooLow { max_icp_payout: user_icp_balance - ICP_PAYOUT_FEE - IcpTokens::from_e8s(ICP_LEDGER_TRANSFER_DEFAULT_FEE.e8s() * 2) }));
            }
                        
            let icp_payout_transfer_call: CallResult<IcpTransferResult> = icp_transfer(
                MAINNET_LEDGER_CANISTER_ID,
                IcpTransferArgs {
                    memo: ICP_PAYOUT_MEMO,
                    amount: icp_payout_quest.icp,
                    fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE,
                    from_subaccount: Some(principal_icp_subaccount(&user)),
                    to: icp_payout_quest.payout_icp_id,
                    created_at_time: Some(IcpTimestamp { timestamp_nanos: time() })
                }
            ).await; 

           let icp_payout_transfer_call_block_index: IcpBlockIndex = match icp_payout_transfer_call {
                Ok(transfer_result) => match transfer_result {
                    Ok(block_index) => block_index,
                    Err(transfer_error) => {
                        unlock_user(&user);
                        return CollectBalanceSponse::icp_payout(Err(IcpPayoutError::IcpLedgerTransferError(transfer_error)));
                    }
                },
                Err(transfer_call_error) => {
                    unlock_user(&user);
                    return CollectBalanceSponse::icp_payout(Err(IcpPayoutError::IcpLedgerTransferCallError(format!("{:?}", transfer_call_error))));
                }
            };

            let icp_payout_take_fee_transfer_call: CallResult<IcpTransferResult> = icp_transfer(
                MAINNET_LEDGER_CANISTER_ID,
                IcpTransferArgs {
                    memo: ICP_TAKE_PAYOUT_FEE_MEMO,
                    amount: ICP_PAYOUT_FEE,
                    fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE,
                    from_subaccount: Some(principal_icp_subaccount(&user)),
                    to: main_cts_icp_id(),                        
                    created_at_time: Some(IcpTimestamp { timestamp_nanos: time() })
                }
            ).await;             

            match icp_payout_take_fee_transfer_call {
                Ok(transfer_result) => match transfer_result {
                    Ok(block_index) => {},
                    Err(transfer_error) => {
                        USERS_DATA.with(|ud| {
                            ud.borrow_mut().entry(user).or_default().untaken_icp_to_collect += ICP_PAYOUT_FEE + ICP_LEDGER_TRANSFER_DEFAULT_FEE;
                        });
                    }  // log and take into the count 
                },
                Err(transfer_call_error) => { // log and take into the count
                    USERS_DATA.with(|ud| {
                        ud.borrow_mut().entry(user).or_default().untaken_icp_to_collect += ICP_PAYOUT_FEE + ICP_LEDGER_TRANSFER_DEFAULT_FEE;
                    });
                }
            }
            unlock_user(&user);
            return CollectBalanceSponse::icp_payout(Ok(icp_payout_transfer_call_block_index));
        },



        CollectBalanceQuest::cycles_payout(cycles_payout_quest) => {

            if cycles_payout_quest.cycles == 0 {
                unlock_user(&user);
                return CollectBalanceSponse::cycles_payout(Err(CyclesPayoutError::InvalidCyclesAmount));
            }

            let user_cycles_balance: u128 = check_user_cycles_balance(&user);

            if cycles_payout_quest.cycles + CYCLES_TRANSFER_FEE > user_cycles_balance {
                unlock_user(&user);
                return CollectBalanceSponse::cycles_payout(Err(CyclesPayoutError::BalanceTooLow { max_cycles_payout: user_cycles_balance - CYCLES_TRANSFER_FEE }));
            }

            let cycles_transfer_call: CallResult<Vec<u8>> = call_raw(
                cycles_payout_quest.payout_cycles_transfer_canister,
                "cycles_transfer",
                encode_one(&CyclesTransfer { memo: CyclesTransferMemo::Text("CTS-PAYOUT".to_string()) }).unwrap(),
                cycles_payout_quest.cycles as u64  // as u64 for now till cdk compatible with the u128
            ).await;
            
            // check if it is possible for the canister to reject/trap but still keep the cycles. if yes, [re]turn the cycles_accepted in the error. for now, going as if not possible.

            unlock_user(&user);

            match cycles_transfer_call {
                Ok(_) => {
                    let cycles_accepted: u128 = cycles_payout_quest.cycles - msg_cycles_refunded() as u128; 
                    USERS_DATA.with(|ud| { ud.borrow_mut().get_mut(&user).unwrap().cycles_balance -= cycles_accepted + CYCLES_TRANSFER_FEE; });          // can unwrap here because of the checks [a]bove, that the user's-balance is greater than 1
                    return CollectBalanceSponse::cycles_payout(Ok(cycles_accepted));
                },
                Err(cycles_transfer_call_error) => {
                    match cycles_transfer_call_error.0 {
                        RejectionCode::DestinationInvalid | RejectionCode::CanisterReject | RejectionCode::CanisterError => {
                            USERS_DATA.with(|ud| { ud.borrow_mut().get_mut(&user).unwrap().cycles_balance -= CYCLES_TRANSFER_FEE; });
                            return CollectBalanceSponse::cycles_payout(Err(CyclesPayoutError::CyclesTransferCallError{ call_error: format!("{:?}", cycles_transfer_call_error), paid_fee: true }));
                        },
                        _ => {
                            return CollectBalanceSponse::cycles_payout(Err(CyclesPayoutError::CyclesTransferCallError{ call_error: format!("{:?}", cycles_transfer_call_error), paid_fee: false }));
                        }
                    }
                }
            }


            
        }
    }
}









#[derive(CandidType, Deserialize)]
pub struct ConvertIcpBalanceForTheCyclesWithTheCmcRateQuest {
    icp: IcpTokens
}

#[derive(CandidType, Deserialize)]
pub enum ConvertIcpBalanceForTheCyclesWithTheCmcRateError {
    CmcGetRateCallError(String),
    CmcGetRateCallSponseCandidDecodeError(String),
    TopUpCyclesIcpTransferCallError(String),
    TopUpCyclesIcpTransferError(IcpTransferError),
    TopUpCyclesIcpNotifyQuestCandidEncodeError { candid_error: String, topup_transfer_block_height: IcpBlockIndex },
    TopUpCyclesIcpNotifyCallError { notify_call_error: String, topup_transfer_block_height: IcpBlockIndex },
    TopUpCyclesIcpNotifySponseCandidDecodeError { candid_error: String, topup_transfer_block_height: IcpBlockIndex },
    TopUpCyclesIcpTransferRefund(String, Option<IcpBlockIndex>),
    UnknownIcpNotifySponse
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

#[derive(CandidType, Deserialize)]
struct NotifyCanisterArgs {
    to_subaccount : Option<IcpIdSub>,
    from_subaccount : Option<IcpIdSub>,
    to_canister : Principal,
    max_fee : IcpTokens,
    block_height : IcpBlockIndex,
}

#[derive(CandidType, Deserialize)]
enum CyclesSponse {
    CanisterCreated(Principal),
    // Silly requirement by the candid derivation
    ToppedUp(()),
    Refunded(String, Option<IcpBlockIndex>),
}

#[update]
pub async fn convert_icp_balance_for_the_cycles_with_the_cmc_rate(q: ConvertIcpBalanceForTheCyclesWithTheCmcRateQuest) -> Result<u128, ConvertIcpBalanceForTheCyclesWithTheCmcRateError> {    
    
    let user: Principal = caller();

    let xdr_permyriad_per_icp: u64 = {
        let candid_bytes: Vec<u8> = match call_raw(
            MAINNET_CYCLES_MINTING_CANISTER_ID,
            "get_icp_xdr_conversion_rate",
            encode_one(()).unwrap(),
            0
        ).await {
            Ok(b) => b,
            Err(call_error) => return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::CmcGetRateCallError(format!("{:?}", call_error)))
        };
        let icp_xdr_conversion_rate_with_certification: IcpXdrConversionRateCertifiedResponse = match decode_one(&candid_bytes) {
            Ok(s) => s,
            Err(candid_error) => return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::CmcGetRateCallSponseCandidDecodeError(format!("{}", candid_error))),
        };
        icp_xdr_conversion_rate_with_certification.data.xdr_permyriad_per_icp
    };

    let cycles: u128 = icptokens_to_cycles(q.icp, xdr_permyriad_per_icp);

    check_lock_and_lock_user(&user);

    let topup_cycles_icp_transfer_call: CallResult<IcpTransferResult> = icp_transfer(
        MAINNET_LEDGER_CANISTER_ID,
        IcpTransferArgs {
            memo: MEMO_TOP_UP_CANISTER,
            amount: q.icp,   // q.icp - ICP_LEDGER_TRANSFER_DEFAULT_FEE ??
            fee: ICP_LEDGER_TRANSFER_DEFAULT_FEE,
            from_subaccount: Some(principal_icp_subaccount(&user)),
            to: IcpId::new(&MAINNET_CYCLES_MINTING_CANISTER_ID, &principal_icp_subaccount(&id())),
            created_at_time: Some(IcpTimestamp { timestamp_nanos: time() })
        }
    ).await; 
    
    let topup_cycles_icp_transfer_call_block_index: IcpBlockIndex = match topup_cycles_icp_transfer_call {
        Ok(transfer_call_sponse) => match transfer_call_sponse {
            Ok(block_index) => block_index,
            Err(transfer_error) => {
                unlock_user(&user);
                return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::TopUpCyclesIcpTransferError(transfer_error));
            }
        },
        Err(transfer_call_error) => {
            unlock_user(&user);
            return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::TopUpCyclesIcpTransferCallError(format!("{:?}", transfer_call_error)));
        }
    };

    let topup_cycles_icp_notify_call: CallResult<Vec<u8>> = call_raw(
        MAINNET_LEDGER_CANISTER_ID,
        "notify_dfx",
        match encode_one(
            &NotifyCanisterArgs {
                to_subaccount : Some(principal_icp_subaccount(&id())),
                from_subaccount : Some(principal_icp_subaccount(&user)),
                to_canister : MAINNET_CYCLES_MINTING_CANISTER_ID,
                max_fee : ICP_LEDGER_TRANSFER_DEFAULT_FEE,
                block_height : topup_cycles_icp_transfer_call_block_index,
            }
        ) {
            Ok(b) => b,
            Err(candid_error) => {
                unlock_user(&user);
                return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::TopUpCyclesIcpNotifyQuestCandidEncodeError { candid_error: format!("{}", candid_error), topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index });
            }
        },
        0
    ).await;

    unlock_user(&user);

    let topup_cycles_icp_notify_sponse: CyclesSponse = match topup_cycles_icp_notify_call {
        Ok(b) => match decode_one(&b) {
            Ok(cycles_sponse) => cycles_sponse,
            Err(candid_error) => {
                return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::TopUpCyclesIcpNotifySponseCandidDecodeError { candid_error: format!("{}", candid_error), topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index });
            }
        },
        Err(notify_call_error) => {
            return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::TopUpCyclesIcpNotifyCallError { notify_call_error: format!("{:?}", notify_call_error), topup_transfer_block_height: topup_cycles_icp_transfer_call_block_index });
        }
    };

    match topup_cycles_icp_notify_sponse {
        CyclesSponse::Refunded(refund_message, optional_refund_block_height) => {
            return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::TopUpCyclesIcpTransferRefund(refund_message, optional_refund_block_height));
        },
        CyclesSponse::ToppedUp(_) => {
            USERS_DATA.with(|ud| {
                ud.borrow_mut().entry(user).or_default().cycles_balance += cycles;
            });
            return Ok(cycles);
        },
        CyclesSponse::CanisterCreated(principal) => {
            return Err(ConvertIcpBalanceForTheCyclesWithTheCmcRateError::UnknownIcpNotifySponse);
        }
    }
}











#[derive(CandidType, Deserialize)]
struct PurchaseCyclesTransferQuest {
    r#for: Principal,
    cycles: u128,
    cycles_transfer_memo: CyclesTransferMemo,
    public: bool,
}

#[derive(CandidType, Deserialize)]
enum PurchaseCyclesTransferError {
    BalanceTooLow,
    CanisterDoesNotExist,
    NoCyclesTransferMethodOnTheCanister,

}

// #[update]
// pub async fn purchase_cycles_transfer(PurchaseCyclesTransferQuest) -> Result<u128, PurchaseCyclesTransferError> {

// }





#[derive(CandidType, Deserialize)]
struct PurchaseCyclesBankQuest {

}

#[derive(CandidType, Deserialize)]
struct CyclesBankPurchaseLog {
    cycles_bank_principal: Principal,
    cost_cycles: u64, // 64? or 128
    timestamp: u64
}

#[derive(CandidType, Deserialize)]
enum PurchaseCyclesBankError {

}

// #[update]
// pub async fn purchase_cycles_bank(q: PurchaseCyclesBankQuest) -> Result<CyclesBankPurchaseLog, PurchaseCyclesBankError> {

// }




#[derive(CandidType, Deserialize)]
struct CyclesTransferPurchaseLog {
    r#for: Principal,
    cycles_sent: u128,
    cycles_accepted: u128, // 64?
    cycles_transfer_memo: CyclesTransferMemo,
    timestamp: u64,
}

// #[update]
// pub fn see_cycles_transfer_purchases(page: u128) -> Vec<CyclesTransferPurchaseLog> {

// }


// #[update]
// pub fn see_cycles_bank_purchases(page: u128) -> Vec<CyclesBankPurchaseLog> {

// }



#[derive(CandidType, Deserialize)]
struct Fees {
    purchase_cycles_bank_cost_cycles: u128,
    purchase_cycles_transfer_cost_cycles: u128
}

// #[update]
// pub fn see_fees() -> Fees {
    
// }








#[no_mangle]
pub fn canister_inspect_message() {
    // caution: this function is only called for ingress messages 
    
    if [
        "topup_balance", 
        "see_balance", 
        "collect_balance", 
        "convert_icp_balance_for_the_cycles_with_the_cmc_rate", 
    
    ].contains(&&method_name()[..]) {
        if caller() == Principal::anonymous() { // check '==' plementation is correct otherwise caller().as_slice() == Principal::anonymous().as_slice()
            trap("caller cannot be anonymous for this method.")
        }
    }


    ic_cdk::api::call::accept_message();
}



