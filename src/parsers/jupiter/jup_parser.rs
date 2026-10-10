use crate::normalization::structs::{Instruction, Transaction};
use crate::parsers::constants::jupiter::{
    AGGREGATOR_V6, LIMIT_ORDER_V2, RFQ_ORDER_ENGINE, RFQ_ORDER_ENGINE_V2,
};

use crate::parsers::constants::token::{
    TOKEN, TOKEN_2022, NATIVE_WRAPPED_SOL, MINT_DECIMALS_OFFSET, MINT_ACCOUNT_LEN,
};

use crate::parsers::constants::system::SYSTEM_PROGRAM; //native sol



pub async fn parse(tx : &Transaction, ix : &Instruction){
    
}