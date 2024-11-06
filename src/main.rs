use alloy_consensus::transaction::{TxEip1559, TxEnvelope};
use alloy_primitives::Address;
use alloy_rlp::{Decodable, Encodable};
use clap::Parser;
use reth_consensus_debug_client::BlockProvider;
use starknet::{
    core::types::{BlockId, BlockWithReceipts, Felt, MaybePendingBlockWithReceipts},
    providers::{
        jsonrpc::{HttpTransport, JsonRpcClient},
        Provider, Url,
    },
};

// =====================================================================================
// Modules
// =====================================================================================
mod starknet_block_converter;
mod starknet_call_data;
mod starknet_transaction_converter;

use starknet_block_converter::StarknetBlockConverter;

// =====================================================================================
// Constants
// =====================================================================================
const DEFAULT_ADDRESS: Felt =
    Felt::from_hex_unchecked("1d2e513630d8120666fc6e7d52ad0c01479fd99c183baac79fff9135f46e359");
const DEFAULT_BLOCK: u64 = 260017;

// =====================================================================================
// CLI arguments
// =====================================================================================
#[derive(Parser, Debug)]
struct Args {
    /// Starknet account address possessing the transactions
    #[arg(long, default_value_t = DEFAULT_ADDRESS)]
    account: Felt,
    /// Starknet block ID to convert
    #[arg(long, default_value_t = DEFAULT_BLOCK)]
    block: u64,
    /// Debug mode
    #[clap(long, default_value = "false")]
    debug: bool,
}

// =====================================================================================
// Main
// =====================================================================================
#[tokio::main]
async fn main() {
    // CLI arguments
    let args = Args::parse();

    // Setup logger
    env_logger::builder()
        .filter_level(if args.debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .init();

    // Build block converter
    let mut converter = StarknetBlockConverter::new(args.account, Address::ZERO);

    // Pull Starknet block
    let starknet_block = get_starknet_block(args.block).await;
    log::info!("Starknet block: {starknet_block:#?}");

    // XXX Debug print
    print_reference_block(args.block).await;

    // Convert block
    let eth_block = converter
        .convert(starknet_block)
        .await
        .expect("Conversion should work");
    log::info!("Eth block: {eth_block:#?}");
}

// =====================================================================================
// Fetch block helpers
// =====================================================================================
async fn get_starknet_block(id: u64) -> BlockWithReceipts {
    let provider = JsonRpcClient::new(HttpTransport::new(
        Url::parse("https://starknet-sepolia.public.blastapi.io/rpc/v0_7").unwrap(),
    ));

    match provider
        .get_block_with_receipts(BlockId::Number(id))
        .await
        .expect("Block should be found")
    {
        MaybePendingBlockWithReceipts::Block(block) => block,
        _ => panic!("Block should not be pending"),
    }
}

// =====================================================================================
// XXX Debug helper
// =====================================================================================
async fn print_reference_block(id: u64) {
    // XXX Pull ref block to debug
    let client = reth_consensus_debug_client::RpcBlockProvider::new(
        "https://sepolia-rpc.kakarot.org".to_string(),
    );
    let ref_block = client.get_block(id).await.unwrap();

    println!(
        "reference_block: nb_transactions = {}",
        ref_block.transactions.len()
    );
    if let alloy_rpc_types_eth::BlockTransactions::Full(list) = &ref_block.transactions {
        for transaction in list {
            let transaction = transaction.clone();
            let tx_envelope = TxEnvelope::try_from(transaction).unwrap();
            let mut buf = vec![];
            println!("ty: {:?}", tx_envelope.tx_type());
            match &tx_envelope {
                TxEnvelope::Eip2930(tx) => tx.tx().encode(&mut buf),
                TxEnvelope::Eip1559(tx) => tx.tx().encode(&mut buf),
                tx => panic!("Bad TX: {tx:#?}"),
            }

            // XXX
            println!("-> {}", hex::encode_upper(&buf));
            if let TxEnvelope::Eip1559(_) = tx_envelope {
                println!("Reparsing: {:?}", TxEip1559::decode(&mut buf.as_slice()))
            }
        }
    } else {
        panic!();
    }
}
