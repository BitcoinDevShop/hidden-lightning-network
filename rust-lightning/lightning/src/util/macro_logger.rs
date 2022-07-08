// This file is Copyright its original authors, visible in version control
// history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use chain::keysinterface::SpendableOutputDescriptor;
use chain::transaction::OutPoint;

use bitcoin::blockdata::transaction::Transaction;
use bitcoin::hash_types::Txid;
use bitcoin::secp256k1::PublicKey;

use ln::chan_utils::HTLCType;
use routing::router::Route;
use util::logger::DebugBytes;

pub(crate) struct DebugPubKey<'a>(pub &'a PublicKey);
impl<'a> core::fmt::Display for DebugPubKey<'a> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
		for i in self.0.serialize().iter() {
			write!(f, "{:02x}", i)?;
		}
		Ok(())
	}
}
macro_rules! log_pubkey {
	($obj: expr) => {
		::util::macro_logger::DebugPubKey(&$obj)
	};
}

/// Logs a byte slice in hex format.
#[macro_export]
macro_rules! log_bytes {
	($obj: expr) => {
		$crate::util::logger::DebugBytes(&$obj)
	};
}

pub(crate) struct DebugFundingChannelId<'a>(pub &'a Txid, pub u16);
impl<'a> core::fmt::Display for DebugFundingChannelId<'a> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
		for i in (OutPoint { txid: self.0.clone(), index: self.1 }).to_channel_id().iter() {
			write!(f, "{:02x}", i)?;
		}
		Ok(())
	}
}
macro_rules! log_funding_channel_id {
	($funding_txid: expr, $funding_txo: expr) => {
		::util::macro_logger::DebugFundingChannelId(&$funding_txid, $funding_txo)
	};
}

pub(crate) struct DebugFundingInfo<'a, T: 'a>(pub &'a (OutPoint, T));
impl<'a, T> core::fmt::Display for DebugFundingInfo<'a, T> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
		DebugBytes(&(self.0).0.to_channel_id()[..]).fmt(f)
	}
}
macro_rules! log_funding_info {
	($key_storage: expr) => {
		::util::macro_logger::DebugFundingInfo(&$key_storage.get_funding_txo())
	};
}

pub(crate) struct DebugRoute<'a>(pub &'a Route);
impl<'a> core::fmt::Display for DebugRoute<'a> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
		for (idx, p) in self.0.paths.iter().enumerate() {
			writeln!(f, "path {}:", idx)?;
			for h in p.iter() {
				writeln!(
					f,
					" node_id: {}, short_channel_id: {}, fee_msat: {}, cltv_expiry_delta: {}",
					log_pubkey!(h.pubkey),
					h.short_channel_id,
					h.fee_msat,
					h.cltv_expiry_delta
				)?;
			}
		}
		Ok(())
	}
}
macro_rules! log_route {
	($obj: expr) => {
		::util::macro_logger::DebugRoute(&$obj)
	};
}

pub(crate) struct DebugTx<'a>(pub &'a Transaction);
impl<'a> core::fmt::Display for DebugTx<'a> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
		if self.0.input.len() >= 1 && self.0.input.iter().any(|i| !i.witness.is_empty()) {
			if self.0.input.len() == 1
				&& self.0.input[0].witness.last().unwrap().len() == 71
				&& (self.0.input[0].sequence >> 8 * 3) as u8 == 0x80
			{
				write!(f, "commitment tx ")?;
			} else if self.0.input.len() == 1 && self.0.input[0].witness.last().unwrap().len() == 71
			{
				write!(f, "closing tx ")?;
			} else if self.0.input.len() == 1
				&& HTLCType::scriptlen_to_htlctype(self.0.input[0].witness.last().unwrap().len())
					== Some(HTLCType::OfferedHTLC)
				&& self.0.input[0].witness.len() == 5
			{
				write!(f, "HTLC-timeout tx ")?;
			} else if self.0.input.len() == 1
				&& HTLCType::scriptlen_to_htlctype(self.0.input[0].witness.last().unwrap().len())
					== Some(HTLCType::AcceptedHTLC)
				&& self.0.input[0].witness.len() == 5
			{
				write!(f, "HTLC-success tx ")?;
			} else {
				for inp in &self.0.input {
					if !inp.witness.is_empty() {
						if HTLCType::scriptlen_to_htlctype(inp.witness.last().unwrap().len())
							== Some(HTLCType::OfferedHTLC)
						{
							write!(f, "preimage-")?;
							break;
						} else if HTLCType::scriptlen_to_htlctype(inp.witness.last().unwrap().len())
							== Some(HTLCType::AcceptedHTLC)
						{
							write!(f, "timeout-")?;
							break;
						}
					}
				}
				write!(f, "tx ")?;
			}
		} else {
			debug_assert!(false, "We should never generate unknown transaction types");
			write!(f, "unknown tx type ").unwrap();
		}
		write!(f, "with txid {}", self.0.txid())?;
		Ok(())
	}
}

