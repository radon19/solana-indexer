use crate::normalization::events::{
    Event,
    LiquidityAddEvent,
    LiquidityRemoveEvent,
    PoolCreateEvent,
    Protocol,
    SwapEvent,
    TradeEvent,
    TradeSide,
    TransferEvent,
};

use crate::normalization::structs::Transaction;

pub fn print_events(tx: &Transaction, events: &[Event]) {
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TX  {}", tx.signature);
    println!("Slot:       {}", tx.slot);

    // Collect unique protocols
    let mut protocols: Vec<Protocol> = Vec::new();

    for event in events {
        let protocol = match event {
            Event::Swap(event) => event.protocol,
            Event::Trade(event) => event.protocol,
            Event::Transfer(_) => continue,
            Event::LiquidityAdd(event) => event.protocol,
            Event::LiquidityRemove(event) => event.protocol,
            Event::PoolCreate(event) => event.protocol,
        };

        if !protocols.contains(&protocol) {
            protocols.push(protocol);
        }
    }

    println!(
        "Protocols:  {}",
        protocols
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    println!();

    // Print every parsed event
    for event in events {
        match event {
            Event::Swap(event) => print_swap(event),
            Event::Trade(event) => print_trade(event),
            Event::Transfer(event) => print_transfer(event),
            Event::LiquidityAdd(event) => print_liquidity_add(event),
            Event::LiquidityRemove(event) => print_liquidity_remove(event),
            Event::PoolCreate(event) => print_pool_create(event),
        }

        println!();
    }

    println!(
        "Status:     {}",
        if tx.success {
            "SUCCESS"
        } else {
            "FAILED"
        }
    );

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
}

// --------------------------------------------------
// Swap
// --------------------------------------------------

fn print_swap(event: &SwapEvent) {
    println!("{}:", event.protocol.as_str());
    println!("  Action:   SWAP");
    println!(
        "  Input:    {} {}",
        event.input_amount,
        event.input_mint
    );
    println!(
        "  Output:   {} {}",
        event.output_amount,
        event.output_mint
    );

    if let Some(aggregator) = &event.aggregator {
        println!("  Aggregator: {}", aggregator.as_str());
    }
}

// --------------------------------------------------
// Pump.fun Trade
// --------------------------------------------------

fn print_trade(event: &TradeEvent) {
    println!("{}:", event.protocol.as_str());

    let side = match event.side {
        TradeSide::Buy => "BUY",
        TradeSide::Sell => "SELL",
    };

    println!("  Action:   {}", side);
    println!("  Token:    {}", event.mint);
    println!("  Amount:   {}", event.token_amount);
    println!("  SOL:      {}", event.sol_amount);
    println!("  User:     {}", event.user);
}

// --------------------------------------------------
// Transfer
// --------------------------------------------------

fn print_transfer(event: &TransferEvent) {
    println!("Transfer:");
    println!("  From:     {}", event.source);
    println!("  To:       {}", event.destination);
    println!("  Amount:   {} {}", event.amount, event.mint);
}

// --------------------------------------------------
// Add Liquidity
// --------------------------------------------------

fn print_liquidity_add(event: &LiquidityAddEvent) {
    println!("{}:", event.protocol.as_str());
    println!("  Action:   ADD LIQUIDITY");
    println!("  Pool:     {}", event.pool);

    println!(
        "  Token A:  {} {}",
        event.token_a_amount,
        event.token_a_mint
    );

    println!(
        "  Token B:  {} {}",
        event.token_b_amount,
        event.token_b_mint
    );
}

// --------------------------------------------------
// Remove Liquidity
// --------------------------------------------------

fn print_liquidity_remove(event: &LiquidityRemoveEvent) {
    println!("{}:", event.protocol.as_str());
    println!("  Action:   REMOVE LIQUIDITY");
    println!("  Pool:     {}", event.pool);

    println!(
        "  Token A:  {} {}",
        event.token_a_amount,
        event.token_a_mint
    );

    println!(
        "  Token B:  {} {}",
        event.token_b_amount,
        event.token_b_mint
    );
}

// --------------------------------------------------
// Pool Create
// --------------------------------------------------

fn print_pool_create(event: &PoolCreateEvent) {
    println!("{}:", event.protocol.as_str());
    println!("  Action:   POOL CREATE");
    println!("  Pool:     {}", event.pool);
    println!("  Token A:  {}", event.token_a_mint);
    println!("  Token B:  {}", event.token_b_mint);
}