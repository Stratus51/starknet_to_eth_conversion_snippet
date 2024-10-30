use starknet::{
    core::types::{
        BlockId, BlockWithReceipts, Felt, InvokeTransaction, InvokeTransactionReceipt,
        InvokeTransactionV1, MaybePendingBlockWithReceipts, Transaction, TransactionReceipt,
        TransactionWithReceipt,
    },
    providers::{
        jsonrpc::{HttpTransport, JsonRpcClient},
        Provider, Url,
    },
};

// eth types
use alloy_consensus::Header;
use reth_primitives::TransactionSigned;

// starknet types
// use starknet_api::{block::BlockHeader, transaction::InvokeTransactionV3};

const KAKAROT_ADDRESS: Felt =
    Felt::from_hex_unchecked("1d2e513630d8120666fc6e7d52ad0c01479fd99c183baac79fff9135f46e359");
const TARGET_BLOCK: u64 = 260017;

#[tokio::main]
async fn main() {
    let provider = JsonRpcClient::new(HttpTransport::new(
        Url::parse("https://starknet-sepolia.public.blastapi.io/rpc/v0_7").unwrap(),
    ));

    // Fetch targeted block
    let block = match provider
        .get_block_with_receipts(BlockId::Number(TARGET_BLOCK))
        .await
        .expect("Block should be found")
    {
        MaybePendingBlockWithReceipts::Block(block) => block,
        _ => panic!("Block should not be pending"),
    };
    println!("Starknet block: {block:#?}");

    // Convert block
    let eth_block = convert(block);
    println!("Eth block: {eth_block:#?}");
}

#[derive(Debug)]
struct EthBlock {
    header: Header,
    transactions: Vec<TransactionSigned>,
}

// https://github.com/kkrt-labs/kakarot-rpc/blob/2c468f5b8771bf03fa3ff44ba04b140401afb76f/indexer/src/utils/filter.ts#L7
//
// TODO Check: Can irrelevant types of transaction exist with KAKAROT_ADDRESS on this calldata index (9)?
fn is_kakarot_transaction(
    raw: &TransactionWithReceipt,
) -> Option<(&InvokeTransactionV1, &InvokeTransactionReceipt)> {
    // TODO Check whether ethereum validation has failed or not: https://github.com/kkrt-labs/kakarot-rpc/blob/2c468f5b8771bf03fa3ff44ba04b140401afb76f/indexer/src/utils/filter.ts#L36
    if let (
        Transaction::Invoke(InvokeTransaction::V1(transaction)),
        TransactionReceipt::Invoke(receipt),
    ) = (&raw.transaction, &raw.receipt)
    {
        if transaction.calldata.get(9) == Some(&KAKAROT_ADDRESS) {
            return Some((transaction, receipt));
        }
    }
    None
}

fn convert(block: BlockWithReceipts) -> EthBlock {
    let mut transactions = vec![];
    println!("Nb transactions: {}", block.transactions.len());

    // TODO Properly build logs_bloom
    let logs_bloom = alloy_primitives::Bloom::ZERO;

    // let mut transactions = vec![];
    let mut cumulative_gas_used = Felt::ZERO;
    for (transaction, receipt) in block.transactions.iter().filter_map(is_kakarot_transaction) {
        // XXX Debug
        println!("{transaction:#?}");
        println!("Calldata:");
        for d in &transaction.calldata {
            println!("  - {d:?}");
        }

        // Accumulate gas used
        for event in &receipt.events {
            let gas_used = event.data.last().copied().unwrap_or(Felt::ZERO);
            cumulative_gas_used += gas_used;
        }

        // Convert transactions
        // Strange transition filter condition "isRevertedWithOutOfResources"
        // https://github.com/kkrt-labs/kakarot-rpc/blob/2c468f5b8771bf03fa3ff44ba04b140401afb76f/indexer/src/main.ts#L259
        if let Some(transaction) = convert_transaction(transaction) {
            transactions.push(transaction);
        }
    }
    todo!()
}

fn convert_transaction(transaction: &InvokeTransactionV1) -> Option<()> {
    if transaction.calldata.len() < 15 {
        return None;
    }
    let calldata = &transaction.calldata[8..];

    // Multi-calls are not supported for now.
    let call_array_len = calldata[0];
    if call_array_len != Felt::ONE {
        return None;
    }

    let eth_data_len = calldata[5].to_bigint().to_u64_digits().1[0] as usize;
    let signature = &calldata[5 + 1 + eth_data_len + 1..];
    if signature.len() != 5 {
        return None;
    }

    let r_low = signature[0];
    let r_high = signature[1];
    let s_low = signature[2];
    let s_high = signature[3];
    let v = signature[4].to_bigint();

    // TODO Check if this is correct
    let r = (r_high.to_biguint() << 16) | r_low.to_biguint();
    let s = (s_high.to_biguint() << 16) | s_low.to_biguint();

    let calldata_without_signature = &transaction.calldata[..transaction.calldata.len() - 6];
    let new_format_bytes = todo!();
}

// TODO Collect a test sample to check if the conversion works as intended
