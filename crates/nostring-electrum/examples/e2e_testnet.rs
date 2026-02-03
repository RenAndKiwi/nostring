//! End-to-end testnet validation
//!
//! Run with: cargo run -p nostring-electrum --example e2e_testnet

use bitcoin::Network;
use nostring_electrum::ElectrumClient;

fn main() {
    println!("\n=== NoString E2E Testnet Test ===\n");

    // Our test wallet address
    let address = "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v";
    println!("Test wallet: {}", address);

    // Connect to testnet Electrum
    println!("\nConnecting to testnet Electrum...");
    let client = match ElectrumClient::new("ssl://blockstream.info:993", Network::Testnet) {
        Ok(c) => {
            println!("✓ Connected to testnet Electrum");
            c
        }
        Err(e) => {
            println!("✗ Connection failed: {}", e);
            return;
        }
    };

    // Check current height
    match client.get_height() {
        Ok(height) => println!("✓ Current testnet height: {}", height),
        Err(e) => println!("✗ Failed to get height: {}", e),
    }

    // Check our UTXOs
    let addr: bitcoin::Address<bitcoin::address::NetworkUnchecked> = address.parse().unwrap();
    let addr = addr.assume_checked();

    match client.get_utxos(&addr) {
        Ok(utxos) => {
            println!("✓ Found {} UTXO(s)", utxos.len());
            let mut total = 0u64;
            for utxo in &utxos {
                println!(
                    "  - {}:{} = {} sats (height {})",
                    utxo.outpoint.txid,
                    utxo.outpoint.vout,
                    utxo.value.to_sat(),
                    utxo.height
                );
                total += utxo.value.to_sat();
            }
            println!(
                "\n✓ Total balance: {} sats ({:.8} tBTC)",
                total,
                total as f64 / 100_000_000.0
            );
        }
        Err(e) => println!("✗ Failed to get UTXOs: {}", e),
    }

    println!("\n=== E2E Test Complete ===\n");
}
