//! CCD Vault Demo — Chain Code Delegation on Bitcoin Testnet3
//!
//! A runnable demonstration of the full CCD vault lifecycle:
//!   - Key derivation from BIP-39 mnemonics (via nostring-core)
//!   - MuSig2 Taproot vault creation
//!   - UTXO discovery via Electrum
//!   - PSBT construction, MuSig2 signing, broadcast
//!
//! Usage:
//!   cargo run -p nostring-ccd --example ccd_demo -- <command>
//!
//! Commands:
//!   info     Print vault address, keys, and derivation info
//!   balance  Check vault balance via Electrum
//!   fund     Send testnet coins from P2WPKH wallet to vault
//!   spend    MuSig2 sign and broadcast a self-spend
//!
//! ⚠️  Uses hardcoded test mnemonics. NOT FOR PRODUCTION USE.

use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
use bitcoin::{Amount, Network};

// ─── Test Mnemonics (NOT FOR PRODUCTION) ───
const OWNER_MNEMONIC: &str =
    "wrap bubble bunker win flat south life shed twelve payment super taste";
const COSIGNER_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn derive_owner_keys() -> (SecretKey, PublicKey) {
    let secp = Secp256k1::new();
    let mnemonic = nostring_core::seed::parse_mnemonic(OWNER_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");
    let master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
    let path: bitcoin::bip32::DerivationPath = "m/0/0".parse().unwrap();
    let derived = master.derive_priv(&secp, &path).unwrap();
    (derived.private_key, derived.private_key.public_key(&secp))
}

fn derive_cosigner_keys() -> (SecretKey, PublicKey) {
    let secp = Secp256k1::new();
    let mnemonic = nostring_core::seed::parse_mnemonic(COSIGNER_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");
    let master =
        nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
    let path: bitcoin::bip32::DerivationPath = "m/0/0".parse().unwrap();
    let derived = master.derive_priv(&secp, &path).unwrap();
    (derived.private_key, derived.private_key.public_key(&secp))
}

fn derive_chain_code() -> nostring_ccd::types::ChainCode {
    let mnemonic = nostring_core::seed::parse_mnemonic(OWNER_MNEMONIC).unwrap();
    let seed = nostring_core::seed::derive_seed(&mnemonic, "");
    nostring_ccd::derive_chain_code_from_seed(&seed)
}

fn create_vault() -> (
    nostring_ccd::types::CcdVault,
    musig2::KeyAggContext,
    SecretKey,
    SecretKey,
    nostring_ccd::types::DelegatedKey,
) {
    let (owner_sk, owner_pk) = derive_owner_keys();
    let (cosigner_sk, cosigner_pk) = derive_cosigner_keys();
    let chain_code = derive_chain_code();
    let delegated =
        nostring_ccd::register_cosigner_with_chain_code(cosigner_pk, chain_code, "demo-cosigner");
    let (vault, ctx) =
        nostring_ccd::vault::create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet)
            .unwrap();
    (vault, ctx, owner_sk, cosigner_sk, delegated)
}

fn cmd_info() {
    let (_owner_sk, owner_pk) = derive_owner_keys();
    let (_cosigner_sk, cosigner_pk) = derive_cosigner_keys();
    let (vault, _, _, _, _) = create_vault();

    let secp = Secp256k1::new();
    use bitcoin::key::TapTweak;
    let (output_key, _) = vault.aggregate_xonly.tap_tweak(&secp, None);

    println!("\n╔══════════════════════════════════════════╗");
    println!("║     CCD MuSig2 Taproot Vault Info        ║");
    println!("╚══════════════════════════════════════════╝\n");
    println!("Network:          testnet3");
    println!("Vault address:    {}", vault.address);
    println!("Internal key (P): {}", vault.aggregate_xonly);
    println!("Output key   (Q): {}", output_key.to_x_only_public_key());
    println!();
    println!("Owner pubkey:        {}", owner_pk);
    println!("Co-signer pubkey:    {}", cosigner_pk);
    println!("Derived co-signer:   {}", vault.cosigner_derived_pubkey);
    println!();
    println!("Derivation: mnemonic → BIP-84 testnet → m/0/0");
    println!("Chain code: HMAC-SHA512(seed, \"nostring-ccd-chain-code\")");
    println!("Vault type: MuSig2 (BIP-327) key-path-only P2TR");
    println!();

    // Show multiple indices
    println!("Vault addresses by index:");
    for i in 0..5 {
        let chain_code = derive_chain_code();
        let delegated =
            nostring_ccd::register_cosigner_with_chain_code(cosigner_pk, chain_code, "demo");
        let (v, _) =
            nostring_ccd::vault::create_vault_musig2(&owner_pk, &delegated, i, Network::Testnet)
                .unwrap();
        println!("  [{}] {}", i, v.address);
    }
}

fn cmd_balance() {
    let (vault, _, _, _, _) = create_vault();
    println!("\nVault: {}", vault.address);
    println!("Connecting to testnet Electrum...");

    let client = nostring_electrum::ElectrumClient::new(
        nostring_electrum::default_server(Network::Testnet),
        Network::Testnet,
    )
    .expect("Failed to connect");

    let height = client.get_height().unwrap();
    println!("Testnet height: {}\n", height);

    let utxos = client.get_utxos(&vault.address).unwrap();
    if utxos.is_empty() {
        println!("No UTXOs. Fund the vault:");
        println!("  Address: {}", vault.address);
        return;
    }

    let mut total = Amount::ZERO;
    for u in &utxos {
        let conf = if u.height > 0 {
            format!("confirmed h={}", u.height)
        } else {
            "unconfirmed".to_string()
        };
        println!(
            "  {}:{} — {} sat ({})",
            &u.outpoint.txid.to_string()[..12],
            u.outpoint.vout,
            u.value.to_sat(),
            conf
        );
        total += u.value;
    }
    println!("\nTotal: {} sat ({} UTXOs)", total.to_sat(), utxos.len());
}

fn cmd_fund() {
    use bitcoin::sighash::{EcdsaSighashType, SighashCache};
    use bitcoin::transaction::{Transaction, TxIn, Version};
    use bitcoin::{ScriptBuf, TxOut, Witness};

    let secp = Secp256k1::new();
    let (vault, _, _, _, _) = create_vault();

    // Source: owner's P2WPKH wallet
    let (source_sk, source_pk) = derive_owner_keys();
    let source_compressed = bitcoin::CompressedPublicKey(source_pk);
    let source_addr = bitcoin::Address::p2wpkh(&source_compressed, Network::Testnet);

    println!("\nFunding vault from P2WPKH wallet");
    println!("  Source: {}", source_addr);
    println!("  Dest:   {}", vault.address);

    let client = nostring_electrum::ElectrumClient::new(
        nostring_electrum::default_server(Network::Testnet),
        Network::Testnet,
    )
    .expect("Failed to connect");

    let utxos = client.get_utxos(&source_addr).unwrap();
    if utxos.is_empty() {
        println!("\n  ⚠️  No UTXOs at source address. Need testnet coins.");
        return;
    }

    let mut total = Amount::ZERO;
    for u in &utxos {
        total += u.value;
    }
    println!("  Source balance: {} sat", total.to_sat());

    let fee = Amount::from_sat(300);
    if total <= fee {
        println!("  ⚠️  Balance too low for fee");
        return;
    }
    let send_amount = total - fee;

    let inputs: Vec<TxIn> = utxos
        .iter()
        .map(|u| TxIn {
            previous_output: u.outpoint,
            ..Default::default()
        })
        .collect();

    let outputs = vec![TxOut {
        value: send_amount,
        script_pubkey: vault.address.script_pubkey(),
    }];

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: inputs,
        output: outputs,
    };

    // Sign P2WPKH inputs
    for (idx, utxo) in utxos.iter().enumerate() {
        let mut sighash_cache = SighashCache::new(&tx);
        let sighash = sighash_cache
            .p2wpkh_signature_hash(
                idx,
                &ScriptBuf::new_p2wpkh(&source_compressed.wpubkey_hash()),
                utxo.value,
                EcdsaSighashType::All,
            )
            .unwrap();

        let msg =
            bitcoin::secp256k1::Message::from_digest(bitcoin::hashes::Hash::to_byte_array(sighash));
        let sig = secp.sign_ecdsa(&msg, &source_sk);

        let mut sig_bytes = sig.serialize_der().to_vec();
        sig_bytes.push(EcdsaSighashType::All as u8);

        tx.input[idx].witness = Witness::from_slice(&[sig_bytes, source_pk.serialize().to_vec()]);
    }

    match client.broadcast(&tx) {
        Ok(txid) => {
            println!("\n  ✅ Vault funded!");
            println!("  Txid: {}", txid);
            println!("  Sent: {} sat", send_amount.to_sat());
            println!("  View: https://mempool.space/testnet/tx/{}", txid);
        }
        Err(e) => {
            println!("\n  ❌ Broadcast failed: {}", e);
        }
    }
}

