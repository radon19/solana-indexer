// Mint address -> display symbol.
//
// Resolved against Jupiter's verified token registry (lite-api.jup.ag), which
// carries canonical mainnet addresses. An earlier pass through DexScreener's
// search returned bridged and wrapped look-alikes — it picked `EPPiSnGo…` as
// "SOL" and `BUajtJr…` as "USDC" — so do not use pair search for this.
//
// Display only. Amounts stay raw u64; decimals are applied at query time.
// Anything not listed here falls back to a shortened address, never a guess.

pub mod mints {
    pub const SOL: &str = "So11111111111111111111111111111111111111112";
    pub const WSOL: &str = SOL;

    pub const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    pub const USDT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
    pub const USDS: &str = "USDSwr9ApdHk5bvJKMjzff41FfuX8bSxdKcR81vTwcA";
    pub const EURC: &str = "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr";
    pub const PYUSD: &str = "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo";
    pub const FDUSD: &str = "9zNQRsGLjNKwCUU5Gq5LR8beUCPzQMVMqKAi3SSZh54u";

    pub const WBTC: &str = "3NZ9JMVBmGAqocybic2c7LQCJScmgsAZ6vQqTDzcqmJh";
    pub const CBBTC: &str = "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij";

    pub const JUP: &str = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN";
    pub const RAY: &str = "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R";
    pub const ORCA: &str = "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE";

    pub const LST_SOL: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn"; // JitoSOL
    pub const MSOL: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So";
    pub const MNDE: &str = "MNDEFzGvMt87ueuHvVU9VcTqsAP5b3fTGPsHuuPA5ey";

    pub const JLP: &str = "27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4";
    pub const KMNO: &str = "KMNo3nJsBXfcpJTVhZcXLW7RmTwTt4GVFE7suUBo9sS";
    pub const DRIFT: &str = "DriFtupJYLTosbwoN8koMbEYSx54aFAVLddWsbksjwg7";

    pub const PYTH: &str = "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3";
    pub const JTO: &str = "jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL";
    pub const HNT: &str = "hntyVP6YFm1Hg25TN9WGLqM12b8TQmcknKrdu1oxWux";
    pub const RENDER: &str = "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof";

    pub const BONK: &str = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
    pub const WIF: &str = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm";
    pub const PUMP: &str = "pumpCmXqMfrsAkQ5r49WcJnRayYRqmXz6ae8H7H9Dfn";
    pub const TRUMP: &str = "6p6xgHyF7AeE6TZkSmFsko444wqoP15icUSqi2jfGiPN";
    pub const PENGU: &str = "2zMMhcVQEXDtdE6vsFS7S7D5oUodfJHE8vd1gnBouauv";
    pub const MEW: &str = "MEW1gQWJ3nEXg2qgERiKu7FAFj79PHvQVREQUzScPP5";

    // Entry point for pump.fun mints.
    pub const PUMP_FUN: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
}

/// address -> symbol, ordered by the constants above.
const KNOWN: &[(&str, &str)] = &[
    (mints::SOL, "SOL"),
    (mints::USDC, "USDC"),
    (mints::USDT, "USDT"),
    (mints::USDS, "USDS"),
    (mints::EURC, "EURC"),
    (mints::PYUSD, "PYUSD"),
    (mints::FDUSD, "FDUSD"),
    (mints::WBTC, "WBTC"),
    (mints::CBBTC, "cbBTC"),
    (mints::JUP, "JUP"),
    (mints::RAY, "RAY"),
    (mints::ORCA, "ORCA"),
    (mints::LST_SOL, "JitoSOL"),
    (mints::MSOL, "mSOL"),
    (mints::MNDE, "MNDE"),
    (mints::JLP, "JLP"),
    (mints::KMNO, "KMNO"),
    (mints::DRIFT, "DRIFT"),
    (mints::PYTH, "PYTH"),
    (mints::JTO, "JTO"),
    (mints::HNT, "HNT"),
    (mints::RENDER, "RENDER"),
    (mints::BONK, "BONK"),
    (mints::WIF, "WIF"),
    (mints::PUMP, "PUMP"),
    (mints::TRUMP, "TRUMP"),
    (mints::PENGU, "PENGU"),
    (mints::MEW, "MEW"),
];

/// Symbol for a mint, or a shortened address when unknown.
///
/// Never guesses. An unknown mint renders by address so a wrong label can
/// never be mistaken for a real one.
pub fn symbol(mint: &str) -> String {
    match KNOWN.iter().find(|(addr, _)| *addr == mint) {
        Some((_, sym)) => (*sym).to_string(),
        None => abbreviate(mint),
    }
}

/// True when the mint has a known symbol.
pub fn is_known(mint: &str) -> bool {
    KNOWN.iter().any(|(addr, _)| *addr == mint)
}

/// First six and last four characters: `EPjFWdd5Aufq..TDt1v`.
pub fn abbreviate(addr: &str) -> String {
    if addr.len() <= 12 {
        return addr.to_string();
    }
    format!("{}..{}", &addr[..6], &addr[addr.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majors_resolve() {
        assert_eq!(symbol(mints::SOL), "SOL");
        assert_eq!(symbol(mints::USDC), "USDC");
        assert_eq!(symbol(mints::USDT), "USDT");
        assert_eq!(symbol(mints::JUP), "JUP");
        assert_eq!(symbol(mints::PUMP), "PUMP");
    }

    #[test]
    fn unknown_falls_back_to_address() {
        let pump = "kXzjEQ2frgfKHAUWGS1rZFH53tyWjD6b9njoQ82pump";
        assert!(!is_known(pump));
        assert_eq!(symbol(pump), "kXzjEQ..pump");
    }

    #[test]
    fn no_duplicate_addresses() {
        let mut seen = std::collections::HashSet::new();
        for (addr, _) in KNOWN {
            assert!(seen.insert(*addr), "duplicate address {addr}");
        }
    }

    /// Base58 pubkeys decode to 32 bytes. The string length varies between
    /// 43 and 44 depending on leading zero bytes, so check the decoded form.
    #[test]
    fn every_entry_is_a_pubkey() {
        for (addr, sym) in KNOWN {
            let bytes = bs58::decode(addr)
                .into_vec()
                .unwrap_or_else(|_| panic!("{sym} is not base58"));
            assert_eq!(bytes.len(), 32, "{sym} is not 32 bytes");
        }
    }
}