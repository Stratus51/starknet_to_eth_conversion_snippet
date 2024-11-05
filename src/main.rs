use alloy_consensus::{transaction::TxEnvelope, Header};
use alloy_primitives::{Parity, Uint};
use reth_primitives::{Transaction as EthTransaction, TransactionSigned};
use starknet::{
    core::types::{
        BlockId, BlockWithReceipts, Felt, InvokeTransaction, InvokeTransactionReceipt,
        InvokeTransactionV1, MaybePendingBlockWithReceipts, TransactionReceipt,
        TransactionWithReceipt,
    },
    providers::{
        jsonrpc::{HttpTransport, JsonRpcClient},
        Provider, Url,
    },
};

// =====================================================================================
// Constants
// =====================================================================================
const KAKAROT_ADDRESS: Felt =
    Felt::from_hex_unchecked("1d2e513630d8120666fc6e7d52ad0c01479fd99c183baac79fff9135f46e359");
const TARGET_BLOCK: u64 = 260017;

// =====================================================================================
// Main
// =====================================================================================
#[tokio::main]
async fn main() {
    let starknet_block = get_starknet_block(TARGET_BLOCK).await;
    println!("Starknet block: {starknet_block:#?}");

    // Convert block
    let eth_block = convert_block(starknet_block).await;
    println!("Eth block: {eth_block:#?}");
}

// =====================================================================================
// Conversion
// =====================================================================================
#[derive(Debug)]
struct EthBlockParts {
    header: Header,
    transactions: Vec<TransactionSigned>,
}

async fn convert_block(block: BlockWithReceipts) -> EthBlockParts {
    let mut transactions = vec![];
    println!("Nb transactions: {}", block.transactions.len());

    // TODO Properly build logs_bloom
    let logs_bloom = alloy_primitives::Bloom::ZERO;

    // XXX Pull ref block to debug
    let ref_block = get_kakarot_block(TARGET_BLOCK).await;
    println!(
        "kakarot_block: nb_transactions = {}",
        ref_block.transactions.len()
    );
    if let alloy_rpc_types_eth::BlockTransactions::Full(list) = &ref_block.transactions {
        for transaction in list {
            let transaction = transaction.clone();
            let tx_envelope = TxEnvelope::try_from(transaction).unwrap();
            use alloy_rlp::Encodable;
            let mut buf = vec![];
            println!("ty: {:?}", tx_envelope.tx_type());
            match tx_envelope {
                TxEnvelope::Eip2930(tx) => tx.tx().encode(&mut buf),
                TxEnvelope::Eip1559(tx) => tx.tx().encode(&mut buf),
                tx => panic!("Bad TX: {tx:#?}"),
            }
            println!("-> {}", hex::encode_upper(&buf))
        }
    } else {
        panic!();
    }

    // let mut transactions = vec![];
    let mut cumulative_gas_used = Felt::ZERO;
    for (transaction, receipt) in block.transactions.iter().filter_map(is_kakarot_transaction) {
        // XXX Debug
        println!("{transaction:#?}");

        // Accumulate gas used
        for event in &receipt.events {
            let gas_used = event.data.last().copied().unwrap_or(Felt::ZERO);
            cumulative_gas_used += gas_used;
        }

        // Convert transactions
        if let Some(transaction) = convert_transaction(transaction) {
            println!("transaction: {transaction:#?}");
            transactions.push(transaction);
        }

        // TODO Build receipt trie
        // TODO Build transaction trie
    }
    println!("Found {} transactions", transactions.len());

    // TODO Improvise root trie for example
    // TODO Build header

    todo!()
}

