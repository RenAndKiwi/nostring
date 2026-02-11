//! Fund a CCD vault from the existing testnet wallet.
//! This is a one-time operation to move funds into the vault for testing.

#[cfg(test)]
mod tests {
    use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
    use bitcoin::{Amount, Network};

    use crate::vault::create_vault_musig2;
    use crate::types::ChainCode;
    use crate::register_cosigner_with_chain_code;

    fn deterministic_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = seed_byte;
        bytes[0] = 0x01;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    /// Send testnet coins from the existing P2WPKH wallet to the CCD vault.
    #[test]
    #[ignore = "sends real testnet transaction"]
    fn test_fund_vault_from_existing_wallet() {
        use bitcoin::bip32::DerivationPath;
        use bitcoin::hashes::Hash;
        use bitcoin::sighash::{EcdsaSighashType, SighashCache};
        use bitcoin::transaction::{Transaction, TxIn, Version};
        use bitcoin::{ScriptBuf, TxOut, Witness};
        use nostring_electrum::{default_server, ElectrumClient};

        let secp = Secp256k1::new();

        // ─── Source wallet (existing testnet funds) ───
        let mnemonic = "wrap bubble bunker win flat south life shed twelve payment super taste";
        let mnemonic = nostring_core::seed::parse_mnemonic(mnemonic).unwrap();
        let seed = nostring_core::seed::derive_seed(&mnemonic, "");

        // BIP-84 testnet: m/84'/1'/0'
        let master = nostring_core::keys::derive_bitcoin_master_for_network(&seed, Network::Testnet).unwrap();
        // Derive the signing key at m/0/0 (first receive, non-hardened from account xpriv)
        let path: DerivationPath = "m/0/0".parse().unwrap();
        let derived = master.derive_priv(&secp, &path).unwrap();
        let source_sk = derived.private_key;
        let source_pk = source_sk.public_key(&secp);
        let source_compressed = bitcoin::CompressedPublicKey(source_pk);
        let source_addr = bitcoin::Address::p2wpkh(&source_compressed, Network::Testnet);

        println!("Source address: {}", source_addr);
        assert_eq!(
            source_addr.to_string(),
            "tb1qgmex2e43kf5zxy5408chn9qmuupqp24h3mu97v",
            "Source address mismatch — wrong derivation"
        );

        // ─── Destination: CCD vault ───
        let (_owner_sk, owner_pk) = deterministic_keypair(1);
        let (_cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner_with_chain_code(cosigner_pk, ChainCode::from_bytes([0xCC; 32]), "testnet-ccd");
        let (vault, _ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Testnet).unwrap();

        println!("Vault address: {}", vault.address);

        // ─── Connect and find UTXOs ───
        let client = ElectrumClient::new(default_server(Network::Testnet), Network::Testnet)
            .expect("Failed to connect");

        let utxos = client
            .get_utxos(&source_addr)
            .expect("Failed to get UTXOs");

        if utxos.is_empty() {
            panic!("No UTXOs at source address");
        }

        println!("Source UTXOs:");
        let mut total = Amount::ZERO;
        for u in &utxos {
            println!("  {}:{} = {} sat", u.outpoint.txid, u.outpoint.vout, u.value.to_sat());
            total += u.value;
        }
        println!("Total: {} sat", total.to_sat());

        // Send everything minus fee to vault (no change)
        let fee = Amount::from_sat(300);
        assert!(total > fee, "Insufficient funds: {} sat", total.to_sat());
        let send_amount = total - fee;

        // Build transaction
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

        // Sign each input (P2WPKH)
        let prevouts: Vec<TxOut> = utxos
            .iter()
            .map(|u| TxOut {
                value: u.value,
                script_pubkey: source_addr.script_pubkey(),
            })
            .collect();

        for idx in 0..tx.input.len() {
            let mut sighash_cache = SighashCache::new(&tx);

            let sighash = sighash_cache
                .p2wpkh_signature_hash(
                    idx,
                    &ScriptBuf::new_p2wpkh(
                        &source_compressed.wpubkey_hash(),
                    ),
                    prevouts[idx].value,
                    EcdsaSighashType::All,
                )
                .expect("Failed to compute sighash");

            let msg = bitcoin::secp256k1::Message::from_digest(sighash.to_byte_array());
            let sig = secp.sign_ecdsa(&msg, &source_sk);

            // P2WPKH witness: [signature + sighash_type, pubkey]
            let mut sig_bytes = sig.serialize_der().to_vec();
            sig_bytes.push(EcdsaSighashType::All as u8);

            tx.input[idx].witness = Witness::from_slice(&[
                sig_bytes,
                source_pk.serialize().to_vec(),
            ]);
        }

        let tx_bytes = bitcoin::consensus::serialize(&tx);
        println!("Funding tx: {} bytes", tx_bytes.len());
        println!("Txid: {}", tx.compute_txid());

        // Broadcast
        match client.broadcast(&tx) {
            Ok(txid) => {
                println!("✅ Vault funded!");
                println!("Txid: {}", txid);
                println!("View: https://mempool.space/testnet/tx/{}", txid);
                println!("Sent {} sat to vault {}", send_amount.to_sat(), vault.address);
                println!("\nWait for confirmation, then run:");
                println!("  cargo test -p nostring-ccd test_testnet_vault_spend -- --ignored --nocapture");
            }
            Err(e) => {
                println!("❌ Broadcast failed: {}", e);
                println!("Raw tx: {}", hex::encode(&tx_bytes));
                panic!("Broadcast failed: {}", e);
            }
        }
    }
}
