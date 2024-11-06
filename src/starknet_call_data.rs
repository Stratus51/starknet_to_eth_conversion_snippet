use alloy_primitives::{Parity, Uint};
use reth_primitives::Signature;
use starknet::core::types::{Felt, InvokeTransaction};

pub struct CallData {
    // call_len: Felt,
    // to: Felt,
    // selector: Felt,
    // call_data_len: Felt,
    // outside_execution: CallDataOutsideExecution,

    // From https://github.com/kkrt-labs/kakarot-rpc/blob/15da170828f3281721a4c2995a47d64636d5607a/indexer/src/types/transaction.ts#L289
    // [call_array_len, to, selector, data_offset, data_len, calldata_len, calldata, signature_len, signature]
    pub call_array_len: Felt,
    pub to: Felt,
    // selector2: Felt,
    // data_offset: Felt,
    // data_len: Felt,
    // pub calldata_len: Felt,
    pub data: Vec<u8>,
    // signature_len: Felt
    pub signature: Signature,
}

// struct CallDataOutsideExecution {
//     caller: Felt,
//     nonce: Felt,
//     after: Felt,
//     before: Felt,
// }

// XXX Should not be ignored in production code
#[allow(dead_code)]
#[derive(Debug)]
pub enum Error {
    CallDataTooSmall,
    DataLenOverflow(Felt),
    DataLenBiggerThanCallData,
    CallDataLenOverflow(Felt),
    SignatureLenOverflow(Felt),
    BadSignature(SignatureError),
}

impl CallData {
    pub fn parse_from_transaction(
        transaction: &InvokeTransaction,
    ) -> Result<Self, (Error, Vec<Felt>)> {
        let raw_call_data = match transaction {
            InvokeTransaction::V0(transaction) => &transaction.calldata,
            InvokeTransaction::V1(transaction) => &transaction.calldata,
            InvokeTransaction::V3(transaction) => &transaction.calldata,
        };
        Self::parse(raw_call_data).map_err(|e| (e, raw_call_data.to_vec()))
    }

    pub fn parse(raw: &[Felt]) -> Result<Self, Error> {
        // Quick pre-filter
        if raw.len() <= 17 {
            return Err(Error::CallDataTooSmall);
        }

        // Parse data len
        let data_len = felt_to_usize(raw[12]).map_err(Error::DataLenOverflow)?;

        // Parse data
        let calldata_len = felt_to_usize(raw[14]).map_err(Error::CallDataLenOverflow)?;
        let relevant_data = raw
            .get(15..raw.len() - 6)
            .ok_or(Error::DataLenBiggerThanCallData)?;
        let data = parse_data(relevant_data, calldata_len)?;

        // Parse signature
        let sig_offset = 12 + 1 + data_len + 1;
        let signature_len = felt_to_usize(raw[sig_offset]).map_err(Error::SignatureLenOverflow)?;
        let relevant_data = raw
            .get(sig_offset + 1..sig_offset + 1 + signature_len)
            .ok_or(Error::BadSignature(SignatureError::IncompleteSignatureData))?;
        let signature = parse_signature(relevant_data).map_err(Error::BadSignature)?;

        Ok(Self {
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
            to: raw[9],
            // selector2: raw[10],
            // data_offset: raw[11],
            // data_len,
            // calldata_len: raw[14],
            data,
            signature,
        })
    }
}

// =====================================================================================
// Data parsing
// =====================================================================================
fn parse_data(raw: &[Felt], len: usize) -> Result<Vec<u8>, Error> {
    // Calculate required number of felts
    let mut nb_felts = len / 32;
    let mut remaining_bytes = len % 32;
    if remaining_bytes == 0 {
        remaining_bytes = 32;
        nb_felts += 1;
    }

    // Check that call_data is big enough
    let data = raw
        .get(..nb_felts + 1)
        .ok_or(Error::DataLenBiggerThanCallData)?;
    let last_felt = data.last().ok_or(Error::DataLenBiggerThanCallData)?;

    // Concatenate first full felts together
    let mut ret: Vec<u8> = data[..data.len() - 1]
        .iter()
        .map(|felt| felt.to_bytes_be())
        .collect::<Vec<_>>()
        .concat();

    // Add remaining bytes from last felt
    let offset = 32 - remaining_bytes;
    ret.extend(&last_felt.to_bytes_be()[offset..]);

    // Return
    Ok(ret)
}

// =====================================================================================
// Signature parsing
// =====================================================================================
// XXX Should not be ignored in production code
#[allow(dead_code)]
#[derive(Debug)]
pub enum SignatureError {
    IncompleteSignatureData,
    UnsupportedLen(usize),
    BadR(U128FeltError),
    BadV(U128FeltError),
}

fn parse_signature(raw: &[Felt]) -> Result<Signature, SignatureError> {
    // We only support one rigid type of signature format
    if raw.len() != 5 {
        return Err(SignatureError::UnsupportedLen(raw.len()));
    }

    // Felts semantics
    let r_low = raw[0];
    let r_high = raw[1];
    let s_low = raw[2];
    let s_high = raw[3];
    let v = raw[4];

    // TODO Check that taking the lower bytes of _low and _high is the right conversion to do
    let r = concat_u128_felt_into_uint256(r_low, r_high).map_err(SignatureError::BadR)?;
    let s = concat_u128_felt_into_uint256(s_low, s_high).map_err(SignatureError::BadV)?;

    // TODO Check if u64 parity for Eip155 should be different
    let v = Parity::Parity(v != Felt::ZERO);

    Ok(Signature::new(r, s, v))
}

// =====================================================================================
// Basic types conversion
// =====================================================================================
fn felt_to_usize(felt: Felt) -> Result<usize, Felt> {
    usize::try_from(felt.to_biguint()).map_err(|_| felt)
}

// XXX Should not be ignored in production code
#[allow(dead_code)]
#[derive(Debug)]
pub enum U128FeltError {
    LowOverflow(Felt),
    HighOverflow(Felt),
}

fn concat_u128_felt_into_uint256(low: Felt, high: Felt) -> Result<Uint<256, 4>, U128FeltError> {
    let low_bytes = low.to_bytes_be();
    let high_bytes = high.to_bytes_be();

    // Check for overflows
    if low_bytes[0..16] != [0; 16] {
        return Err(U128FeltError::LowOverflow(low));
    }
    if high_bytes[0..16] != [0; 16] {
        return Err(U128FeltError::HighOverflow(low));
    }

    // This unwrap can never panic
    let data: [u8; 32] = [&high_bytes[16..], &low_bytes[16..]]
        .concat()
        .try_into()
        .unwrap();
    Ok(Uint::<256, 4>::from_be_bytes(data))
}
