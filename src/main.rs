use ethers::abi::{Address, Token};
use ethers_solc::artifacts::output_selection::ContractOutputSelection;
use ethers_solc::{Artifact, Project, ProjectPathsConfig, SolcConfig};
use evm::backend::{MemoryAccount, MemoryBackend, MemoryVicinity};
use evm::executor::stack::{MemoryStackState, StackExecutor, StackSubstateMetadata};
use evm::{Capture, Config, Context, ExitReason, Handler};
use eyre::{eyre, ContextCompat, Result};
use hex::ToHex;
use maplit::btreemap;
use primitive_types::{H160, H256, U256};
use sha3::{Digest, Keccak256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

fn simple_run() -> Result<()> {
    fn prn() {
        println!("{}", "=".repeat(80));
    }

    // Two addresses we'll give some ethers to start with
    let owner = H160::from_str("0xf000000000000000000000000000000000000000")?;
    let to = H160::from_str("0x1000000000000000000000000000000000000000")?;

    // Target contract to be tested
    let contract_name = "C";
    let get_balance_function_name = Some("balanceOf");
    let state_function_name = "transfer";

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

    let solc_config = SolcConfig::builder()
        .additional_output(ContractOutputSelection::StorageLayout)
        .build();

    let project = Project::builder()
        .paths(paths)
        .set_auto_detect(true) // auto detect solc version from solidity source code
        .solc_config(solc_config)
        .build()?;

    let output = project.compile()?;
    if output.has_compiler_errors() {
        return Err(eyre!(
            "Compiling solidity project failed: {:?}",
            output.output().errors
        ))?;
    }

    // Print the contracts we found in the project
    prn();
    println!("Found contracts in project:");
    for (id, _) in output.clone().into_artifacts() {
        let name = id.name;
        println!("{name}");
    }

    let contract = output.find(contract_name).context("Contract not found")?;
    let contract_deploy_hex: String = contract
        .clone()
        .into_bytecode_bytes()
        .context("Missing bytecode")?
        .encode_hex_upper();
    println!("Contract deploy hex:\n{}", contract_deploy_hex);
    let contract_bytecode = contract
        .clone()
        .into_bytecode_bytes()
        .context("Missing bytecode")?
        .to_vec();
    let abi = contract
        .clone()
        .into_abi()
        .context("Missing ABI in contract")?;
    let deployed_hex: String = contract
        .clone()
        .deployed_bytecode
        .context("Missing deployed bytecode")?
        .bytecode
        .context("Missing deployed bytecode bytes")?
        .object
        .encode_hex();
    println!("Contract deployed bytecode as hex:\n{}", deployed_hex);

    prn();
    // Create bytecode for query balance from ABI and function parameters
    let mut balance_bin = None;
    if let Some(get_balance_function_name) = get_balance_function_name {
        let args: Vec<Token> = vec![Token::Address(owner)];
        let func = abi
            .functions()
            .find(|ref func| func.name.eq(get_balance_function_name))
            .context("Balance function not found")?;
        let encoded_bin = func.encode_input(&args)?;

        println!("balanceOf(owner) bin: {}", hex::encode(&encoded_bin));
        balance_bin = Some(encoded_bin);
    }

    let storage_layout = contract.clone().storage_layout;
    // Unfortunately storage layout is not available for old versions of solidity
    if let Some(ref layout) = storage_layout {
        println!("Storage layout: {:?}", layout);
    }

    prn();
    let fcall = abi
        .functions()
        .find(|ref func| func.name.eq(state_function_name))
        .context("Target function not found")?;

    // transfering 9999 ERC tokens
    let args = vec![Token::Address(to), Token::Uint(U256::from(9999))];
    let fcall_bin = fcall.encode_input(&args)?;
    println!(
        "transfer(to, 9999) bin: {fcall:?} {}",
        hex::encode(&fcall_bin)
    );

    // Configure EVM executor and set initial state
    let config = Config::istanbul();
    let vicinity = MemoryVicinity {
        gas_price: U256::zero(),
        origin: H160::default(),
        chain_id: U256::one(),
        block_hashes: Vec::new(),
        block_number: Default::default(),
        block_coinbase: Default::default(),
        block_timestamp: Default::default(),
        block_difficulty: U256::one(),
        block_gas_limit: U256::zero(),
        block_base_fee_per_gas: U256::zero(),
    };

    // Initial state
    let initial_state = btreemap! {
        owner => MemoryAccount {
            nonce: U256::zero(),
            balance: U256::from(66_666_666u64),
            storage: BTreeMap::new(),
            code: Vec::new(),
        },
        to =>  MemoryAccount {
            nonce: U256::one(),
            balance: U256::from(77_777_777u64),
            storage: BTreeMap::new(),
            code: Vec::new(), // Put contract code here, this account will own the contract
        },
    };

    // Create executor
    let backend = MemoryBackend::new(&vicinity, initial_state);
    let gas_limit = u64::MAX; // max gas limit allowed in this execution
    let metadata = StackSubstateMetadata::new(gas_limit, &config);
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
        let context = Context {
            address: contract_address,
            caller: owner,
            apparent_value: U256::zero(),
        };
        prn();
        let gas_start = executor.used_gas();
        println!("EXECUTE get balance method");
        let reason = executor.call(
            contract_address,
            None,
            balance_bin.clone(),
            None,
            true,
            context,
        );
        println!("RETURNS {reason:?} ");
        if let Capture::Exit((ExitReason::Succeed(_), data)) = reason {
            let balance = U256::from_big_endian(data.as_slice());
            println!("Balance: {balance}");
        }
        let gas_used = executor.used_gas() - gas_start;
        println!("Gas used in getting balance transaction {gas_used}");
    }

    prn();
    for _ in 0..2 {
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
            // let balance = hex::encode(&data);
            println!("Balance: {balance}");
        }
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
            // let balance = hex::encode(&data);
            println!("Balance: {balance}");
        }
    }

    prn();
    let state = executor.state().clone();
    println!("Complete state:\n{state:#?}");

    // Decode account data
    prn();
    let mut data: Vec<u8> = Vec::new();
    let slot = 6;
    let mut key = H256::from(owner.clone()).as_bytes().to_owned().into();
    let mut position: Vec<u8> = H256::from_low_u64_be(slot).as_bytes().to_owned().into();

    data.append(&mut key);
    data.append(&mut position);
    let idx = Keccak256::digest(data.as_slice());
    let idx = H256::from_slice(idx.as_slice());

    let contract_storage = executor.storage(contract_address, idx).clone();
    let balance = U256::from(contract_storage.as_bytes());
    if balance != U256::zero() {
        println!(
            "Decoded Contract storage value at slot {} with idx {}: {}",
            slot,
            idx.encode_hex::<String>(),
            balance,
        );
    }

    // Decode log
    prn();
    let (_, logs) = executor.state_mut().clone().deconstruct();
    println!("Decoded Logs:");
    for log in logs {
        println!(
            "Source {} From {} To {} Amount {}",
            log.address.encode_hex::<String>(),
            log.topics[1].encode_hex::<String>(),
            log.topics[2].encode_hex::<String>(),
            U256::from_big_endian(&log.data),
        );
    }

    // Total gas usage
    prn();
    let gas_used = executor.used_gas();
    println!("Gas used: {}", gas_used);

    println!("Gas left: {}", executor.gas_left());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_run()
}
