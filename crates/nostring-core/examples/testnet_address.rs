//! Generate a testnet address for testing
//! Run with: cargo run --example testnet_address

use bitcoin::bip32::{DerivationPath, Xpriv, Xpub};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{Address, CompressedPublicKey, Network};
use bip39::{Language, Mnemonic};

fn main() {
    // Generate a new mnemonic for testing
    let mnemonic = Mnemonic::generate_in(Language::English, 12).unwrap();
    println!("=== TESTNET WALLET ===\n");
    println!("⚠️  SAVE THIS MNEMONIC (testnet only):\n");
    println!("{}\n", mnemonic);

    // Derive seed
    let seed = mnemonic.to_seed("");

    // Derive master key for testnet (m/84'/1'/0')
    let secp = Secp256k1::new();
    let master = Xpriv::new_master(Network::Testnet, &seed).unwrap();
    let path: DerivationPath = "m/84'/1'/0'".parse().unwrap();
    let account = master.derive_priv(&secp, &path).unwrap();

    // Derive first receive address (m/84'/1'/0'/0/0)
    let recv_path: DerivationPath = "m/0/0".parse().unwrap();
    let recv_key = account.derive_priv(&secp, &recv_path).unwrap();
    let public_key = Xpub::from_priv(&secp, &recv_key);
    let compressed = CompressedPublicKey(public_key.public_key);
    let address = Address::p2wpkh(&compressed, Network::Testnet);

    println!("First receive address (m/84'/1'/0'/0/0):\n");
    println!("{}\n", address);
    println!("Use this address to request testnet coins from:");
    println!("https://coinfaucet.eu/en/btc-testnet/\n");
}
