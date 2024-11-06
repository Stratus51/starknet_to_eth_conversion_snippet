use crate::starknet_call_data::CallData;
use alloy_consensus::transaction::{TxEip1559, TxEip2930, TxLegacy, TxType};
use alloy_rlp::Decodable;
use reth_primitives::{Transaction, TransactionSigned};
use starknet::core::types::Felt;

const TX_TYPE_EIP2930: u8 = TxType::Eip2930 as u8;
const TX_TYPE_EIP1559: u8 = TxType::Eip1559 as u8;

// XXX Should not be ignored in production code
#[allow(dead_code)]
#[derive(Debug)]
pub enum Error {
    Eip2930(alloy_rlp::Error),
    Eip1559(alloy_rlp::Error),
    Legacy(alloy_rlp::Error),
    UnsupportedTransactionType { tx_type: u8 },
    UnsupportedMultipleTransaction,
}

pub fn convert_transaction(call_data: CallData) -> Result<TransactionSigned, Error> {
    // Multi-calls are not supported for now.
    if call_data.call_array_len != Felt::ONE {
        return Err(Error::UnsupportedMultipleTransaction);
    }

    // Parse transaction
    let transaction = parse_transaction(&call_data.data)?;

    // XXX Debug
    log::debug!("transaction: {transaction:#?}");

    // Return
    Ok(TransactionSigned::from_transaction_and_signature(
        transaction,
        call_data.signature,
    ))
}

fn parse_transaction(data: &[u8]) -> Result<Transaction, Error> {
    let tx_type = data[1];
    let mut data_ptr = &data[2..];

    // Select parser based on tx_type
    if tx_type < 0x7f {
        match tx_type {
            TX_TYPE_EIP2930 => TxEip2930::decode(&mut data_ptr)
                .map(|tx| tx.into())
                .map_err(Error::Eip2930),
            TX_TYPE_EIP1559 => {
                // XXX
                log::debug!("Decoding TX_TYPE_EIP1559: {}", hex::encode(data_ptr));
                TxEip1559::decode(&mut data_ptr)
                    .map(|tx| tx.into())
                    .map_err(Error::Eip1559)
            }
            _ => Err(Error::UnsupportedTransactionType { tx_type }),
        }
    } else {
        TxLegacy::decode(&mut data_ptr)
            .map(|tx| tx.into())
            .map_err(Error::Legacy)
    }
}
