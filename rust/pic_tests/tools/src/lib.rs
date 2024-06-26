use pocket_ic::{*, common::rest::RawEffectivePrincipal};
use candid::{Nat, Principal, CandidType, Deserialize};
use std::collections::{HashSet, HashMap};
use cts_lib::{
    consts::{TRILLION, KiB},
    tools::principal_token_subaccount,
    types::{
        cm::cm_main::*,
        Cycles,
        CanisterCode,
    },
};
use icrc_ledger_types::icrc1::{account::Account, transfer::{TransferArg, TransferError, BlockIndex}};


pub mod bank; 
pub mod tc;

pub const ICP_LEDGER_TRANSFER_FEE: u128 = 10_000;
pub const CMC_RATE: u128 = 55555;
pub const ICP_MINTER: Principal = Principal::from_slice(b"icp-minter");
pub const CMC: Principal = Principal::from_slice(&[0,0,0,0,0,0,0,4,1,1]);
pub const NNS_GOVERNANCE: Principal = Principal::from_slice(&[0,0,0,0,0,0,0,1,1,1]);
pub const ICP_LEDGER: Principal = Principal::from_slice(&[0,0,0,0,0,0,0,2,1,1]);
pub const CTS_CONTROLLER: Principal = Principal::from_slice(&[0,1,2,3,4,5,6,7,8,9]);
pub const CTS: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 48, 0, 110, 1, 1]);
pub const CM_MAIN: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 48, 0, 111, 1, 1]);
pub const BANK: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 48, 0, 170, 1, 1]);

use std::path::PathBuf;


/*
fn workspace_dir() -> PathBuf {
    let output = std::process::Command::new(env!("CARGO"))
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .output()
        .unwrap()
        .stdout;
    let cargo_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
    cargo_path.parent().unwrap().to_path_buf()
}
*/
pub fn workspace_dir() -> PathBuf {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d = d.parent().unwrap().parent().unwrap().to_path_buf();
    d
}

pub fn wasms_dir() -> PathBuf {
    let mut d = workspace_dir();
    d.push("target/wasm32-unknown-unknown/debug");
    d
}

