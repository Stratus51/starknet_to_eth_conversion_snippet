use starknet::{
    core::types::{
        BlockId, BlockWithTxs, InvokeTransaction, MaybePendingBlockWithTxs, Transaction,
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

const TARGET_BLOCK: u64 = 260017;

#[tokio::main]
async fn main() {
    // Fetch targeted block
    let provider = JsonRpcClient::new(HttpTransport::new(
        Url::parse("https://starknet-sepolia.public.blastapi.io/rpc/v0_7").unwrap(),
    ));

    let block = match provider
        .get_block_with_txs(BlockId::Number(TARGET_BLOCK))
        .await
        .expect("Block should be found")
    {
        MaybePendingBlockWithTxs::Block(block) => block,
        _ => panic!("Block should not be pending"),
    };
    println!("Starknet block: {block:#?}");

    let eth_block = convert(block);
    println!("Eth block: {eth_block:#?}");
}

#[derive(Debug)]
struct EthBlock {
    header: Header,
    transactions: Vec<TransactionSigned>,
}

fn convert(block: BlockWithTxs) -> EthBlock {
    // let mut transactions = vec![];
    println!("Nb transactions: {}", block.transactions.len());
    for transaction in &block.transactions {
        if let Transaction::Invoke(InvokeTransaction::V3(transaction)) = transaction {
            println!("{transaction:#?}");
            // transactions.push(TransactionSigned {
            //     hash:,
            //     signature:,
            //     transaction:,
            // });
        }
    }
    todo!()
}