fn cmd_spend() {
    let (vault, key_agg_ctx, owner_sk, cosigner_sk, _delegated) = create_vault();

    println!("\nSpending from CCD vault via MuSig2");
    println!("  Vault: {}", vault.address);

    let client = nostring_electrum::ElectrumClient::new(
        nostring_electrum::default_server(Network::Testnet),
        Network::Testnet,
    )
    .expect("Failed to connect");

    let utxos = client.get_utxos(&vault.address).unwrap();
    if utxos.is_empty() {
        println!("  ⚠️  No UTXOs. Fund the vault first: cargo run -p nostring-ccd --example ccd_demo -- fund");
        return;
    }

    let mut total = Amount::ZERO;
    let utxo_pairs: Vec<_> = utxos
        .iter()
        .map(|u| {
            total += u.value;
            (
                u.outpoint,
                bitcoin::TxOut {
                    value: u.value,
                    script_pubkey: vault.address.script_pubkey(),
                },
            )
        })
        .collect();

    let fee = Amount::from_sat(300);
    if total <= fee {
        println!("  ⚠️  Balance {} sat too low for fee", total.to_sat());
        return;
    }

    println!("  Balance: {} sat ({} UTXOs)", total.to_sat(), utxos.len());

    // Build PSBT (self-spend)
    let (psbt, tweaks) = nostring_ccd::vault::build_spend_psbt(
        &vault,
        &utxo_pairs,
        &[(vault.address.clone(), total - fee)],
        fee,
        None,
    )
    .unwrap();

    println!(
        "  PSBT: {} inputs, {} outputs, {} sat fee",
        psbt.inputs.len(),
        psbt.unsigned_tx.output.len(),
        fee.to_sat()
    );

    // CCD: derive co-signer child key
    let cosigner_child_sk = nostring_ccd::apply_tweak(&cosigner_sk, &tweaks[0].tweak).unwrap();

    // MuSig2 sign
    println!("  MuSig2 ceremony...");
    let signed_tx =
        nostring_ccd::vault::musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
            .unwrap();

    let tx_bytes = bitcoin::consensus::serialize(&signed_tx);
    println!(
        "  Signed: {} bytes, {} inputs",
        tx_bytes.len(),
        signed_tx.input.len()
    );

    for (i, input) in signed_tx.input.iter().enumerate() {
        let wit: Vec<&[u8]> = input.witness.iter().collect();
        println!(
            "  Input {}: {} witness element(s), {} bytes",
            i,
            wit.len(),
            wit[0].len()
        );
    }

    // Broadcast
    match client.broadcast(&signed_tx) {
        Ok(txid) => {
            println!("\n  ✅ BROADCAST SUCCESSFUL");
            println!("  Txid: {}", txid);
            println!("  View: https://mempool.space/testnet/tx/{}", txid);
        }
        Err(e) => {
            println!("\n  ❌ Broadcast failed: {}", e);
            println!("  Raw tx: {}", hex::encode(&tx_bytes));
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("info");

    match cmd {
        "info" => cmd_info(),
        "balance" => cmd_balance(),
        "fund" => cmd_fund(),
        "spend" => cmd_spend(),
        _ => {
            eprintln!("Usage: ccd_demo <info|balance|fund|spend>");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  info     Print vault address and key info");
            eprintln!("  balance  Check vault balance via Electrum");
            eprintln!("  fund     Send testnet coins to vault from P2WPKH wallet");
            eprintln!("  spend    MuSig2 sign and broadcast a self-spend");
            std::process::exit(1);
        }
    }
}
