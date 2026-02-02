//! NoString Electrum Client
//!
//! Provides Bitcoin network access via Electrum protocol for:
//! - UTXO discovery (finding inheritance funds)
//! - Block height monitoring (timelock tracking)
//! - Transaction broadcasting (check-in execution)
//!
//! # Security
//!
//! - Always use SSL/TLS connections (ssl:// or tcp+tls://)
//! - Validate all data received from server
//! - Never send private keys over the wire
//!
//! # Example
//!
//! ```ignore
//! use nostring_electrum::ElectrumClient;
//! use bitcoin::Network;
//!
//! let client = ElectrumClient::new("ssl://electrum.blockstream.info:60002", Network::Bitcoin)?;
//! let height = client.get_height()?;
//! println!("Current block height: {}", height);
//! ```

use bitcoin::{Address, Amount, Network, OutPoint, Script, ScriptBuf, Transaction, Txid};
use electrum_client::{ElectrumApi, Error as ElectrumError};
use thiserror::Error;

// Re-export the raw client for direct usage
pub use electrum_client::Client as RawClient;

/// Errors from Electrum operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Electrum protocol error: {0}")]
    Protocol(#[from] ElectrumError),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Transaction not found: {0}")]
    TxNotFound(Txid),

    #[error("Broadcast failed: {0}")]
    BroadcastFailed(String),

    #[error("No UTXOs found for address")]
    NoUtxos,
}

/// A UTXO (unspent transaction output) discovered via Electrum
#[derive(Debug, Clone)]
pub struct Utxo {
    /// The outpoint (txid:vout)
    pub outpoint: OutPoint,
    /// Value in satoshis
    pub value: Amount,
    /// Block height where this was confirmed (0 if unconfirmed)
    pub height: u32,
    /// The script pubkey
    pub script_pubkey: ScriptBuf,
}

/// Electrum client for Bitcoin network operations
pub struct ElectrumClient {
    client: electrum_client::Client,
    network: Network,
}

impl ElectrumClient {
    /// Create a new Electrum client
    ///
    /// # Arguments
    /// * `url` - Electrum server URL (e.g., "ssl://electrum.blockstream.info:60002")
    /// * `network` - Bitcoin network (Mainnet, Testnet, Signet, Regtest)
    ///
    /// # Security
    /// Always use SSL URLs in production. Plaintext connections can be MITM'd.
    pub fn new(url: &str, network: Network) -> Result<Self, Error> {
        // Warn if not using SSL
        if !url.starts_with("ssl://") && !url.contains("tls") {
            log::warn!("Connecting to Electrum without SSL - insecure for mainnet!");
        }

        let client = electrum_client::Client::new(url)
            .map_err(|e: ElectrumError| Error::Connection(e.to_string()))?;

        Ok(Self { client, network })
    }

    /// Get current blockchain height
    pub fn get_height(&self) -> Result<u32, Error> {
        let header = self.client.block_headers_subscribe()?;
        Ok(header.height as u32)
    }

    /// Get UTXOs for a script (typically from a descriptor address)
    ///
    /// # Arguments
    /// * `script` - The script pubkey to search for
    pub fn get_utxos_for_script(&self, script: &Script) -> Result<Vec<Utxo>, Error> {
        let unspent = self.client.script_list_unspent(script)?;

        let utxos: Vec<Utxo> = unspent
            .into_iter()
            .map(|u| Utxo {
                outpoint: OutPoint {
                    txid: u.tx_hash,
                    vout: u.tx_pos as u32,
                },
                value: Amount::from_sat(u.value),
                height: u.height as u32,
                script_pubkey: script.to_owned(),
            })
            .collect();

        Ok(utxos)
    }

    /// Get UTXOs for an address
    pub fn get_utxos(&self, address: &Address) -> Result<Vec<Utxo>, Error> {
        // Note: Address type in bitcoin 0.32 uses NetworkKind, not Network directly
        // We trust the caller to provide a valid address for the network
        self.get_utxos_for_script(address.script_pubkey().as_script())
    }

    /// Get a transaction by txid
    pub fn get_transaction(&self, txid: &Txid) -> Result<Transaction, Error> {
        self.client
            .transaction_get(txid)
            .map_err(|_| Error::TxNotFound(*txid))
    }

    /// Broadcast a signed transaction
    ///
    /// # Returns
    /// The txid of the broadcast transaction
    pub fn broadcast(&self, tx: &Transaction) -> Result<Txid, Error> {
        self.client
            .transaction_broadcast(tx)
            .map_err(|e: ElectrumError| Error::BroadcastFailed(e.to_string()))
    }

    /// Get the balance for a script
    pub fn get_balance(&self, script: &Script) -> Result<Amount, Error> {
        let balance = self.client.script_get_balance(script)?;
        // Note: unconfirmed can be negative (pending spends), so handle carefully
        let total = balance.confirmed as i64 + balance.unconfirmed;
        Ok(Amount::from_sat(total.max(0) as u64))
    }

    /// Get the network this client is configured for
    pub fn network(&self) -> Network {
        self.network
    }

    /// Check if a transaction is confirmed
    pub fn is_confirmed(&self, txid: &Txid) -> Result<bool, Error> {
        match self.client.transaction_get(txid) {
            Ok(_) => {
                // Check if it has confirmations by looking at merkle proof
                match self.client.transaction_get_merkle(txid, 0) {
                    Ok(merkle) => Ok(merkle.block_height > 0),
                    Err(_) => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }
}

/// Default Electrum servers for each network
pub fn default_server(network: Network) -> &'static str {
    match network {
        Network::Bitcoin => "ssl://electrum.blockstream.info:60002",
        Network::Testnet => "ssl://electrum.blockstream.info:60004",
        Network::Signet => "ssl://mempool.space:60602",
        Network::Regtest => "tcp://127.0.0.1:60401",
        _ => "ssl://electrum.blockstream.info:60002",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_servers() {
        assert!(default_server(Network::Bitcoin).contains("60002"));
        assert!(default_server(Network::Testnet).contains("60004"));
    }

    // Integration tests require network access
    // Run with: cargo test --package nostring-electrum -- --ignored

    #[test]
    #[ignore = "requires network access"]
    fn test_connect_mainnet() {
        let client = ElectrumClient::new(default_server(Network::Bitcoin), Network::Bitcoin);
        assert!(client.is_ok());
    }

    #[test]
    #[ignore = "requires network access"]
    fn test_get_height_mainnet() {
        let client =
            ElectrumClient::new(default_server(Network::Bitcoin), Network::Bitcoin).unwrap();
        let height = client.get_height().unwrap();
        // Block height should be > 800000 as of 2024
        assert!(height > 800000);
        println!("Current mainnet height: {}", height);
    }

    #[test]
    #[ignore = "requires network access"]
    fn test_get_height_testnet() {
        let client =
            ElectrumClient::new(default_server(Network::Testnet), Network::Testnet).unwrap();
        let height = client.get_height().unwrap();
        assert!(height > 0);
        println!("Current testnet height: {}", height);
    }
}
