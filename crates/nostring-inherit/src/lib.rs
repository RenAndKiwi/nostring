//! NoString Inheritance Module
//!
//! Bitcoin timelock-based inheritance using miniscript, adapted from Liana.
//!
//! # Concepts
//!
//! - **Primary path**: Owner can always spend
//! - **Recovery path(s)**: Heir(s) can spend after timelock expires
//! - **Check-in**: Owner spends and recreates UTXO to reset timelock
//!
//! # Example Policy
//!
//! ```text
//! or(
//!   pk(OWNER),
//!   and(
//!     thresh(2, pk(HEIR1), pk(HEIR2), pk(HEIR3)),
//!     older(26280)  // ~6 months
//!   )
//! )
//! ```

pub mod checkin;
pub mod heir;
pub mod policy;

// TODO: Port from Liana:
// - Miniscript policy construction
// - Descriptor generation
// - PSBT creation for check-in
// - Recovery path management
