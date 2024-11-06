use crate::{
    starknet_call_data::{self, CallData},
    starknet_transaction_converter::convert_transaction,
};
use alloy_consensus::{Block, BlockBody, Header};
use alloy_primitives::{Address, Bytes, B256, B64, U256};
use reth_primitives::TransactionSigned;
use starknet::core::types::{
    BlockWithReceipts, Felt, Transaction as StarknetTransaction,
    TransactionReceipt as StarknetTransactionReceipt,
    TransactionWithReceipt as StarknetTransactionWithReceipt,
};

// XXX Should not be ignored in production code
#[allow(dead_code)]
#[derive(Debug)]
pub enum Error {
    GasUsedOverflow(Felt),
}

pub struct StarknetBlockConverter {
    source_account_address: Felt,
    target_account_address: Address,
    // TODO Add target blockchain state infos
}

impl StarknetBlockConverter {
    pub fn new(source_account_address: Felt, target_account_address: Address) -> Self {
        Self {
            source_account_address,
            target_account_address,
        }
    }

    pub async fn convert(
        &mut self,
        block: BlockWithReceipts,
    ) -> Result<Block<TransactionSigned>, Error> {
        let mut transactions = vec![];
        // XXX Debug
        log::debug!("Nb transactions: {}", block.transactions.len());

        // TODO Properly build logs_bloom
        let logs_bloom = alloy_primitives::Bloom::ZERO;

        // let mut transactions = vec![];
        let mut cumulative_gas_used = Felt::ZERO;
        for StarknetTransactionWithReceipt {
            transaction,
            receipt,
        } in block.transactions.iter()
        {
            // Only process Invoke transactions
            let (transaction, receipt) = match (transaction, receipt) {
                (
                    StarknetTransaction::Invoke(transaction),
                    StarknetTransactionReceipt::Invoke(receipt),
                ) => (transaction, receipt),
                _ => continue,
            };

            // Parse transaction call data
            let call_data = match CallData::parse_from_transaction(transaction) {
                // Irrelevant call data
                Err((starknet_call_data::Error::CallDataTooSmall, _)) => continue,
                // Good call data
                Ok(call_data) => call_data,
                // Parsing error
                Err((error, raw)) => {
                    log::error!("Unparseable call_data: {error:?}\nRaw:{raw:#?}");
                    // TODO We should differentiate:
                    // - Unsupported call_data types: which can safely be ignored
                    // - Bad call_data/parsing fails: which have to be notified to us and fixed
                    continue;
                }
            };

            // Only process targeted source
            if call_data.to != self.source_account_address {
                continue;
            }

            // XXX Debug
            log::debug!("{transaction:#?}");

            // Accumulate gas used
            for event in &receipt.events {
                // https://github.com/kkrt-labs/kakarot-rpc/blob/d41e91ac8304fe2bd26c2c740d942e7e9477791d/indexer/src/types/receipt.ts#L56
                let gas_used = event.data.last().copied().unwrap_or(Felt::ZERO);
                cumulative_gas_used += gas_used;
            }

            // Convert transactions
            match convert_transaction(call_data) {
                Ok(transaction) => transactions.push(transaction),
                Err(e) => {
                    log::error!("Failed to parse transaction: {e:?}");
                }
            }

            // TODO Build receipt trie
            // TODO Build transaction trie
        }
        log::debug!("Found {} transactions", transactions.len());

        // TODO This cumulative_gas_used felt is wrong
        let gas_used = felt_to_u64(cumulative_gas_used).map_err(Error::GasUsedOverflow)?;

        let header = Header {
            // TODO
            parent_hash: B256::ZERO,
            // TODO
            ommers_hash: B256::ZERO,
            beneficiary: self.target_account_address,
            // TODO
            state_root: B256::ZERO,
            // TODO
            transactions_root: B256::ZERO,
            // TODO
            receipts_root: B256::ZERO,
            // TODO
            withdrawals_root: None,
            // TODO
            logs_bloom,
            // TODO
            difficulty: U256::ZERO,
            // TODO
            number: 0,
            // TODO
            gas_limit: 0,
            gas_used,
            // TODO: Should this be the timestamp of the source block?
            timestamp: block.timestamp,
            // TODO
            mix_hash: B256::ZERO,
            // TODO
            nonce: B64::ZERO,
            // TODO
            base_fee_per_gas: None,
            // TODO
            blob_gas_used: None,
            // TODO
            excess_blob_gas: None,
            // TODO
            parent_beacon_block_root: None,
            // TODO
            requests_root: None,
            // TODO
            extra_data: Bytes::new(),
        };

        let body = BlockBody {
            transactions,
            // TODO
            ommers: vec![],
            // TODO
            withdrawals: None,
            // TODO
            requests: None,
        };

        Ok(Block { header, body })
    }
}

// =====================================================================================
// Basic types conversion
// =====================================================================================
fn felt_to_u64(felt: Felt) -> Result<u64, Felt> {
    u64::try_from(felt.to_biguint()).map_err(|_| felt)
}
