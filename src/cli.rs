use crate::disk::FilesystemLogger;
use crate::hex_utils;
use crate::probe::{block_from_scid, find_routes, probe, scid_from_parts, vout_from_scid};
use crate::{disk, PaymentState};
use anyhow::Result;
use lightning::routing::gossip::{NodeAlias, NodeId};
use std::str;
use std::time::Instant;
use std::{fs::File, io::BufWriter};

use crate::{
	ChannelManager, HTLCStatus, InvoicePayer, MillisatAmount, NetworkGraph, PaymentInfo,
	PaymentInfoStorage, PeerManager,
};
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::network::constants::Network;
use bitcoin::secp256k1::PublicKey;
use chrono::NaiveDateTime;
use ctrlc;
use lightning::chain::keysinterface::{KeysInterface, KeysManager, Recipient};
use lightning::ln::channelmanager::PaymentSendFailure;
use rusqlite::Connection;
use std::collections::HashMap;

use lightning::ln::msgs::NetAddress;
use lightning::ln::{PaymentHash, PaymentPreimage};
use lightning::routing::scoring::ProbabilisticScorer;
use lightning::util::config::{ChannelHandshakeConfig, ChannelHandshakeLimits, UserConfig};
use lightning::util::events::EventHandler;
use lightning::util::logger::Logger;
use lightning::{log_given_level, log_info, log_internal, log_trace};
use lightning_invoice::payment::PaymentError;
use lightning_invoice::{utils, Currency, Invoice};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::io::{BufRead, Write};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::ops::Deref;
use std::path::Path;
use std::process;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Node {
	pubkey: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Transaction {
	//block_hash: String,
	block_height: u64,
	id: String,
	block_index: u64,
	transaction_index: u64,
	amount: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Attempt {
	pub(crate) target_pubkey: String,
	pub(crate) guess_pubkey: String,
	pub(crate) channel_id: String,
	pub(crate) result: String,
	pub(crate) date_found: NaiveDateTime,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct AttemptResult {
	pub(crate) target_pubkey: String,
	pub(crate) other_pubkey: String,
	pub(crate) channel_id: String,
	pub(crate) transaction_id_output: String,
	pub(crate) amount: u64,
	pub(crate) block_open: u32,
	pub(crate) date_found: NaiveDateTime,
}

pub(crate) struct LdkUserInfo {
	pub(crate) bitcoind_rpc_username: String,
	pub(crate) bitcoind_rpc_password: String,
	pub(crate) bitcoind_rpc_port: u16,
	pub(crate) bitcoind_rpc_host: String,
	pub(crate) ldk_storage_dir_path: String,
	pub(crate) ldk_peer_listening_port: u16,
	pub(crate) ldk_announced_listen_addr: Vec<NetAddress>,
	pub(crate) ldk_announced_node_name: [u8; 32],
	pub(crate) network: Network,
}

pub(crate) fn parse_startup_args() -> Result<LdkUserInfo, ()> {
	if env::args().len() < 3 {
		println!("ldk-tutorial-node requires 3 arguments: `cargo run <bitcoind-rpc-username>:<bitcoind-rpc-password>@<bitcoind-rpc-host>:<bitcoind-rpc-port> ldk_storage_directory_path [<ldk-incoming-peer-listening-port>] [bitcoin-network] [announced-node-name announced-listen-addr*]`");
	}
	let bitcoind_rpc_info = env::args().skip(1).next().unwrap();
	let bitcoind_rpc_info_parts: Vec<&str> = bitcoind_rpc_info.rsplitn(2, "@").collect();
	if bitcoind_rpc_info_parts.len() != 2 {
		println!("ERROR: bad bitcoind RPC URL provided");
		return Err(());
	}
	let rpc_user_and_password: Vec<&str> = bitcoind_rpc_info_parts[1].split(":").collect();
	if rpc_user_and_password.len() != 2 {
		println!("ERROR: bad bitcoind RPC username/password combo provided");
		return Err(());
	}
	let bitcoind_rpc_username = rpc_user_and_password[0].to_string();
	let bitcoind_rpc_password = rpc_user_and_password[1].to_string();
	let bitcoind_rpc_path: Vec<&str> = bitcoind_rpc_info_parts[0].split(":").collect();
	if bitcoind_rpc_path.len() != 2 {
		println!("ERROR: bad bitcoind RPC path provided");
		return Err(());
	}
	let bitcoind_rpc_host = bitcoind_rpc_path[0].to_string();
	let bitcoind_rpc_port = bitcoind_rpc_path[1].parse::<u16>().unwrap();

	let ldk_storage_dir_path = env::args().skip(2).next().unwrap();

	let mut ldk_peer_port_set = true;
	let ldk_peer_listening_port: u16 = match env::args().skip(3).next().map(|p| p.parse()) {
		Some(Ok(p)) => p,
		Some(Err(_)) => {
			ldk_peer_port_set = false;
			9735
		}
		None => {
			ldk_peer_port_set = false;
			9735
		}
	};

	let mut arg_idx = match ldk_peer_port_set {
		true => 4,
		false => 3,
	};
	let network: Network = match env::args().skip(arg_idx).next().as_ref().map(String::as_str) {
		Some("mainnet") => Network::Bitcoin,
		Some("testnet") => Network::Testnet,
		Some("regtest") => Network::Regtest,
		Some("signet") => Network::Signet,
		Some(net) => {
			panic!("Unsupported network provided. Options are: `regtest`, `testnet`, and `signet`. Got {}", net);
		}
		None => Network::Testnet,
	};

	let ldk_announced_node_name = match env::args().skip(arg_idx + 1).next().as_ref() {
		Some(s) => {
			if s.len() > 32 {
				panic!("Node Alias can not be longer than 32 bytes");
			}
			arg_idx += 1;
			let mut bytes = [0; 32];
			bytes[..s.len()].copy_from_slice(s.as_bytes());
			bytes
		}
		None => [0; 32],
	};

	let mut ldk_announced_listen_addr = Vec::new();
	loop {
		match env::args().skip(arg_idx + 1).next().as_ref() {
			Some(s) => match IpAddr::from_str(s) {
				Ok(IpAddr::V4(a)) => {
					ldk_announced_listen_addr
						.push(NetAddress::IPv4 { addr: a.octets(), port: ldk_peer_listening_port });
					arg_idx += 1;
				}
				Ok(IpAddr::V6(a)) => {
					ldk_announced_listen_addr
						.push(NetAddress::IPv6 { addr: a.octets(), port: ldk_peer_listening_port });
					arg_idx += 1;
				}
				Err(_) => panic!("Failed to parse announced-listen-addr into an IP address"),
			},
			None => break,
		}
	}

	Ok(LdkUserInfo {
		bitcoind_rpc_username,
		bitcoind_rpc_password,
		bitcoind_rpc_host,
		bitcoind_rpc_port,
		ldk_storage_dir_path,
		ldk_peer_listening_port,
		ldk_announced_listen_addr,
		ldk_announced_node_name,
		network,
	})
}

pub(crate) async fn poll_for_user_input<E: EventHandler>(
	_pending_payment_state: PaymentState, invoice_payer: Arc<InvoicePayer<E>>,
	peer_manager: Arc<PeerManager>, channel_manager: Arc<ChannelManager>,
	keys_manager: Arc<KeysManager>, inbound_payments: PaymentInfoStorage,
	outbound_payments: PaymentInfoStorage, pending_payments: PaymentInfoStorage,
	ldk_data_dir: String, network: Network, network_graph: Arc<NetworkGraph>,
	logger: Arc<FilesystemLogger>,
	scorer: Arc<Mutex<ProbabilisticScorer<Arc<NetworkGraph>, Arc<FilesystemLogger>>>>,
	db: Arc<Mutex<rusqlite::Connection>>,
) {
	println!("LDK startup successful. To view available commands: \"help\".");
	println!("LDK logs are available at <your-supplied-ldk-data-dir-path>/.ldk/logs");
	println!("Local Node ID is {}.", channel_manager.get_our_node_id());

	let running = Arc::new(AtomicUsize::new(0));
	let r = running.clone();
	ctrlc::set_handler(move || {
		let prev = r.fetch_add(1, Ordering::SeqCst);
		if prev == 0 {
			println!("Exiting...");
		} else {
			process::exit(0);
		}
	})
	.expect("Error setting Ctrl-C handler");

	let stdin = io::stdin();
	let mut line_reader = stdin.lock().lines();
	loop {
		print!("> ");
		io::stdout().flush().unwrap(); // Without flushing, the `>` doesn't print
		let line = match line_reader.next() {
			Some(l) => l.unwrap(),
			None => break,
		};
		let mut words = line.split_whitespace();
		if let Some(word) = words.next() {
			match word {
				"help" => help(),
				"stop" => {
					println!("Stopping the program");
					break;
				}
				"openchannel" => {
					let peer_pubkey_and_ip_addr = words.next();
					let channel_value_sat = words.next();
					if peer_pubkey_and_ip_addr.is_none() || channel_value_sat.is_none() {
						println!("ERROR: openchannel has 2 required arguments: `openchannel pubkey@host:port channel_amt_satoshis` [--public]");
						continue;
					}
					let peer_pubkey_and_ip_addr = peer_pubkey_and_ip_addr.unwrap();
					let (pubkey, peer_addr) =
						match parse_peer_info(peer_pubkey_and_ip_addr.to_string()) {
							Ok(info) => info,
							Err(e) => {
								println!("{:?}", e.into_inner().unwrap());
								continue;
							}
						};

					let chan_amt_sat: Result<u64, _> = channel_value_sat.unwrap().parse();
					if chan_amt_sat.is_err() {
						println!("ERROR: channel amount must be a number");
						continue;
					}

					if connect_peer_if_necessary(pubkey, peer_addr, peer_manager.clone())
						.await
						.is_err()
					{
						continue;
					};

					let announce_channel = match words.next() {
						Some("--public") | Some("--public=true") => true,
						Some("--public=false") => false,
						Some(_) => {
							println!("ERROR: invalid `--public` command format. Valid formats: `--public`, `--public=true` `--public=false`");
							continue;
						}
						None => false,
					};

					if open_channel(
						pubkey,
						chan_amt_sat.unwrap(),
						announce_channel,
						channel_manager.clone(),
					)
					.is_ok()
					{
						let peer_data_path = format!("{}/channel_peer_data", ldk_data_dir.clone());
						let _ = disk::persist_channel_peer(
							Path::new(&peer_data_path),
							peer_pubkey_and_ip_addr,
						);
					}
				}
				"sendpayment" => {
					let invoice_str = words.next();
					if invoice_str.is_none() {
						println!("ERROR: sendpayment requires an invoice: `sendpayment <invoice>`");
						continue;
					}

					let invoice = match Invoice::from_str(invoice_str.unwrap()) {
						Ok(inv) => inv,
						Err(e) => {
							println!("ERROR: invalid invoice: {:?}", e);
							continue;
						}
					};

					send_payment(&*invoice_payer, &invoice, outbound_payments.clone());
				}
				"keysend" => {
					let dest_pubkey = match words.next() {
						Some(dest) => match hex_utils::to_compressed_pubkey(dest) {
							Some(pk) => pk,
							None => {
								println!("ERROR: couldn't parse destination pubkey");
								continue;
							}
						},
						None => {
							println!("ERROR: keysend requires a destination pubkey: `keysend <dest_pubkey> <amt_msat>`");
							continue;
						}
					};
					let amt_msat_str = match words.next() {
						Some(amt) => amt,
						None => {
							println!("ERROR: keysend requires an amount in millisatoshis: `keysend <dest_pubkey> <amt_msat>`");
							continue;
						}
					};
					let amt_msat: u64 = match amt_msat_str.parse() {
						Ok(amt) => amt,
						Err(e) => {
							println!("ERROR: couldn't parse amount_msat: {}", e);
							continue;
						}
					};
					keysend(
						&*invoice_payer,
						dest_pubkey,
						amt_msat,
						&*keys_manager,
						outbound_payments.clone(),
					);
				}
				"getinvoice" => {
					let amt_str = words.next();
					if amt_str.is_none() {
						println!("ERROR: getinvoice requires an amount in millisatoshis");
						continue;
					}

					let amt_msat: Result<u64, _> = amt_str.unwrap().parse();
					if amt_msat.is_err() {
						println!("ERROR: getinvoice provided payment amount was not a number");
						continue;
					}

					let expiry_secs_str = words.next();
					if expiry_secs_str.is_none() {
						println!("ERROR: getinvoice requires an expiry in seconds");
						continue;
					}

					let expiry_secs: Result<u32, _> = expiry_secs_str.unwrap().parse();
					if expiry_secs.is_err() {
						println!("ERROR: getinvoice provided expiry was not a number");
						continue;
					}

					get_invoice(
						amt_msat.unwrap(),
						inbound_payments.clone(),
						channel_manager.clone(),
						keys_manager.clone(),
						network,
						expiry_secs.unwrap(),
					);
				}
				"connectpeer" => {
					let peer_pubkey_and_ip_addr = words.next();
					if peer_pubkey_and_ip_addr.is_none() {
						println!("ERROR: connectpeer requires peer connection info: `connectpeer pubkey@host:port`");
						continue;
					}
					let (pubkey, peer_addr) =
						match parse_peer_info(peer_pubkey_and_ip_addr.unwrap().to_string()) {
							Ok(info) => info,
							Err(e) => {
								println!("{:?}", e.into_inner().unwrap());
								continue;
							}
						};
					if connect_peer_if_necessary(pubkey, peer_addr, peer_manager.clone())
						.await
						.is_ok()
					{
						println!("SUCCESS: connected to peer {}", pubkey);
					}
				}
				"listchannels" => list_channels(&channel_manager, &network_graph),
				"listpayments" => {
					list_payments(inbound_payments.clone(), outbound_payments.clone())
				}
				"closechannel" => {
					let channel_id_str = words.next();
					if channel_id_str.is_none() {
						println!("ERROR: closechannel requires a channel ID: `closechannel <channel_id> <peer_pubkey>`");
						continue;
					}
					let channel_id_vec = hex_utils::to_vec(channel_id_str.unwrap());
					if channel_id_vec.is_none() || channel_id_vec.as_ref().unwrap().len() != 32 {
						println!("ERROR: couldn't parse channel_id");
						continue;
					}
					let mut channel_id = [0; 32];
					channel_id.copy_from_slice(&channel_id_vec.unwrap());

					let peer_pubkey_str = words.next();
					if peer_pubkey_str.is_none() {
						println!("ERROR: closechannel requires a peer pubkey: `closechannel <channel_id> <peer_pubkey>`");
						continue;
					}
					let peer_pubkey_vec = match hex_utils::to_vec(peer_pubkey_str.unwrap()) {
						Some(peer_pubkey_vec) => peer_pubkey_vec,
						None => {
							println!("ERROR: couldn't parse peer_pubkey");
							continue;
						}
					};
					let peer_pubkey = match PublicKey::from_slice(&peer_pubkey_vec) {
						Ok(peer_pubkey) => peer_pubkey,
						Err(_) => {
							println!("ERROR: couldn't parse peer_pubkey");
							continue;
						}
					};

					close_channel(channel_id, peer_pubkey, channel_manager.clone());
				}
				"forceclosechannel" => {
					let channel_id_str = words.next();
					if channel_id_str.is_none() {
						println!("ERROR: forceclosechannel requires a channel ID: `forceclosechannel <channel_id> <peer_pubkey>`");
						continue;
					}
					let channel_id_vec = hex_utils::to_vec(channel_id_str.unwrap());
					if channel_id_vec.is_none() || channel_id_vec.as_ref().unwrap().len() != 32 {
						println!("ERROR: couldn't parse channel_id");
						continue;
					}
					let mut channel_id = [0; 32];
					channel_id.copy_from_slice(&channel_id_vec.unwrap());

					let peer_pubkey_str = words.next();
					if peer_pubkey_str.is_none() {
						println!("ERROR: forceclosechannel requires a peer pubkey: `forceclosechannel <channel_id> <peer_pubkey>`");
						continue;
					}
					let peer_pubkey_vec = match hex_utils::to_vec(peer_pubkey_str.unwrap()) {
						Some(peer_pubkey_vec) => peer_pubkey_vec,
						None => {
							println!("ERROR: couldn't parse peer_pubkey");
							continue;
						}
					};
					let peer_pubkey = match PublicKey::from_slice(&peer_pubkey_vec) {
						Ok(peer_pubkey) => peer_pubkey,
						Err(_) => {
							println!("ERROR: couldn't parse peer_pubkey");
							continue;
						}
					};

					force_close_channel(channel_id, peer_pubkey, channel_manager.clone());
				}
				"nodeinfo" => node_info(channel_manager.clone(), peer_manager.clone()),
				"listpeers" => list_peers(peer_manager.clone()),
				"signmessage" => {
					const MSG_STARTPOS: usize = "signmessage".len() + 1;
					if line.as_bytes().len() <= MSG_STARTPOS {
						println!("ERROR: signmsg requires a message");
						continue;
					}
					println!(
						"{:?}",
						lightning::util::message_signing::sign(
							&line.as_bytes()[MSG_STARTPOS..],
							&keys_manager.get_node_secret(Recipient::Node).unwrap()
						)
					);
				}
				"findroutes" => {
					let pubkey_str = words.next();
					if pubkey_str.is_none() {
						println!("ERROR: findroutes requires a pubkey: `findroutes <pubkey>`");
						continue;
					}

					let route = find_routes(
						&invoice_payer,
						channel_manager.clone(),
						pubkey_str.unwrap(),
						&network_graph,
						&logger,
						ldk_data_dir.clone(),
						vec![],
						&scorer,
					);

					if let Ok(route) = route {
						dbg!(route.paths);
					} else {
						println!("No route found")
					}
				}
				"sendfakepayment" => {
					let pubkey_str = words.next();
					if pubkey_str.is_none() {
						println!("ERROR: findroutes requires a pubkey: `findroutes <pubkey>`");
						continue;
					}

					let route = find_routes(
						&invoice_payer,
						channel_manager.clone(),
						pubkey_str.unwrap(),
						&network_graph,
						&logger,
						ldk_data_dir.clone(),
						vec![],
						&scorer,
					);

					let fake_preimage = rand::thread_rng().gen::<[u8; 32]>();

					let payment_hash = PaymentHash(Sha256::hash(&fake_preimage).into_inner());

					if let Ok(route) = route {
						let payment = channel_manager.send_payment(&route, payment_hash, &None);
						match payment {
							Ok(_payment_id) => {
								println!("Payment attempt sent");
							}
							Err(e) => match e {
								PaymentSendFailure::ParameterError(e) => {
									println!("parameter error");
									dbg!(e);
								}
								PaymentSendFailure::PathParameterError(e) => {
									println!("path parameter error");
									dbg!(e);
								}
								PaymentSendFailure::AllFailedRetrySafe(e) => {
									println!("all failed retry safe");
									dbg!(e);
								}
								PaymentSendFailure::PartialFailure {
									results,
									failed_paths_retry,
									payment_id,
								} => {
									println!(
										"partial failure: {:?} {:?} {:?}",
										results, failed_paths_retry, payment_id
									)
								}
							},
						}
					}
				}
				"probeprivate" => {
					let pubkey_str = words.next();
					let pubkey_guess = words.next();
					let channel_id_str = words.next();

					if pubkey_str.is_none() || channel_id_str.is_none() || pubkey_guess.is_none() {
						println!("ERROR: probeprivate requires pubkey, guessed pubkey, and channel_id: `probeprivate <pubkey> <pubkey_guess> <channel_id>`");
						continue;
					}

					match probe(
						pubkey_str.unwrap(),
						channel_id_str.unwrap(),
						pubkey_guess.unwrap(),
						&invoice_payer,
						channel_manager.clone(),
						&network_graph,
						&logger,
						&ldk_data_dir,
						pending_payments.clone(),
						&scorer,
					) {
						Ok(_) => continue,
						Err(_) => continue,
					}
				}
				"probeall" => {
					let probetype = words.next();
					let nodepath = words.next();
					let txpath = words.next();

					if probetype.is_none() || nodepath.is_none() || txpath.is_none() {
						println!("ERROR: probeall requires type nodefile and txfile: `probeprivate <probetype> <nodefile> <txfile>`");
						continue;
					}

					// if nodefile is "all" then read from network graph
					let mut nodes: Vec<Node> = vec![];
					if nodepath.unwrap() == "all" {
						for (pubkey, _) in network_graph.read_only().nodes() {
							println!(
								"trying to parse pubkey {:?} : {:?}",
								pubkey,
								hex::encode(pubkey.as_slice())
							);
							let pubkey_str = hex::encode(pubkey.as_slice());
							nodes.push(Node { pubkey: String::from(pubkey_str) });
						}
					} else {
						// Parse nodefile
						let node_data = match fs::read_to_string(nodepath.unwrap()) {
							Ok(file) => file,
							Err(e) => {
								println!("{:?}", e.into_inner().unwrap());
								continue;
							}
						};
						nodes = match serde_json::from_str(&node_data) {
							Ok(n) => n,
							Err(e) => {
								println!("{:?}", e);
								continue;
							}
						};
					}

					// get a list of public channels

					let short_channel_ids = network_graph.read_only().channels().clone();

					// Parse tx files
					let mut txs: Vec<Transaction> = vec![];
					let read_res = fs::read_dir(txpath.unwrap());
					match read_res {
						Ok(txdir) => {
							for json_file in txdir {
								println!(
									"Reading tx file: {:?}",
									json_file.as_ref().unwrap().file_name().as_os_str().to_str()
								);
								let tx_data = match fs::read_to_string(json_file.unwrap().path()) {
									Ok(file) => file,
									Err(e) => {
										println!("{:?}", e.into_inner().unwrap());
										continue;
									}
								};
								let mut new_txs: Vec<Transaction> =
									match serde_json::from_str(&tx_data) {
										Ok(n) => n,
										Err(e) => {
											println!("{:?}", e);
											continue;
										}
									};
								//println!("{:?}", new_txs);
								txs.append(&mut new_txs);
							}
						}
						Err(e) => {
							println!("{:?}", e.into_inner().unwrap());
							continue;
						}
					}

					// sort the transaction with recent (highest number) blocks
					log_info!(logger, "sorting transactions by height");
					txs.sort_by(|a, b| b.block_height.cmp(&a.block_height));

					// This doesn't matter right now so we'll hardcode it
					let pubkey_guess =
						"03b2c32c46e0b4b720c4f45f02a0cc4c5475df7ce4d5b1ab563961b1681c6917d6";

					let set_of_attempts = get_attempts_str(&db.clone().lock().unwrap()).unwrap();

					log_info!(logger, "Starting probing...");
					let mut total_probes = 1;
					let probe_start = Instant::now();

					for node in nodes {
						// first try to see if we can even find normal routes first

						let route = find_routes(
							&invoice_payer,
							channel_manager.clone(),
							&node.pubkey,
							&network_graph,
							&logger,
							ldk_data_dir.clone(),
							vec![],
							&scorer,
						);

						match route {
							Ok(_) => (),
							Err(_) => {
								log_info!(
									logger,
									"No routes to node {}, skipping probes...",
									&node.pubkey
								);
								continue;
							}
						}

						let txptr = &txs;
						for (i, tx) in txptr.into_iter().enumerate() {
							// check if the program has been requested to
							// close down

							loop {
								// check signal in this loop in case we
								// are htlc stalled
								if running.load(Ordering::SeqCst) > 0 {
									break;
								}
								let pending_outbound_payments = pending_payments.lock().unwrap();
								// Assuming only 1 channel, TODO a better check
								let len = pending_outbound_payments.keys().len();
								drop(pending_outbound_payments);
								if len > 30 {
									// wait for pending htlc's to clear
									log_trace!(logger, "Close to max htlc's, waiting...");
									thread::sleep(Duration::from_millis(50));
									continue;
								}
								break;
							}

							// check signal again
							if running.load(Ordering::SeqCst) > 0 {
								break;
							}

							// check if we are running with assumptions
							if probetype.unwrap() == "assumptions" {
								if tx.amount % 10000 != 0 {
									continue;
								}
								if tx.transaction_index > 1 {
									continue;
								}
							}

							let scid = scid_from_parts(
								tx.block_height,
								tx.block_index,
								tx.transaction_index,
							);

							// make sure scid is not in public channel
							// list
							match short_channel_ids.get(&scid) {
								Some(_) => {
									log_info!(logger, "skipping public chan {}", scid.to_string());
									continue;
								}
								None => (),
							}

							let attempt = format!("{}:{}", node.pubkey, scid.to_string());
							if set_of_attempts.contains_key(&attempt) {
								log_info!(logger, "skipping attempt {}", attempt);
								continue;
							}

							let mut elapsed = probe_start.elapsed().as_secs();
							if elapsed == 0 {
								elapsed = 1;
							}
							println!(
								"{} {} of {} | tx {}:{} (tps: {}, total: {}s)",
								total_probes,
								i,
								txptr.len(),
								node.pubkey,
								scid.to_string(),
								total_probes / elapsed,
								probe_start.elapsed().as_secs_f64()
							);
							total_probes += 1;
							loop {
								match probe(
									&node.pubkey,
									&scid.to_string(),
									pubkey_guess,
									&invoice_payer,
									channel_manager.clone(),
									&network_graph,
									&logger,
									&ldk_data_dir,
									pending_payments.clone(),
									&scorer,
								) {
									Ok(_) => break,
									Err(_) => {
										// keep probing this
										// one until it
										// succeeds
										thread::sleep(Duration::from_millis(100));
										continue;
									}
								}
							}
						}
						log_info!(logger, "Probing next node...");
					}
				}
				"dump_results" => {
					let result_dir = words.next();
					let txpath = words.next();

					if result_dir.is_none() || txpath.is_none() {
						println!(
							"ERROR: dump_results requires result_dir tx_dir: `dump_results <result_dir> <tx_dir>`"
						);
						continue;
					}

					// Parse tx files
					let mut txs: HashMap<u64, Transaction> = HashMap::new();
					let read_res = fs::read_dir(txpath.unwrap());
					match read_res {
						Ok(txdir) => {
							for json_file in txdir {
								println!(
									"Reading tx file: {:?}",
									json_file.as_ref().unwrap().file_name().as_os_str().to_str()
								);
								let tx_data = match fs::read_to_string(json_file.unwrap().path()) {
									Ok(file) => file,
									Err(e) => {
										println!("{:?}", e.into_inner().unwrap());
										continue;
									}
								};
								let new_txs: Vec<Transaction> = match serde_json::from_str(&tx_data)
								{
									Ok(n) => n,
									Err(e) => {
										println!("{:?}", e);
										continue;
									}
								};
								for tx in new_txs {
									let scid = scid_from_parts(
										tx.block_height,
										tx.block_index,
										tx.transaction_index,
									);
									txs.insert(scid, tx);
								}
							}
						}
						Err(e) => {
							println!("{:?}", e.into_inner().unwrap());
							continue;
						}
					}

					let attempts = get_attempts_found(&db.clone().lock().unwrap()).unwrap();
					let mut results: Vec<AttemptResult> = vec![];
					for attempt in attempts {
						let mut result = AttemptResult {
							target_pubkey: attempt.target_pubkey,
							other_pubkey: "".to_string(),
							channel_id: attempt.channel_id.clone(),
							transaction_id_output: "".to_string(),
							amount: 0,
							block_open: block_from_scid(
								&attempt.channel_id.parse::<u64>().unwrap().clone(),
							),
							date_found: attempt.date_found,
						};

						let output_index =
							vout_from_scid(&attempt.channel_id.parse::<u64>().unwrap().clone());

						// go through tx set and find txid and amount
						let tx = txs.get(&attempt.channel_id.parse::<u64>().unwrap().clone());
						match tx {
							Some(utxo) => {
								result.transaction_id_output =
									format!("{}:{}", utxo.id, output_index);
								result.amount = utxo.amount;
							}
							None => {
								// TODO if not found in this set (bc spent), do an
								// electrum lookup?
							}
						}

						if attempt.result == "incorrect_or_unknown_payment_details" {
							result.other_pubkey = attempt.guess_pubkey;
						}

						results.push(result);
					}

					let writer = BufWriter::new(
						File::create(format!("{}/results.json", result_dir.unwrap())).unwrap(),
					);
					serde_json::to_writer_pretty(writer, &results).unwrap();
				}
				_ => println!("Unknown command. See `\"help\" for available commands."),
			}
		}
	}
}

fn help() {
	println!("openchannel pubkey@host:port <amt_satoshis>");
	println!("sendpayment <invoice>");
	println!("getinvoice <amt_millisatoshis>");
	println!("connectpeer pubkey@host:port");
	println!("listchannels");
	println!("listpayments");
	println!("closechannel <channel_id>");
	println!("forceclosechannel <channel_id>");
	println!("nodeinfo");
	println!("listpeers");
	println!("signmessage <message>");
	println!("findroutes <pubkey>");
	println!("sendfakepayment <pubkey>");
	println!("probeprivate <pubkey> <guessed_node> <channel_id>");
	println!("probeprivate <nodefile> <txfile>");
}

fn node_info(channel_manager: Arc<ChannelManager>, peer_manager: Arc<PeerManager>) {
	println!("\t{{");
	println!("\t\t node_pubkey: {}", channel_manager.get_our_node_id());
	let chans = channel_manager.list_channels();
	println!("\t\t num_channels: {}", chans.len());
	println!("\t\t num_usable_channels: {}", chans.iter().filter(|c| c.is_usable).count());
	let local_balance_msat = chans
		.iter()
		.map(|c| c.unspendable_punishment_reserve.unwrap_or(0) * 1000 + c.outbound_capacity_msat)
		.sum::<u64>();
	println!("\t\t local_balance_msat: {}", local_balance_msat);
	println!("\t\t num_peers: {}", peer_manager.get_peer_node_ids().len());
	println!("\t}},");
}

fn list_peers(peer_manager: Arc<PeerManager>) {
	println!("\t{{");
	for pubkey in peer_manager.get_peer_node_ids() {
		println!("\t\t pubkey: {}", pubkey);
	}
	println!("\t}},");
}

fn list_channels(channel_manager: &Arc<ChannelManager>, network_graph: &Arc<NetworkGraph>) {
	print!("[");
	for chan_info in channel_manager.list_channels() {
		println!("");
		println!("\t{{");
		println!("\t\tchannel_id: {},", hex_utils::hex_str(&chan_info.channel_id[..]));
		if let Some(funding_txo) = chan_info.funding_txo {
			println!("\t\tfunding_txid: {},", funding_txo.txid);
		}

		println!(
			"\t\tpeer_pubkey: {},",
			hex_utils::hex_str(&chan_info.counterparty.node_id.serialize())
		);
		if let Some(node_info) = network_graph
			.read_only()
			.nodes()
			.get(&NodeId::from_pubkey(&chan_info.counterparty.node_id))
		{
			if let Some(announcement) = &node_info.announcement_info {
				println!("\t\tpeer_alias: {}", &announcement.alias);
			}
		}

		if let Some(id) = chan_info.short_channel_id {
			println!("\t\tshort_channel_id: {},", id);
		}
		println!("\t\tis_channel_ready: {},", chan_info.is_channel_ready);
		println!("\t\tchannel_value_satoshis: {},", chan_info.channel_value_satoshis);
		println!("\t\tlocal_balance_msat: {},", chan_info.balance_msat);
		if chan_info.is_usable {
			println!("\t\tavailable_balance_for_send_msat: {},", chan_info.outbound_capacity_msat);
			println!("\t\tavailable_balance_for_recv_msat: {},", chan_info.inbound_capacity_msat);
		}
		println!("\t\tchannel_can_send_payments: {},", chan_info.is_usable);
		println!("\t\tpublic: {},", chan_info.is_public);
		println!("\t}},");
	}
	println!("]");
}

fn list_payments(inbound_payments: PaymentInfoStorage, outbound_payments: PaymentInfoStorage) {
	let inbound = inbound_payments.lock().unwrap();
	let outbound = outbound_payments.lock().unwrap();
	print!("[");
	for (payment_hash, payment_info) in inbound.deref() {
		println!("");
		println!("\t{{");
		println!("\t\tamount_millisatoshis: {},", payment_info.amt_msat);
		println!("\t\tpayment_hash: {},", hex_utils::hex_str(&payment_hash.0));
		println!("\t\thtlc_direction: inbound,");
		println!(
			"\t\thtlc_status: {},",
			match payment_info.status {
				HTLCStatus::Pending => "pending",
				HTLCStatus::Succeeded => "succeeded",
				HTLCStatus::Failed => "failed",
			}
		);

		println!("\t}},");
	}

	for (payment_hash, payment_info) in outbound.deref() {
		println!("");
		println!("\t{{");
		println!("\t\tamount_millisatoshis: {},", payment_info.amt_msat);
		println!("\t\tpayment_hash: {},", hex_utils::hex_str(&payment_hash.0));
		println!("\t\thtlc_direction: outbound,");
		println!(
			"\t\thtlc_status: {},",
			match payment_info.status {
				HTLCStatus::Pending => "pending",
				HTLCStatus::Succeeded => "succeeded",
				HTLCStatus::Failed => "failed",
			}
		);

		println!("\t}},");
	}
	println!("]");
}

pub(crate) async fn connect_peer_if_necessary(
	pubkey: PublicKey, peer_addr: SocketAddr, peer_manager: Arc<PeerManager>,
) -> Result<(), ()> {
	for node_pubkey in peer_manager.get_peer_node_ids() {
		if node_pubkey == pubkey.clone() {
			return Ok(());
		}
	}
	let res = do_connect_peer(pubkey.clone(), peer_addr, peer_manager.clone()).await;
	if res.is_err() {
		println!("ERROR: failed to connect to peer");
	}

	let peer_manager_clone = peer_manager.clone();
	let pubkey_clone = pubkey.clone();
	let peer_addr_clone = peer_addr.clone();
	tokio::spawn(async move {
		let mut interval = tokio::time::interval(Duration::from_secs(10));
		loop {
			interval.tick().await;
			let connected_node_ids = peer_manager_clone.get_peer_node_ids();
			if !connected_node_ids.contains(&pubkey_clone) {
				let res =
					do_connect_peer(pubkey_clone, peer_addr_clone, peer_manager.clone()).await;
				if res.is_err() {
					println!("ERROR: failed to connect to peer");
				}
			}
		}
	});

	res
}

pub(crate) async fn do_connect_peer(
	pubkey: PublicKey, peer_addr: SocketAddr, peer_manager: Arc<PeerManager>,
) -> Result<(), ()> {
	match lightning_net_tokio::connect_outbound(Arc::clone(&peer_manager), pubkey, peer_addr).await
	{
		Some(connection_closed_future) => {
			let mut connection_closed_future = Box::pin(connection_closed_future);
			loop {
				match futures::poll!(&mut connection_closed_future) {
					std::task::Poll::Ready(_) => {
						return Err(());
					}
					std::task::Poll::Pending => {}
				}
				// Avoid blocking the tokio context by sleeping a bit
				match peer_manager.get_peer_node_ids().iter().find(|id| **id == pubkey) {
					Some(_) => return Ok(()),
					None => tokio::time::sleep(Duration::from_millis(10)).await,
				}
			}
		}
		None => Err(()),
	}
}

fn open_channel(
	peer_pubkey: PublicKey, channel_amt_sat: u64, announced_channel: bool,
	channel_manager: Arc<ChannelManager>,
) -> Result<(), ()> {
	let config = UserConfig {
		channel_handshake_config: ChannelHandshakeConfig {
			announced_channel,
			..Default::default()
		},
		channel_handshake_limits: ChannelHandshakeLimits {
			// lnd's max to_self_delay is 2016, so we want to be compatible.
			their_to_self_delay: 2016,
			..Default::default()
		},
		..Default::default()
	};

	match channel_manager.create_channel(peer_pubkey, channel_amt_sat, 0, 0, Some(config)) {
		Ok(_) => {
			println!("EVENT: initiated channel with peer {}. ", peer_pubkey);
			return Ok(());
		}
		Err(e) => {
			println!("ERROR: failed to open channel: {:?}", e);
			return Err(());
		}
	}
}

fn send_payment<E: EventHandler>(
	invoice_payer: &InvoicePayer<E>, invoice: &Invoice, payment_storage: PaymentInfoStorage,
) {
	let status = match invoice_payer.pay_invoice(invoice) {
		Ok(_payment_id) => {
			let payee_pubkey = invoice.recover_payee_pub_key();
			let amt_msat = invoice.amount_milli_satoshis().unwrap();
			println!("EVENT: initiated sending {} msats to {}", amt_msat, payee_pubkey);
			print!("> ");
			HTLCStatus::Pending
		}
		Err(PaymentError::Invoice(e)) => {
			println!("ERROR: invalid invoice: {}", e);
			print!("> ");
			return;
		}
		Err(PaymentError::Routing(e)) => {
			println!("ERROR: failed to find route: {}", e.err);
			print!("> ");
			return;
		}
		Err(PaymentError::Sending(e)) => {
			println!("ERROR: failed to send payment: {:?}", e);
			print!("> ");
			HTLCStatus::Failed
		}
	};
	let payment_hash = PaymentHash(invoice.payment_hash().clone().into_inner());
	let payment_secret = Some(invoice.payment_secret().clone());

	let mut payments = payment_storage.lock().unwrap();
	payments.insert(
		payment_hash,
		PaymentInfo {
			preimage: None,
			secret: payment_secret,
			status,
			amt_msat: MillisatAmount(invoice.amount_milli_satoshis()),
		},
	);
}

fn keysend<E: EventHandler, K: KeysInterface>(
	invoice_payer: &InvoicePayer<E>, payee_pubkey: PublicKey, amt_msat: u64, keys: &K,
	payment_storage: PaymentInfoStorage,
) {
	let payment_preimage = keys.get_secure_random_bytes();

	let status = match invoice_payer.pay_pubkey(
		payee_pubkey,
		PaymentPreimage(payment_preimage),
		amt_msat,
		40,
	) {
		Ok(_payment_id) => {
			println!("EVENT: initiated sending {} msats to {}", amt_msat, payee_pubkey);
			print!("> ");
			HTLCStatus::Pending
		}
		Err(PaymentError::Invoice(e)) => {
			println!("ERROR: invalid payee: {}", e);
			print!("> ");
			return;
		}
		Err(PaymentError::Routing(e)) => {
			println!("ERROR: failed to find route: {}", e.err);
			print!("> ");
			return;
		}
		Err(PaymentError::Sending(e)) => {
			println!("ERROR: failed to send payment: {:?}", e);
			print!("> ");
			HTLCStatus::Failed
		}
	};

	let mut payments = payment_storage.lock().unwrap();
	payments.insert(
		PaymentHash(Sha256::hash(&payment_preimage).into_inner()),
		PaymentInfo {
			preimage: None,
			secret: None,
			status,
			amt_msat: MillisatAmount(Some(amt_msat)),
		},
	);
}

fn get_invoice(
	amt_msat: u64, payment_storage: PaymentInfoStorage, channel_manager: Arc<ChannelManager>,
	keys_manager: Arc<KeysManager>, network: Network, expiry_secs: u32,
) {
	let mut payments = payment_storage.lock().unwrap();
	let currency = match network {
		Network::Bitcoin => Currency::Bitcoin,
		Network::Testnet => Currency::BitcoinTestnet,
		Network::Regtest => Currency::Regtest,
		Network::Signet => Currency::Signet,
	};
	let invoice = match utils::create_invoice_from_channelmanager(
		&channel_manager,
		keys_manager,
		currency,
		Some(amt_msat),
		"ldk-tutorial-node".to_string(),
		expiry_secs,
	) {
		Ok(inv) => {
			println!("SUCCESS: generated invoice: {}", inv);
			inv
		}
		Err(e) => {
			println!("ERROR: failed to create invoice: {:?}", e);
			return;
		}
	};

	let payment_hash = PaymentHash(invoice.payment_hash().clone().into_inner());
	payments.insert(
		payment_hash,
		PaymentInfo {
			preimage: None,
			secret: Some(invoice.payment_secret().clone()),
			status: HTLCStatus::Pending,
			amt_msat: MillisatAmount(Some(amt_msat)),
		},
	);
}

fn close_channel(
	channel_id: [u8; 32], counterparty_node_id: PublicKey, channel_manager: Arc<ChannelManager>,
) {
	match channel_manager.close_channel(&channel_id, &counterparty_node_id) {
		Ok(()) => println!("EVENT: initiating channel close"),
		Err(e) => println!("ERROR: failed to close channel: {:?}", e),
	}
}

fn force_close_channel(
	channel_id: [u8; 32], counterparty_node_id: PublicKey, channel_manager: Arc<ChannelManager>,
) {
	match channel_manager.force_close_broadcasting_latest_txn(&channel_id, &counterparty_node_id) {
		Ok(()) => println!("EVENT: initiating channel force-close"),
		Err(e) => println!("ERROR: failed to force-close channel: {:?}", e),
	}
}

pub(crate) fn parse_peer_info(
	peer_pubkey_and_ip_addr: String,
) -> Result<(PublicKey, SocketAddr), std::io::Error> {
	let mut pubkey_and_addr = peer_pubkey_and_ip_addr.split("@");
	let pubkey = pubkey_and_addr.next();
	let peer_addr_str = pubkey_and_addr.next();
	if peer_addr_str.is_none() || peer_addr_str.is_none() {
		return Err(std::io::Error::new(
			std::io::ErrorKind::Other,
			"ERROR: incorrectly formatted peer info. Should be formatted as: `pubkey@host:port`",
		));
	}

	let peer_addr = peer_addr_str.unwrap().to_socket_addrs().map(|mut r| r.next());
	if peer_addr.is_err() || peer_addr.as_ref().unwrap().is_none() {
		return Err(std::io::Error::new(
			std::io::ErrorKind::Other,
			"ERROR: couldn't parse pubkey@host:port into a socket address",
		));
	}

	let pubkey = hex_utils::to_compressed_pubkey(pubkey.unwrap());
	if pubkey.is_none() {
		return Err(std::io::Error::new(
			std::io::ErrorKind::Other,
			"ERROR: unable to parse given pubkey for node",
		));
	}

	Ok((pubkey.unwrap(), peer_addr.unwrap().unwrap()))
}

fn get_attempts_found(conn: &Connection) -> Result<Vec<Attempt>, Box<dyn std::error::Error>> {
	let mut stmt = conn.prepare("SELECT * FROM attempt")?;
	let mut rows = stmt.query([])?;

	let mut attempts = vec![];
	while let Some(row) = rows.next()? {
		let attempt = Attempt {
			target_pubkey: row.get(0)?,
			guess_pubkey: row.get(1)?,
			channel_id: row.get(2)?,
			result: row.get(3)?,
			date_found: row.get(4)?,
		};

		if attempt.result == "unknown" || attempt.result == "unknown_next_peer" {
			continue;
		}

		attempts.push(attempt);
	}

	Ok(attempts)
}

fn get_attempts_str(
	conn: &Connection,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
	let mut stmt = conn.prepare("SELECT * FROM attempt")?;
	let mut rows = stmt.query([])?;

	let mut attempts = HashMap::new();
	while let Some(row) = rows.next()? {
		let attempt = Attempt {
			target_pubkey: row.get(0)?,
			guess_pubkey: row.get(1)?,
			channel_id: row.get(2)?,
			result: row.get(3)?,
			date_found: row.get(4)?,
		};

		if attempt.result == "unknown" {
			continue;
		}

		let attempt_str = format!("{}:{}", attempt.target_pubkey, attempt.channel_id);
		attempts.insert(attempt_str, attempt.result);
	}

	Ok(attempts)
}
