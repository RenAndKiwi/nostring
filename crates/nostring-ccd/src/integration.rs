//! Integration tests for CCD vaults on Signet.
//!
//! These tests require network access and are gated behind `#[ignore]`.
//! Run with: `cargo test -p nostring-ccd -- --ignored`
//!
//! To actually broadcast, you need a funded Signet vault. The tests will
//! create a vault address you can fund from a Signet faucet.

#[cfg(test)]
mod tests {
    use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
    use bitcoin::Network;

    use crate::musig::musig2_key_agg_tweaked;
    use crate::vault::{build_spend_psbt, create_vault_musig2, musig2_sign_psbt};
    use crate::{apply_tweak, register_cosigner};

    fn deterministic_keypair(seed_byte: u8) -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = seed_byte;
        bytes[0] = 0x01;
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let pk = sk.public_key(&secp);
        (sk, pk)
    }

    /// Print a vault address for manual funding via Signet faucet.
    #[test]
    fn test_print_signet_vault_address() {
        let (_owner_sk, owner_pk) = deterministic_keypair(1);
        let (_cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "signet-test");

        let (vault, _ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        println!("=== CCD MuSig2 Vault on Signet ===");
        println!("Address: {}", vault.address);
        println!("Internal key (x-only): {}", vault.aggregate_xonly);
        println!("Owner pubkey: {}", owner_pk);
        println!("Co-signer pubkey: {}", cosigner_pk);
        println!("Co-signer derived pubkey: {}", vault.cosigner_derived_pubkey);
        println!("Fund this address from: https://signetfaucet.com/");
        println!("Then run: cargo test -p nostring-ccd test_signet_spend -- --ignored");
    }

    /// Attempt to spend from a funded Signet vault.
    ///
    /// Prerequisites:
    /// 1. Run `test_print_signet_vault_address` to get the address
    /// 2. Fund it from https://signetfaucet.com/
    /// 3. Wait for confirmation
    /// 4. Run this test
    #[test]
    #[ignore = "requires funded Signet vault + network access"]
    fn test_signet_spend() {
        use nostring_electrum::{default_server, ElectrumClient};

        let (owner_sk, owner_pk) = deterministic_keypair(1);
        let (cosigner_sk, cosigner_pk) = deterministic_keypair(42);
        let delegated = register_cosigner(cosigner_pk, "signet-test");

        let (vault, key_agg_ctx) =
            create_vault_musig2(&owner_pk, &delegated, 0, Network::Signet).unwrap();

        println!("Vault address: {}", vault.address);

        // Connect to Signet Electrum
        let electrum_url = default_server(Network::Signet);
        let client = ElectrumClient::new(electrum_url, Network::Signet)
            .expect("Failed to connect to Signet Electrum");

        let height = client.get_height().expect("Failed to get height");
        println!("Signet height: {}", height);

        // Find UTXOs
        let utxos = client
            .get_utxos(&vault.address)
            .expect("Failed to get UTXOs");

        if utxos.is_empty() {
            println!("No UTXOs found. Fund the address first:");
            println!("  Address: {}", vault.address);
            println!("  Faucet: https://signetfaucet.com/");
            panic!("No UTXOs — fund the vault address first");
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

        // Send back to self minus fee
        let fee = bitcoin::Amount::from_sat(300);
        let send_amount = total - fee;

        let (psbt, tweaks) = build_spend_psbt(
            &vault,
            &utxo_pairs,
            &[(vault.address.clone(), send_amount)],
            fee,
            None,
        )
        .expect("Failed to build PSBT");

        // Derive co-signer child key
        let cosigner_child_sk = apply_tweak(&cosigner_sk, &tweaks[0].tweak)
            .expect("Failed to apply tweak");

        // MuSig2 sign
        let signed_tx = musig2_sign_psbt(&owner_sk, &cosigner_child_sk, &key_agg_ctx, &psbt)
            .expect("Failed to sign");

        println!("Signed tx: {} bytes", bitcoin::consensus::serialize(&signed_tx).len());
        println!("Txid: {}", signed_tx.compute_txid());

        // Broadcast
        let txid = client
            .broadcast(&signed_tx)
            .expect("Failed to broadcast");

        println!("✅ Broadcast successful! Txid: {}", txid);
        println!("View: https://mempool.space/signet/tx/{}", txid);
    }
}
