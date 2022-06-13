use ethers::abi::{Address, Token};
use ethers_providers::Middleware;
use ethers_solc::resolver::print;
use ethers_solc::{Artifact, Project, ProjectPathsConfig};
use evm::backend::{MemoryAccount, MemoryBackend, MemoryVicinity};
use evm::executor::stack::{MemoryStackState, StackExecutor, StackSubstateMetadata};
use evm::{Capture, Config, ExitReason, Handler};
use eyre::{eyre, ContextCompat, Result};
use primitive_types::{H160, H256, U256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::slice::SliceIndex;
use std::str::FromStr;

fn simple_run() -> Result<()> {
    fn prn() {
        println!("{}", "=".repeat(80));
    }

    let owner = H160::from_str("0xf000000000000000000000000000000000000000")?;
    let to = H160::from_str("0x1000000000000000000000000000000000000000")?;
    // Target contract to be tested
    let contract_name = "SimpleToken";
    let get_balance_function_name = Some("balanceOf");
    let state_function_name = "transfer";
    // let contract_name = "Branching";
    // let get_balance_function_name: Option<&str> = None;
    // let state_function_name = "run";

    // Arguments in the target function
    let mut fargs: Vec<Token> = Vec::new();
    fargs.push(Token::Address(owner));
    fargs.push(Token::Uint(U256::from(999u64)));

    // Compile project
    let root = "contracts";
    let root = PathBuf::from(root);
    if !root.exists() {
        return Err(eyre!("Project root {root:?} does not exists!"));
    }

    let paths = ProjectPathsConfig::builder()
        .root(&root)
        .sources(&root)
        .build()?;

    let project = Project::builder()
        .paths(paths)
        .set_auto_detect(true) // auto detect solc version from solidity source code
        .no_artifacts()
        .build()?;

    let output = project.compile()?;
    if output.has_compiler_errors() {
        return Err(eyre!(
            "Compiling solidity project failed: {:?}",
            output.output().errors
        ))?;
    }

    // Print the contracts we found in the project
    for (id, _) in output.clone().into_artifacts() {
        let name = id.name;
        println!("{name}");
    }

    let contract = output.find(contract_name).context("Contract not found")?;
    let contract_bytecode = contract
        .clone()
        .into_bytecode_bytes()
        .context("Missing bytecode")?
        .to_vec();

    // println!("conctract: {}", hex::encode(&contract_bytecode));
    let mut balance_bin = None;
    if let Some(get_balance_function_name) = get_balance_function_name {
        let args: Vec<Token> = vec![Token::Address(owner)];
        let abi = contract
            .clone()
            .into_abi()
            .context("Missing ABI in contract")?;
        let func = abi
            .functions()
            .find(|ref func| func.name.eq(get_balance_function_name))
            .context("Balance function not found")?;
        let encoded_bin = func.encode_input(&args)?;

        println!("get_balance bin: {}", hex::encode(&encoded_bin));
        balance_bin = Some(encoded_bin);
    }

    let abi = contract
        .clone()
        .into_abi()
        .context("Missing ABI in contract")?;

    let fcall = abi
        .functions()
        .find(|ref func| func.name.eq(state_function_name))
        .context("Target function not found")?;

    // transfering 9999 ERC tokens
    let args = vec![Token::Address(to), Token::Uint(U256::from(9999))];
    let fcall_bin = fcall.encode_input(&args)?;
    println!("fcall: {fcall:?} {}", hex::encode(&fcall_bin));
    println!("fcall bytecode: {fcall_bin:?}");

    // Create EVM instance
    let config = Config::istanbul();

    let vicinity = MemoryVicinity {
        gas_price: U256::default(),
        origin: H160::default(),
        chain_id: U256::one(),
        block_hashes: Vec::new(),
        block_number: Default::default(),
        block_coinbase: Default::default(),
        block_timestamp: Default::default(),
        block_difficulty: Default::default(),
        block_gas_limit: Default::default(),
        block_base_fee_per_gas: U256::zero(),
    };

    // EVM initial state
    let mut state = BTreeMap::new();
    state.insert(
        to,
        MemoryAccount {
            nonce: U256::one(),
            balance: U256::from(1000000000000u64),
            storage: BTreeMap::new(),
            code: Vec::new(), // Put contract code here, this account will own the contract
        },
    );

    state.insert(
        owner,
        MemoryAccount {
            nonce: U256::one(),
            balance: U256::from(10000000000000000u64),
            storage: BTreeMap::new(),
            code: Vec::new(),
        },
    );

    // Start EVM
    let backend = MemoryBackend::new(&vicinity, state);
    let metadata = StackSubstateMetadata::new(u64::MAX, &config);
    let state = MemoryStackState::new(metadata, &backend);
    let precompiles = BTreeMap::new();
    let mut executor = StackExecutor::new_with_precompiles(state, &config, &precompiles);

    // Execute state changes on EVM
    prn();
    println!("EXECUTE contract deploy");
    let reason = executor.create(
        owner,
        evm::CreateScheme::Legacy { caller: owner },
        U256::default(),
        contract_bytecode.clone(),
        None,
    );
    println!("RETURNS {reason:?} ");

    let mut contract_address: Address = Address::default();
    if let Capture::Exit((_reason, address, _return_data)) = reason {
        println!("Contract deployed to adderss {address:?} ");
        contract_address = address.context("Missing contract address, deployment failed")?;
    }

    if let Some(ref balance_bin) = balance_bin {
        prn();
        println!("EXECUTE get balance method");
        let reason = executor.transact_call(
            owner,
            contract_address,
            U256::zero(),
            balance_bin.clone(),
            u64::MAX,
            Vec::new(),
        );
        println!("RETURNS {reason:?} ");
        if let (ExitReason::Succeed(_), data) = reason {
            let balance = U256::from_big_endian(data.as_slice());
            println!("Balance: {balance}");
        }
    }

    prn();
    println!("EXECUTE contract method");
    let reason = executor.transact_call(
        owner,
        contract_address,
        U256::zero(),
        fcall_bin.clone(),
        u64::MAX,
        Vec::new(),
    );
    println!("RETURNS {reason:?} ");

    if let Some(ref balance_bin) = balance_bin {
        prn();
        println!("EXECUTE get balance method");
        let reason = executor.transact_call(
            owner,
            contract_address,
            U256::zero(),
            balance_bin.clone(),
            u64::MAX,
            Vec::new(),
        );
        println!("RETURNS {reason:?} ");
        if let (ExitReason::Succeed(_), data) = reason {
            let balance = U256::from_big_endian(data.as_slice());
            println!("Balance: {balance}");
        }
    }

    prn();
    let state = executor.state().clone();
    println!("Complte state: {state:?}");

    // prn();
    // let mut state = executor.state().clone();
    // This only returns the stack account, which contains no information about the storge data
    // let contract_account = state.account_mut(contract_address);
    // println!("Contract state: {:?}", contract_account);

    // prn();
    // let mut state = executor.state().clone();
    // let owner_account = state.account_mut(owner);
    // println!("Account state: {:?}", owner_account);

    prn();
    for i in 0..10 {
        let contract_storage = executor
            .storage(contract_address, H256::from_low_u64_be(i))
            .clone();
        println!("Contract storage: {:?}", contract_storage);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_run()
}
