#![feature(test)]

extern crate test;
use ethers::abi::{Address, Token};
use ethers::prelude::k256::elliptic_curve::ops::ReduceNonZero;
use ethers::prelude::k256::pkcs8::der::Encodable;
use ethers_solc::{Artifact, Project, ProjectPathsConfig};
use evm::backend::{MemoryAccount, MemoryBackend, MemoryVicinity};
use evm::executor::stack::{MemoryStackState, StackExecutor, StackSubstateMetadata};
use evm::{Capture, Config, Handler};
use eyre::{eyre, ContextCompat, Result, WrapErr};
use primitive_types::{H160, U256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

fn simple_run() -> Result<()> {
    fn printline() {
        println!("{}", "=".repeat(80));
    }

    let owner = H160::from_str("0xf000000000000000000000000000000000000000")?;
    // Target contract to be tested
    // let contract_name = "SimpleToken";
    // let get_balance_function_name = "balanceOf";
    // let transfer_function_name = "transfer";
    let contract_name = "Branching";
    let get_balance_function_name: Option<&str> = None;
    let state_function_name = "run";

    // Arguments in the target function
    let mut fargs: Vec<Token> = Vec::new();
    // fargs.push(Token::Address(owner));
    // fargs.push(Token::Uint(U256::from(999u64)));

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
        .context("Missing bytecodexunq ")?
        .to_vec();

    println!("conctract: {}", hex::encode(&contract_bytecode));
    if let Some(get_balance_function_name) = get_balance_function_name {
        let balance_args: Vec<Token> = vec![Token::Address(owner)];
        let get_erc_balance_bin = contract
            .clone()
            .into_abi()
            .context("Missing ABI in contract")?
            .functions()
            .find(|ref func| func.name.eq(get_balance_function_name))
            .context("Balance function not found")?
            .encode_input(&balance_args)?
            .to_vec()
            .map_err(|_| eyre!("Encoding get_balance function args failed"))?;
        println!("get_balance: {}", hex::encode(&get_erc_balance_bin));
    }

    let abi = contract
        .clone()
        .into_abi()
        .context("Missing ABI in contract")?;

    let fcall = abi
        .functions()
        .find(|ref func| func.name.eq(state_function_name))
        .context("Target function not found")?;

    let fcall_sig = fcall.clone().short_signature();
    let v = Vec::from(fcall_sig);
    println!("fcall_sig: {v:?}");

    let fcall_bin: Vec<u8> = Vec::from(fcall.clone().short_signature());
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
        H160::from_str("0x1000000000000000000000000000000000000000")?,
        MemoryAccount {
            nonce: U256::one(),
            balance: U256::from(1000000000000u64),
            storage: BTreeMap::new(),
            code: hex::decode("6080604052348015600f57600080fd5b506004361060285760003560e01c80630f14a40614602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b6000806000905060005b83811015608f5760018201915080806001019150506076565b508091505091905056fea26469706673582212202bc9ec597249a9700278fe4ce78da83273cb236e76d4d6797b441454784f901d64736f6c63430007040033")?,
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

    // println!("EXECUTE get contract balance");
    // let reason = executor.transact_call(
    //     owner,
    //     contract_address,
    //     U256::zero(),
    //     get_erc_balance_bin,
    //     u64::MAX,
    //     Vec::new(),
    // );
    // println!("RETURNS {reason:?} ");

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

    println!("EXECUTE contract method");
    let reason = executor.transact_call(
        owner,
        H160::from_str("0x1000000000000000000000000000000000000000")?,
        U256::zero(),
        hex::decode("0f14a4060000000000000000000000000000000000000000000000000000000000002ee0")?,
        u64::MAX,
        Vec::new(),
    );
    println!("RETURNS {reason:?} ");

    let start = Instant::now();
    let num_execs = 0;

    printline();
    (0..num_execs).for_each(|_| {
        let reason = executor.transact_call(
            owner,
            contract_address,
            U256::zero(),
            fcall_bin.clone(),
            u64::MAX,
            Vec::new(),
        );
        println!("RETURNS {reason:?} ");
    });

    printline();
    let contract = hex::decode("608060405234801561001057600080fd5b50610402806100206000396000f300608060405260043610610057576000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff16806318160ddd1461005c57806370a0823114610087578063a9059cbb146100de575b600080fd5b34801561006857600080fd5b50610071610143565b6040518082815260200191505060405180910390f35b34801561009357600080fd5b506100c8600480360381019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919050505061014d565b6040518082815260200191505060405180910390f35b3480156100ea57600080fd5b50610129600480360381019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610195565b604051808215151515815260200191505060405180910390f35b6000600154905090565b60008060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020549050919050565b60008073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff16141515156101d257600080fd5b6000803373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054821115151561021f57600080fd5b610273600183016000803373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020546103ba90919063ffffffff16565b6000803373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002081905550610309600183016000808673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020546103ba90919063ffffffff16565b6000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508273ffffffffffffffffffffffffffffffffffffffff163373ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef846040518082815260200191505060405180910390a36001905092915050565b600081830190508281101515156103cd57fe5b809050929150505600a165627a7a72305820c80f9a59303c6c6d3a43ba89c4628606b06e441ee007c7516df3a6e356f3b2f20029")?;
    let method = hex::decode("a9059cbb00000000000000000000000000000000000000000000000000000000deadbeef0000000000000000000000000000000000000000000000000000000000000000")?;

    println!("EXECUTE contract deploy");
    let reason = executor.create(
        owner,
        evm::CreateScheme::Legacy { caller: owner },
        U256::default(),
        contract.clone(),
        None,
    );
    println!("RETURNS {reason:?} ");

    if let Capture::Exit((_reason, address, _return_data)) = reason {
        println!("Contract deployed to adderss {address:?} ");
        contract_address = address.context("Missing contract address, deployment failed")?;
    }

    let start = Instant::now();
    let num_execs = 100_000;
    (0..num_execs).for_each(|_| {
        let _ = executor.transact_call(
            owner,
            contract_address,
            U256::zero(),
            method.clone(),
            u64::MAX,
            Vec::new(),
        );
        // println!("RETURNS {reason:?} ");
    });

    let duration = start.elapsed();
    if num_execs > 0 {
        println!(
            "{} runs, total {}ms, average: {}ms",
            num_execs,
            duration.as_millis(),
            duration.as_millis() as f32 / num_execs as f32,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;
    static BENCH_SIZE: usize = 20;

    #[bench]
    fn loop_test(b: &mut Bencher) {
        (0..BENCH_SIZE).for_each(|_| b.iter(simple_run));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_run()
}
