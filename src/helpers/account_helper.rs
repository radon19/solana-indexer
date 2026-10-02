use yellowstone_grpc_proto::geyser::SubscribeUpdateAccount;

use crate::normalization::structs::{Ingester, Account};

pub fn get_account(acc: SubscribeUpdateAccount) -> Ingester {
    let account = acc.account.unwrap();

    let temp =Account{
        pubkey :  bs58::encode(account.pubkey).into_string(),
        owner : bs58::encode(account.owner).into_string(),
        lamports : account.lamports,
        slot : acc.slot,
        data : account.data,
        executable : account.executable,
        write_version :account.write_version

    };

    Ingester::Account(temp)
}

