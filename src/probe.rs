use crate::disk::FilesystemLogger;

use crate::{ChannelManager, InvoicePayer, PaymentInfo, PaymentInfoStorage};
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::key::PublicKey;
use lightning::ln::msgs::ErrorAction;
use lightning::ln::msgs::LightningError;
use lightning::ln::{PaymentHash, PaymentPreimage};
use lightning::routing::network_graph::{NetworkGraph, RoutingFees};
use lightning::routing::router::PaymentParameters;
use lightning::routing::router::Route;
use lightning::routing::router::RouteParameters;
use lightning::routing::router::{find_route, RouteHint, RouteHintHop};
use lightning::routing::scoring::{ProbabilisticScorer, ProbabilisticScoringParameters};
use lightning::util::logger::Logger;
use lightning::{log_given_level, log_info, log_internal, log_trace, log_warn};

use lightning::util::events::EventHandler;
use lightning_invoice::payment::Payer;

use rand::Rng;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

pub(crate) fn probe<E: EventHandler>(
	pubkey_str: &str, channel_id_str: &str, pubkey_guess: &str, invoice_payer: &InvoicePayer<E>,
	channel_manager: Arc<ChannelManager>, network_graph: &Arc<NetworkGraph>,
	logger: &Arc<FilesystemLogger>, ldk_data_dir: &String,
	pending_payment_state: PaymentInfoStorage,
	scorer: &Arc<Mutex<ProbabilisticScorer<Arc<NetworkGraph>>>>,
) -> Result<(), Box<dyn std::error::Error>> {
	let source_pubkey = PublicKey::from_str(pubkey_str).unwrap();
	let channel_id = channel_id_str.parse::<u64>();
	if channel_id.is_err() {
		eprintln!("channel_id isn't a number");
		return Err("channel_id isn't a number")?;
	}
	let fake_preimage = rand::thread_rng().gen::<[u8; 32]>();
	let payment_hash = PaymentHash(Sha256::hash(&fake_preimage).into_inner());
	// Create the fake route information
	let guessed_fee = RoutingFees { base_msat: 1000, proportional_millionths: 1 };
	let next_route_hint = vec![RouteHint(vec![RouteHintHop {
		src_node_id: source_pubkey, // the source
		short_channel_id: channel_id.unwrap(),
		fees: guessed_fee,
		cltv_expiry_delta: 40, // the most common cltv
		htlc_minimum_msat: None,
		htlc_maximum_msat: None,
	}])];
	let route = find_routes(
		invoice_payer,
		channel_manager.clone(),
		pubkey_guess, // send to guessed pubkey instead of who we are intending as the target
		network_graph,
		logger,
		ldk_data_dir.clone(),
		next_route_hint,
		&scorer,
	);

	let route = if let Ok(route) = route {
		// paths should always be a vec<vec<hops>>
		route
	} else {
		// if no routes found, check to make sure we are still
		// connected to our node
		println!("No route: {:?}", route.err().unwrap());
		return Err("no route")?;
	};

	let preimage = PaymentPreimage(fake_preimage);
	let payment_info = PaymentInfo {
		preimage: Some(preimage),
		secret: None,
		status: crate::HTLCStatus::Pending,
		amt_msat: crate::MillisatAmount(Some(10000)),
	};
	let mut state = pending_payment_state.lock().unwrap();
	state.insert(payment_hash, payment_info);
	let payment = channel_manager.send_payment(&route, payment_hash, &None);
	match payment {
		Ok(payment_id) => {
			log_trace!(logger, "sent payment {:?}", payment_id)
		}
		Err(e) => {
			log_warn!(logger, "error sending probe {:?}", e);
			state.remove(&payment_hash);
			return Err("no route")?;
		}
	}

	Ok(())
}

pub(crate) fn find_routes<E: EventHandler>(
	_invoice_payer: &InvoicePayer<E>, channel_manager: Arc<ChannelManager>, payee_pubkey: &str,
	network: &NetworkGraph, logger: &FilesystemLogger, _ldk_data_dir: String,
	private_routes: Vec<RouteHint>, scorer: &Arc<Mutex<ProbabilisticScorer<Arc<NetworkGraph>>>>,
) -> Result<Route, LightningError> {
	let our_node_pubkey = channel_manager.get_our_node_id();

	let their_pubkey = match PublicKey::from_str(payee_pubkey) {
		Ok(pubkey) => pubkey,
		Err(_e) => {
			return Err(LightningError { err: String::new(), action: ErrorAction::IgnoreError })
		}
	};

	let payment_params =
		PaymentParameters::from_node_id(their_pubkey).with_route_hints(private_routes);
	let route_params =
		RouteParameters { payment_params, final_value_msat: 1000, final_cltv_expiry_delta: 40 };

	// Insert the fake hops at the end as route hints
	let first_hops = channel_manager.first_hops();

	let route = find_route(
		&our_node_pubkey,
		&route_params,
		network,
		Some(&first_hops.iter().collect::<Vec<_>>()),
		logger,
		&scorer.lock().unwrap(),
	);

	route
}

/// Maximum block height that can be used in a `short_channel_id`. This
/// value is based on the 3-bytes available for block height.
pub const MAX_SCID_BLOCK: u64 = 0x00ffffff;

/// Maximum transaction index that can be used in a `short_channel_id`.
/// This value is based on the 3-bytes available for tx index.
pub const MAX_SCID_TX_INDEX: u64 = 0x00ffffff;

/// Maximum vout index that can be used in a `short_channel_id`. This
/// value is based on the 2-bytes available for the vout index.
pub const MAX_SCID_VOUT_INDEX: u64 = 0xffff;

pub fn scid_from_parts(block: u64, tx_index: u64, vout_index: u64) -> u64 {
	(block << 40) | (tx_index << 16) | vout_index
}

/// Extracts the block height (most significant 3-bytes) from the `short_channel_id`
pub fn block_from_scid(short_channel_id: &u64) -> u32 {
	return (short_channel_id >> 40) as u32;
}

/// Extracts the tx index (bytes [2..4]) from the `short_channel_id`
pub fn tx_index_from_scid(short_channel_id: &u64) -> u32 {
	return ((short_channel_id >> 16) & MAX_SCID_TX_INDEX) as u32;
}

/// Extracts the vout (bytes [0..2]) from the `short_channel_id`
pub fn vout_from_scid(short_channel_id: &u64) -> u16 {
	return ((short_channel_id) & MAX_SCID_VOUT_INDEX) as u16;
}
