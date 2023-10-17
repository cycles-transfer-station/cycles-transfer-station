use candid::{Principal, CandidType, Deserialize};
use crate::icrc::{IcrcId, Tokens, TokenTransferError, BlockId};
use crate::types::{Cycles, CallError, canister_code::CanisterCode};
use crate::consts::KiB;
use serde::Serialize;

pub type PositionId = u128;
pub type PurchaseId = u128;
pub type CyclesPerToken = Cycles;

#[derive(CandidType, Deserialize)]
pub struct CMIcrc1TokenTradeContractInit {
    pub cts_id: Principal,
    pub cm_main_id: Principal,
    pub icrc1_token_ledger: Principal,
    pub icrc1_token_ledger_transfer_fee: Tokens,
    pub trades_storage_canister_code: CanisterCode,
    pub positions_storage_canister_code: CanisterCode,
}

// ----



#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct BuyTokensQuest {
    pub cycles: Cycles,
    pub cycles_per_token_rate: CyclesPerToken,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct SellTokensQuest {
    pub tokens: Tokens,
    pub cycles_per_token_rate: CyclesPerToken,
}



#[derive(CandidType, Deserialize)]
pub enum BuyTokensError {
    BuyTokensMinimumTokens(Tokens),
    RateCannotBeZero,
    MsgCyclesTooLow,
    CyclesMarketIsBusy,
}

#[derive(CandidType, Deserialize)]
pub struct BuyTokensSuccess {
    pub position_id: PositionId,
}

pub type BuyTokensResult = Result<BuyTokensSuccess, BuyTokensError>;





#[derive(CandidType, Deserialize)]
pub enum SellTokensError {
    SellTokensMinimum(Tokens),
    RateCannotBeZero,
    CallerIsInTheMiddleOfADifferentCallThatLocksTheTokenBalance,
    CyclesMarketIsBusy,
    CheckUserCyclesMarketTokenLedgerBalanceError(CallError),
    UserTokenBalanceTooLow{ user_token_balance: Tokens },
}


#[derive(CandidType, Deserialize)]
pub struct SellTokensSuccess {
    pub position_id: PositionId,
    //sell_tokens_so_far: Tokens,
    //cycles_payout_so_far: Cycles,
    //position_closed: bool
}


pub type SellTokensResult = Result<SellTokensSuccess, SellTokensError>;



#[derive(CandidType, Deserialize)]
pub struct VoidPositionQuest {
    pub position_id: PositionId
}

#[derive(CandidType, Deserialize)]
pub enum VoidPositionError {
    WrongCaller,
    MinimumWaitTime{ minimum_wait_time_seconds: u128, position_creation_timestamp_seconds: u128 },
    CyclesMarketIsBusy,
    PositionNotFound,
}

pub type VoidPositionResult = Result<(), VoidPositionError>;

// ----

#[derive(CandidType, Deserialize)]
pub struct TransferTokenBalanceQuest {
    pub tokens: Tokens,
    pub token_fee: Tokens, // must set and cant be opt, bc the contract must check that the user has the available balance unlocked and must know the amount + fee is available (not locked) in the account.   
    pub to: IcrcId,
    pub created_at_time: Option<u64>
}

#[derive(CandidType, Deserialize)]
pub enum TransferTokenBalanceError {
    CyclesMarketIsBusy,
    CallerIsInTheMiddleOfACreateTokenPositionOrPurchaseCyclesPositionOrTransferTokenBalanceCall,
    CheckUserCyclesMarketTokenLedgerBalanceCallError((u32, String)),
    UserTokenBalanceTooLow{ user_token_balance: Tokens },
    TokenTransferCallError((u32, String)),
    TokenTransferError(TokenTransferError)
}

pub type TransferTokenBalanceResult = Result<BlockId, TransferTokenBalanceError>;

// ----

#[derive(CandidType, Serialize, Deserialize)]
pub struct CMVoidCyclesPositionPositorMessageQuest {
    pub position_id: PositionId,
    // cycles in the call
    pub timestamp_nanos: u128
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct CMVoidTokenPositionPositorMessageQuest {
    pub position_id: PositionId,
    pub void_tokens: Tokens,
    pub timestamp_nanos: u128
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct CMCyclesPositionPurchasePositorMessageQuest {
    pub cycles_position_id: PositionId,
    pub purchase_id: PurchaseId,
    pub purchaser: Principal,
    pub purchase_timestamp_nanos: u128,
    pub cycles_purchase: Cycles,
    pub cycles_position_cycles_per_token_rate: CyclesPerToken,
    pub token_payment: Tokens,
    pub token_transfer_block_height: BlockId,
    pub token_transfer_timestamp_nanos: u128,
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct CMCyclesPositionPurchasePurchaserMessageQuest {
    pub cycles_position_id: PositionId,
    pub cycles_position_positor: Principal,
    pub cycles_position_cycles_per_token_rate: CyclesPerToken,
    pub purchase_id: PurchaseId,
    pub purchase_timestamp_nanos: u128,
    // cycles in the call
    pub token_payment: Tokens,
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct CMTokenPositionPurchasePositorMessageQuest {
    pub token_position_id: PositionId,
    pub token_position_cycles_per_token_rate: CyclesPerToken,
    pub purchaser: Principal,
    pub purchase_id: PurchaseId,
    pub token_purchase: Tokens,
    pub purchase_timestamp_nanos: u128,
    // cycles in the call
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct CMTokenPositionPurchasePurchaserMessageQuest {
    pub token_position_id: PositionId,
    pub purchase_id: PurchaseId, 
    pub positor: Principal,
    pub purchase_timestamp_nanos: u128,
    pub cycles_payment: Cycles,
    pub token_position_cycles_per_token_rate: CyclesPerToken,
    pub token_purchase: Tokens,
    pub token_transfer_block_height: BlockId,
    pub token_transfer_timestamp_nanos: u128,
}


// ---------------





pub mod trade_log; 
pub mod position_log;



pub const MAX_LATEST_TRADE_LOGS_SPONSE_TRADE_DATA: usize = 512*KiB*3 / std::mem::size_of::<LatestTradesDataItem>();



#[derive(Copy, Clone, CandidType, Serialize, Deserialize, PartialEq, Eq)]
pub enum PositionKind {
    Cycles,
    Token
}


#[derive(CandidType, Deserialize)]
pub struct ViewLatestTradesQuest {
    pub opt_start_before_id: Option<PurchaseId>,
}

pub type LatestTradesDataItem = (PurchaseId, Tokens, CyclesPerToken, u64, PositionKind);

#[derive(CandidType, Deserialize)]
pub struct ViewLatestTradesSponse {
    pub trades_data: Vec<LatestTradesDataItem>, 
    pub is_last_chunk_on_this_canister: bool,
}



