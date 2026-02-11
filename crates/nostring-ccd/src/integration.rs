//! Integration tests for CCD vaults on testnet.
//!
//! These tests require network access and are gated behind `#[ignore]`.
//! Run with: `cargo test -p nostring-ccd -- --ignored --nocapture`

#[cfg(test)]
mod tests {
    use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
    use bitcoin::Network;

    use crate::types::ChainCode;
    use crate::vault::{build_spend_psbt, create_vault_musig2, musig2_sign_psbt};
    use crate::{apply_tweak, register_cosigner_with_chain_code};

    fn deterministic_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = seed_byte;
        bytes[0] = 0x01;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    /// Print the testnet vault address. Fund this, then run the spend test.
    #[test]
    fn test_print_testnet_vault_address() {
        let (_owner_sk, owner_pk) = deterministic_keypair(1);
        let (_cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, ChainCode::from_bytes([0xCC; 32]), "testnet-ccd");

        let (vault, _ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        println!("=== CCD MuSig2 Vault on Testnet3 ===");
        println!("Address: {}", vault.address);
        println!("Internal key (x-only): {}", vault.aggregate_xonly);
        println!("Fund this address, then run:");
        println!("  cargo test -p nostring-ccd test_testnet_vault_spend -- --ignored --nocapture");
    }

    /// Step 1: Fund the vault by sending from the existing testnet wallet.
    ///
    /// The existing testnet wallet (tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v)
    /// has funds. But that's a P2WPKH address from nostring-core, and we need to
    /// send TO our CCD vault address.
    ///
    /// This test checks the vault address balance.
    #[test]
    #[ignore = "requires network access"]
    fn test_check_vault_balance() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (_owner_sk, owner_pk) = deterministic_keypair(1);
        let (_cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, ChainCode::from_bytes([0xCC; 32]), "testnet-ccd");

        let (vault, _ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        println!("Vault address: {}", vault.address);

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect to testnet Electrum");

        let height = client.get_height().expect("Failed to get height");
        println!("Testnet height: {}", height);

        let utxos = client
            .get_utxos(&vault.address)
            .expect("Failed to get UTXOs");

        if utxos.is_empty() {
            println!("No UTXOs at vault address yet.");
            println!("Send testnet coins to: {}", vault.address);

            // Also check existing wallet balance
            let existing_addr: bitcoin::Address<bitcoin::address::NetworkUnchecked> =
                "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v".parse().unwrap();
            let existing_addr = existing_addr.assume_checked();
            let existing_utxos = client.get_utxos(&existing_addr).unwrap();
            let total: bitcoin::Amount = existing_utxos.iter().map(|u| u.value).sum();
            println!("Existing wallet has {} sat across {} UTXOs", total.to_sat(), existing_utxos.len());
        } else {
            println!("Found {} UTXOs at vault:", utxos.len());
            let mut total = bitcoin::Amount::ZERO;
            for u in &utxos {
                println!("  {}:{} = {} sat", u.outpoint.txid, u.outpoint.vout, u.value.to_sat());
                total += u.value;
            }
            println!("Total vault balance: {} sat", total.to_sat());
        }
    }

    /// Spend from a funded CCD MuSig2 vault on testnet3.
    ///
    /// Full flow: find UTXOs → build PSBT → CCD tweak → MuSig2 sign → broadcast
    #[test]
    #[ignore = "requires funded vault + network access"]
    fn test_testnet_vault_spend() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (owner_sk, owner_pk) = deterministic_keypair(1);
        let (cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, ChainCode::from_bytes([0xCC; 32]), "testnet-ccd");

        let (vault, key_agg_ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        println!("Vault address: {}", vault.address);

        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect to testnet Electrum");

        let height = client.get_height().expect("Failed to get height");
        println!("Testnet height: {}", height);

        // Find UTXOs at vault address
        let utxos = client
            .get_utxos(&vault.address)
            .expect("Failed to get UTXOs");

        if utxos.is_empty() {
            panic!(
                "No UTXOs at vault address {}. Send testnet coins there first.",
                vault.address
            );
        }

        println!("Found {} UTXOs:", utxos.len());
        let mut total = bitcoin::Amount::ZERO;
        let utxo_pairs: Vec<(bitcoin::OutPoint, bitcoin::TxOut)> = utxos
            .iter()
            .map(|u| {
                println!("  {}:{} = {} sat", u.outpoint.txid, u.outpoint.vout, u.value.to_sat());
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

        println!("Total: {} sat", total.to_sat());

        // Send back to self minus fee (self-spend proves the signature works)
        let fee = bitcoin::Amount::from_sat(300);
        if total <= fee {
            panic!("Vault balance {} sat is too low to cover {} sat fee", total.to_sat(), fee.to_sat());
        }
        let send_amount = total - fee;

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxo_pairs,
            &[(vault.address.clone(), send_amount)],
            fee,
            None,
        )
        .expect("Failed to build PSBT");

        println!("PSBT built: {} inputs, {} outputs", psbt.inputs.len(), psbt.unsigned_tx.output.len());

        // CCD: derive co-signer child key
        let cosigner_child_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak)
            .expect("Failed to apply CCD tweak");

        // MuSig2 sign
        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
            .expect("MuSig2 signing failed");

        let tx_bytes = bitcoin::consensus::serialize(&signed_tx);
        println!("Signed tx: {} bytes", tx_bytes.len());
        println!("Txid: {}", signed_tx.compute_txid());

        // Broadcast
        match client.broadcast(&signed_tx) {
            Ok(txid) => {
                println!("✅ BROADCAST SUCCESSFUL!");
                println!("Txid: {}", txid);
                println!("View: https://mempool.space/testnet/tx/{}", txid);
                println!("\nCCD + MuSig2 vault spend PROVEN on testnet3.");
            }
            Err(e) => {
                // Print the raw tx hex for debugging
                println!("❌ Broadcast failed: {}", e);
                println!("Raw tx hex: {}", hex::encode(&tx_bytes));
                panic!("Broadcast failed: {}", e);
            }
        }
    }
}