pub fn pic_get_time_nanos(pic: &PocketIc) -> u128 {
    pic.get_time().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

pub fn icrc1_transfer(pic: &PocketIc, ledger: Principal, owner: Principal, q: TransferArg) -> Result<BlockIndex, TransferError> {
    call_candid_as::<_, (Result<BlockIndex, TransferError>,)>(pic, ledger, RawEffectivePrincipal::None, owner, "icrc1_transfer", (q,)).unwrap().0
}

pub fn icrc1_balance(pic: &PocketIc, ledger: Principal, countid: &Account) -> u128 {
    call_candid(
        pic,
        ledger,
        RawEffectivePrincipal::None,
        "icrc1_balance_of",
        (countid,),
    ).map(|t: (u128,)| t.0).unwrap()
}

pub fn mint_icp(pic: &PocketIc, to: &Account, amount: u128) {
    let (mint_icp_r,): (Result<Nat, TransferError>,) = call_candid_as_(
        pic,
        ICP_LEDGER,
        ICP_MINTER,            
        "icrc1_transfer",
        (TransferArg{
            from_subaccount: None,
            to: to.clone(),
            fee: None,
            created_at_time: None,
            memo: None,
            amount: amount.into(),
        },)
    ).unwrap();
    mint_icp_r.unwrap();
}

pub fn create_and_download_state_snapshot<T: candid::CandidType + for<'a> Deserialize<'a>>(pic: &PocketIc, caller: Principal, canister: Principal, memory_id: u8) -> T {
    let (snapshot_len,): (u64,) = call_candid_as(&pic, canister, RawEffectivePrincipal::None, caller, "controller_create_state_snapshot", (memory_id,)).unwrap();
    let mut v = Vec::<u8>::new();
    let mut i = 0;
    let chunk_size_bytes = 512 * KiB * 3; 
    while (v.len() as u64) < snapshot_len {
        let (chunk,): (Vec<u8>,) = query_candid_as(&pic, canister, caller, "controller_download_state_snapshot", 
            (memory_id, v.len() as u64, std::cmp::min(chunk_size_bytes as u64, snapshot_len - v.len() as u64))
        ).unwrap(); 
        i = i + chunk.len();
        v.extend(chunk);
    }  
    assert_eq!(v.len(), snapshot_len as usize);
    candid::decode_one(&v).unwrap()    
}

pub fn call_candid_as_<Input, Output>(
    env: &PocketIc,
    canister_id: Principal,
    sender: Principal,
    method: &str,
    input: Input
) -> Result<Output, pocket_ic::CallError>
where
    Input: candid::utils::ArgumentEncoder,
    Output: for<'a> candid::utils::ArgumentDecoder<'a>,
{
    call_candid_as(env, canister_id, RawEffectivePrincipal::None, sender, method, input)
}

pub fn call_candid_<Input, Output>(
    env: &PocketIc,
    canister_id: Principal,
    method: &str,
    input: Input
) -> Result<Output, pocket_ic::CallError>
where
    Input: candid::utils::ArgumentEncoder,
    Output: for<'a> candid::utils::ArgumentDecoder<'a>,
{
    call_candid(env, canister_id, RawEffectivePrincipal::None, method, input)
}

pub trait WasmResultUnwrap {
    fn unwrap(self) -> Vec<u8>;
}
impl WasmResultUnwrap for WasmResult {
    fn unwrap(self) -> Vec<u8> {
        match self {
            WasmResult::Reply(b) => b,
            WasmResult::Reject(s) => panic!("{}", s),
        }
    }
}


pub fn set_up() -> PocketIc {
    let pic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_fiduciary_subnet()
        .build();
    //let _nns_subnet = pic.topology().get_nns().unwrap();
    //let _fid_subnet = pic.topology().get_fiduciary().unwrap();
    
    // ICP-LEDGER
    let icp_minter = ICP_MINTER;
    let icp_ledger_wasm = std::fs::read(workspace_dir().join("pic_tests/pre-built-wasms/ledger-canister-o-98eb213581b239c3829eee7076bea74acad9937b.wasm.gz")).unwrap();
    let icp_ledger = pic.create_canister_with_id(None, None, ICP_LEDGER).unwrap();
    pic.add_cycles(icp_ledger, 1_000 * TRILLION);    
    
    #[derive(CandidType, Deserialize)]
    enum IcpLedgerPayload {
        Init(IcpLedgerInitArgs)
    }
    #[derive(CandidType, Deserialize)]
    struct IcpLedgerFeatureFlags {
        icrc2: bool
    }
    #[derive(CandidType, Deserialize)]
    struct IcpLedgerInitArgs {
        minting_account: String,
        icrc1_minting_account: Option<Account>,
        initial_values: HashMap<String, ic_ledger_types::Tokens>,
        send_whitelist: HashSet<Principal>,
        transfer_fee: Option<ic_ledger_types::Tokens>,
        token_symbol: Option<String>,
        token_name: Option<String>,
        feature_flags: Option<IcpLedgerFeatureFlags>,
    }
    pic.install_canister(
        icp_ledger, 
        icp_ledger_wasm, 
        candid::encode_one(
            IcpLedgerPayload::Init(
                IcpLedgerInitArgs{
        			minting_account: ic_ledger_types::AccountIdentifier::new(&icp_minter, &ic_ledger_types::DEFAULT_SUBACCOUNT).to_hex(),
        			icrc1_minting_account: Some(Account{owner: icp_minter, subaccount: None}),
        			initial_values: HashMap::new(),
        			send_whitelist: HashSet::new(),
        			transfer_fee: Some(ic_ledger_types::Tokens::from_e8s(ICP_LEDGER_TRANSFER_FEE as u64)),
        			token_symbol: Some("ICP".to_string()),
    				token_name: Some("Internet-Computer".to_string()),
    				feature_flags: Some(IcpLedgerFeatureFlags{ icrc2: true }),
                }   
            )
        ).unwrap(), 
        None
    );
    
    
    
    
    // CMC
    let nns_governance = NNS_GOVERNANCE;
    let cmc_wasm = std::fs::read(workspace_dir().join("pic_tests/pre-built-wasms/cmc-o-14e0b0adf6632a6225cb1b0a22d4bafce75eb81e.wasm.gz")).unwrap();
    let cmc = pic.create_canister_with_id(None, None, CMC).unwrap();
    pic.add_cycles(cmc, 1_000 * TRILLION);    
    pic.install_canister(
        cmc, 
        cmc_wasm, 
        candid::encode_one(
            {
                #[derive(CandidType, Deserialize)]
                struct Ia {
                    ledger_canister_id: Option<Principal>,
                    governance_canister_id: Option<Principal>,
                    minting_account_id: Option<String>,
                    last_purged_notification: Option<u64>,
                }
                Ia{
                    ledger_canister_id: Some(icp_ledger),
                    governance_canister_id: Some(nns_governance),
                    minting_account_id: Some(ic_ledger_types::AccountIdentifier::new(&icp_minter, &ic_ledger_types::DEFAULT_SUBACCOUNT).to_hex()),   
                    last_purged_notification: Some(0),
                }
            }
        ).unwrap(), 
        None
    );
       
    let cmc_rate: u128 = CMC_RATE;
    #[derive(CandidType, Deserialize)]
    struct UpdateIcpXdrConversionRatePayload {
        data_source: String,
        timestamp_seconds: u64,
        xdr_permyriad_per_icp: u64,
    }
    let (r,): (Result<(), String>,) = call_candid_as(
        &pic,
        cmc,
        RawEffectivePrincipal::None,
        nns_governance,
        "set_icp_xdr_conversion_rate",
        (UpdateIcpXdrConversionRatePayload {
            data_source: "".to_string(),
            timestamp_seconds: u64::MAX, //pic.get_time().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 5*60,
            xdr_permyriad_per_icp: cmc_rate as u64,
        },)
    ).unwrap();
    r.unwrap();
    
    // BANK
    pic.create_canister_with_id(Some(CTS_CONTROLLER), None, BANK).unwrap();
    pic.add_cycles(BANK, 1_000 * TRILLION);
    pic.install_canister(
        BANK, 
        std::fs::read(wasms_dir().join("bank.wasm")).unwrap(), 
        candid::encode_args(()).unwrap(), 
        Some(CTS_CONTROLLER), 
    );

    // CM_MAIN
    pic.create_canister_with_id(Some(CTS_CONTROLLER), None, CM_MAIN).unwrap();
    pic.add_cycles(CM_MAIN, 1_000 * TRILLION);
    pic.install_canister(
        CM_MAIN, 
        std::fs::read(wasms_dir().join("cm_main.wasm")).unwrap(), 
        candid::encode_one(CMMainInit {
            cts_id: CTS,
            cycles_bank_id: BANK,
        }).unwrap(), 
        Some(CTS_CONTROLLER), 
    );
    
    for (wasm_path, market_canister_type) in [
        ("cm_tc.wasm", MarketCanisterType::TradeContract),
        ("cm_positions_storage.wasm", MarketCanisterType::PositionsStorage),
        ("cm_trades_storage.wasm", MarketCanisterType::TradesStorage),
    ] {
        let cc = CanisterCode::new(std::fs::read(wasms_dir().join(wasm_path)).unwrap());
        call_candid_as::<_, ()>(&pic, CM_MAIN, RawEffectivePrincipal::None, CTS_CONTROLLER, "controller_upload_canister_code", (cc, market_canister_type)).unwrap();
    }
    
    pic
}

pub fn set_up_tc(pic: &PocketIc) -> Principal {
    call_candid_as::<_, (Result<ControllerCreateIcrc1TokenTradeContractSuccess, ControllerCreateIcrc1TokenTradeContractError>,)>(
        &pic, CM_MAIN, RawEffectivePrincipal::None, CTS_CONTROLLER, "controller_create_trade_contract", (
            ControllerCreateIcrc1TokenTradeContractQuest {
                icrc1_ledger_id: ICP_LEDGER,
                icrc1_ledger_transfer_fee: ICP_LEDGER_TRANSFER_FEE,
            },
        )
    ).unwrap().0.unwrap().trade_contract_canister_id
}


pub fn set_up_canister_caller(pic: &PocketIc) -> Principal {
    let canister_caller: Principal = pic.create_canister();
    let canister_caller_wasm: Vec<u8> = std::fs::read(wasms_dir().join("canister_caller.wasm")).unwrap();
    pic.add_cycles(canister_caller, 1_000_000_000 * TRILLION);
    pic.install_canister(
        canister_caller, 
        canister_caller_wasm, 
        candid::encode_args(()).unwrap(),
        None,
    );
    canister_caller
}

