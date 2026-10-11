use crate::normalization::events::Event;
use crate::normalization::structs::{Instruction, Transaction};
use crate::parsers::constants::jupiter::{
    AGGREGATOR_V6, LIMIT_ORDER_V2, RFQ_ORDER_ENGINE, RFQ_ORDER_ENGINE_V2,
};

use crate::parsers::constants::token::{
    TOKEN, TOKEN_2022, NATIVE_WRAPPED_SOL, MINT_DECIMALS_OFFSET, MINT_ACCOUNT_LEN,
};

use crate::parsers::constants::system::SYSTEM_PROGRAM;
use crate::parsers::jupiter::constants::{ ROUTE,}; //native sol



pub async fn parse(tx : &Transaction, ix : &Instruction ) -> Vec<Event> {
    let Ok(head): Result<[u8; 8], _> = ix.data[..8].try_into() else {
        return Vec::new();
    };

    match head {
        ROUTE => parse_route(tx, ix),
        _ => Vec::new(),
    }
          
}


fn parse_route(tx: &Transaction, ix: &Instruction) -> Vec<Event> {

    let Some(body) = ix.data.get (8..) else {
        return Vec::new();
    };

    let Some(len) = body.get(0..4) else {
         return Vec::new();
    };

    let steps = u32::from_le_bytes(len.try_into().expect("length checked")) as usize;

 let at = 4 + steps * 4;


 let Some(in_amount) = body.get(at..at+8) else {
     return Vec::new();
 };


 let Some(quoted_out) = body.get(at+8..at+16) else{
    return Vec::new();
 };

 let Some(slippage) = body.get(at+16..at+18) else {
    return Vec::new();
 };

 let Some(fee) = body.get(at+18) else{
    return Vec::new();
 };


 // type conversion
let in_amount = u64::from_le_bytes(in_amount.try_into().expect("length checked"));
    let quoted_out = u64::from_le_bytes(quoted_out.try_into().expect("length checked"));
    let slippage_bps = u16::from_le_bytes(slippage.try_into().expect("length checked"));
    let platform_fee_bps = *fee;


// accounts data, hard part. learning : we need to find the indexes for these. Alternative : use Vixens or other methods that i will use
let user = ix.accounts.get(2).cloned().unwrap_or_default();
    let in_mint = ix.accounts.get(7).cloned().unwrap_or_default();
    let out_mint = ix.accounts.get(8).cloned().unwrap_or_default();


   Vec::new()
}