// https://github.com/kkrt-labs/kakarot-rpc/blob/2c468f5b8771bf03fa3ff44ba04b140401afb76f/indexer/src/utils/filter.ts#L7
//
// TODO Check: Can irrelevant types of transaction exist with KAKAROT_ADDRESS on this calldata index (9)?
fn is_kakarot_transaction(
    raw: &TransactionWithReceipt,
) -> Option<(&InvokeTransactionV1, &InvokeTransactionReceipt)> {
    use starknet::core::types::Transaction;
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

fn convert_transaction(transaction: &InvokeTransactionV1) -> Option<TransactionSigned> {
    let call_data = CallData::parse(&transaction.calldata).unwrap();

    // Multi-calls are not supported for now.
    if call_data.call_array_len != Felt::ONE {
        return None;
    }

    let signature = call_data.signature;
    let r_low = signature[0];
    let r_high = signature[1];
    let s_low = signature[2];
    let s_high = signature[3];
    let v = signature[4];

    // TODO Check that taking the lower bytes of _low and _high is the right conversion to do
    fn parse_uint256(low: Felt, high: Felt) -> Uint<256, 4> {
        let low = low.to_bytes_be();
        let high = high.to_bytes_be();
        let data: [u8; 32] = [&high[16..], &low[16..]].concat().try_into().unwrap();
        Uint::<256, 4>::from_be_bytes(data)
    }
    let r = parse_uint256(r_low, r_high);
    let s = parse_uint256(s_low, s_high);

    // TODO Check if u64 parity for Eip155 should be different
    let v = Parity::Parity(v != Felt::ZERO);
    let signature = reth_primitives::Signature::new(r, s, v);

    let calldata_without_signature = &transaction.calldata[8..transaction.calldata.len() - 6];
    let data_len: usize = call_data.calldata_len.to_biguint().try_into().unwrap();
    let mut nb_felts = data_len / 32;
    let mut remaining_bytes = data_len % 32;
    if remaining_bytes == 0 {
        remaining_bytes = 32;
        nb_felts += 1;
    }
    println!("remaining_bytes: {remaining_bytes}");
    let data = &calldata_without_signature[7..][..nb_felts + 1];
    let mut new_format_bytes: Vec<u8> = data[..data.len() - 1]
        .iter()
        .map(|felt| felt.to_bytes_be())
        .collect::<Vec<_>>()
        .concat();
    let offset = 32 - remaining_bytes;
    let last_felt = calldata_without_signature.last().unwrap();
    new_format_bytes.extend(&last_felt.to_bytes_be()[offset..]);
    assert_eq!(new_format_bytes.len(), data_len);

    println!("data_len: {data_len}");
    let new_format_bytes = &new_format_bytes[..];
    println!("Extracted data: {}", hex::encode_upper(new_format_bytes));

    // Parse transaction
    let transaction = parse_transaction(new_format_bytes)?;
    println!("transaction: {transaction:#?}");

    // Return
    Some(TransactionSigned::from_transaction_and_signature(
        transaction,
        signature,
    ))
}

fn parse_transaction(data: &[u8]) -> Option<EthTransaction> {
    use alloy_consensus::transaction::{TxEip1559, TxEip2930, TxLegacy, TxType};
    use alloy_rlp::Decodable;

    const TX_TYPE_EIP2930: u8 = TxType::Eip2930 as u8;
    const TX_TYPE_EIP1559: u8 = TxType::Eip1559 as u8;

    let tx_type = data[1];
    let mut data_ptr = &data[2..];
    if tx_type < 0x7f {
        println!("tx_type: {}", tx_type);
        match tx_type {
            TX_TYPE_EIP2930 => TxEip2930::decode(&mut data_ptr).map(|tx| tx.into()).ok(),
            TX_TYPE_EIP1559 => {
                println!("Parsing TxEip1559: {}", hex::encode_upper(data_ptr));
                TxEip1559::decode(&mut data_ptr).map(|tx| tx.into()).ok()
            }
            _ => None,
        }
    } else {
        TxLegacy::decode(&mut data_ptr).map(|tx| tx.into()).ok()
    }
}

// From https://github.com/kkrt-labs/kakarot-rpc/blob/15da170828f3281721a4c2995a47d64636d5607a/indexer/src/types/transaction.ts#L289
// [call_array_len, to, selector, data_offset, data_len, calldata_len, calldata, signature_len, signature]
struct CallData {
    // call_len: Felt,
    // to: Felt,
    // selector: Felt,
    // call_data_len: Felt,
    // outside_execution: CallDataOutsideExecution,
    call_array_len: Felt,
    // to2: Felt,
    // selector2: Felt,
    // data_offset: Felt,
    // data_len: Felt,
    calldata_len: Felt,
    // call_data: Vec<Felt>,
    // signature_len: Felt
    signature: Vec<Felt>,
}

// struct CallDataOutsideExecution {
//     caller: Felt,
//     nonce: Felt,
//     after: Felt,
//     before: Felt,
// }

impl CallData {
    fn parse(raw: &[Felt]) -> Option<Self> {
        assert!(raw.len() > 17);
        let data_len = raw[12];
        let usize_data_len = usize::try_from(data_len.to_biguint()).unwrap();
        let sig_offset = 12 + 1 + usize_data_len + 1;
        let signature_len = raw[sig_offset];
        let usize_signature_len = usize::try_from(signature_len.to_biguint()).unwrap();
        let signature = raw[sig_offset + 1..].to_vec();
        assert_eq!(usize_signature_len, signature.len());
        Some(Self {
            // call_len: raw[0],
            // to: raw[1],
            // selector: raw[2],
            // call_data_len: raw[3],
            // outside_execution: CallDataOutsideExecution {
            //     caller: raw[4],
            //     nonce: raw[5],
            //     after: raw[6],
            //     before: raw[7],
            // },
            call_array_len: raw[8],
            // to2: raw[9],
            // selector2: raw[10],
            // data_offset: raw[11],
            // data_len,
            calldata_len: raw[14],
            // call_data: ,
            signature,
        })
    }
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

async fn get_kakarot_block(id: u64) -> alloy_rpc_types_eth::Block {
    use reth_consensus_debug_client::BlockProvider;
    let client = reth_consensus_debug_client::RpcBlockProvider::new(
        "https://sepolia-rpc.kakarot.org".to_string(),
    );

    client.get_block(id).await.unwrap()
}
