//! Watch events emitted by the monitoring service

use bitcoin::{Amount, OutPoint, Txid};
use serde::{Deserialize, Serialize};

/// Events emitted by the WatchService when UTXO state changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WatchEvent {
    /// A new UTXO appeared for a watched policy
    UtxoAppeared {
        /// Policy identifier (descriptor hash or user-provided label)
        policy_id: String,
        /// The new UTXO
        outpoint: OutPoint,
        /// Value in satoshis
        value: Amount,
        /// Block height where confirmed (0 if mempool)
        height: u32,
    },

    /// A watched UTXO was spent
    UtxoSpent {
        /// Policy identifier
        policy_id: String,
        /// The spent UTXO
        outpoint: OutPoint,
        /// Transaction that spent it
        spending_txid: Txid,
        /// Whether this appears to be an owner check-in or heir claim
        /// (heuristic based on output analysis)
        spend_type: SpendType,
    },

    /// Timelock is approaching expiry
    /// (Delegates to nostring-notify for actual alerting)
    TimelockWarning {
        /// Policy identifier
        policy_id: String,
        /// Blocks remaining until timelock expires
        blocks_remaining: i64,
        /// Approximate days remaining
        days_remaining: f64,
    },

    /// Error during polling (network issue, server unavailable)
    PollError {
        /// Error message
        message: String,
    },
}

/// Type of spend detected
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpendType {
    /// Owner spent via the immediate path (check-in)
    OwnerCheckin,
    /// Heir spent via the timelock path (claim)
    HeirClaim,
    /// Could not determine spend type
    Unknown,
}

impl WatchEvent {
    /// Get the policy_id if this event is associated with one
    pub fn policy_id(&self) -> Option<&str> {
        match self {
            WatchEvent::UtxoAppeared { policy_id, .. } => Some(policy_id),
            WatchEvent::UtxoSpent { policy_id, .. } => Some(policy_id),
            WatchEvent::TimelockWarning { policy_id, .. } => Some(policy_id),
            WatchEvent::PollError { .. } => None,
        }
    }

    /// Check if this is an error event
    pub fn is_error(&self) -> bool {
        matches!(self, WatchEvent::PollError { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_event_policy_id() {
        let event = WatchEvent::UtxoAppeared {
            policy_id: "test-policy".to_string(),
            outpoint: OutPoint::from_str(
                "0000000000000000000000000000000000000000000000000000000000000000:0",
            )
            .unwrap(),
            value: Amount::from_sat(100000),
            height: 934000,
        };

        assert_eq!(event.policy_id(), Some("test-policy"));
        assert!(!event.is_error());
    }

    #[test]
    fn test_poll_error() {
        let event = WatchEvent::PollError {
            message: "Connection refused".to_string(),
        };

        assert!(event.policy_id().is_none());
        assert!(event.is_error());
    }

    #[test]
    fn test_spend_types() {
        assert_ne!(SpendType::OwnerCheckin, SpendType::HeirClaim);
        assert_ne!(SpendType::OwnerCheckin, SpendType::Unknown);
    }
}
