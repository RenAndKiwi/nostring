//! Check-in transaction handling
//!
//! Spend and recreate timelock UTXO to reset the clock.

// TODO: Implement check-in flow:
// 1. Find current timelock UTXO
// 2. Create spend transaction (owner path)
// 3. Create new UTXO with same policy
// 4. Broadcast