macro_rules! log_tx {
	($obj: expr) => {
		::util::macro_logger::DebugTx(&$obj)
	};
}

pub(crate) struct DebugSpendable<'a>(pub &'a SpendableOutputDescriptor);
impl<'a> core::fmt::Display for DebugSpendable<'a> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
		match self.0 {
			&SpendableOutputDescriptor::StaticOutput { ref outpoint, .. } => {
				write!(f, "StaticOutput {}:{} marked for spending", outpoint.txid, outpoint.index)?;
			}
			&SpendableOutputDescriptor::DelayedPaymentOutput(ref descriptor) => {
				write!(
					f,
					"DelayedPaymentOutput {}:{} marked for spending",
					descriptor.outpoint.txid, descriptor.outpoint.index
				)?;
			}
			&SpendableOutputDescriptor::StaticPaymentOutput(ref descriptor) => {
				write!(
					f,
					"StaticPaymentOutput {}:{} marked for spending",
					descriptor.outpoint.txid, descriptor.outpoint.index
				)?;
			}
		}
		Ok(())
	}
}

macro_rules! log_spendable {
	($obj: expr) => {
		::util::macro_logger::DebugSpendable(&$obj)
	};
}

/// Create a new Record and log it. You probably don't want to use this macro directly,
/// but it needs to be exported so `log_trace` etc can use it in external crates.
#[doc(hidden)]
#[macro_export]
macro_rules! log_internal {
	($logger: expr, $lvl:expr, $($arg:tt)+) => (
		$logger.log(&$crate::util::logger::Record::new($lvl, format_args!($($arg)+), module_path!(), file!(), line!()))
	);
}

/// Logs an entry at the given level.
#[macro_export]
macro_rules! log_given_level {
	($logger: expr, $lvl:expr, $($arg:tt)+) => (
		match $lvl {
			#[cfg(not(any(feature = "max_level_off")))]
			$crate::util::logger::Level::Error => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error")))]
			$crate::util::logger::Level::Warn => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn")))]
			$crate::util::logger::Level::Info => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info")))]
			$crate::util::logger::Level::Debug => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info", feature = "max_level_debug")))]
			$crate::util::logger::Level::Trace => log_internal!($logger, $lvl, $($arg)*),
			#[cfg(not(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info", feature = "max_level_debug", feature = "max_level_trace")))]
			$crate::util::logger::Level::Gossip => log_internal!($logger, $lvl, $($arg)*),

			#[cfg(any(feature = "max_level_off", feature = "max_level_error", feature = "max_level_warn", feature = "max_level_info", feature = "max_level_debug", feature = "max_level_trace"))]
			_ => {
				// The level is disabled at compile-time
			},
		}
	);
}

/// Log an error.
#[macro_export]
macro_rules! log_error {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, $crate::util::logger::Level::Error, $($arg)*);
	)
}

/// Log a warning.
#[macro_export]
macro_rules! log_warn {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, $crate::util::logger::Level::Warn, $($arg)*);
	)
}

/// Log some info.
#[macro_export]
macro_rules! log_info {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, $crate::util::logger::Level::Info, $($arg)*);
	)
}

/// Log some debug.
#[macro_export]
macro_rules! log_debug {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, $crate::util::logger::Level::Debug, $($arg)*);
	)
}

/// Log a trace log.
#[macro_export]
macro_rules! log_trace {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, $crate::util::logger::Level::Trace, $($arg)*)
	)
}

/// Log a gossip log.
#[macro_export]
macro_rules! log_gossip {
	($logger: expr, $($arg:tt)*) => (
		log_given_level!($logger, $crate::util::logger::Level::Gossip, $($arg)*);
	)
}
