use crate::cli;
use crate::ChannelManager;
use bitcoin::hash_types::{BlockHash, Txid};
use bitcoin::hashes::hex::{FromHex, ToHex};
use bitcoin::secp256k1::key::PublicKey;
use lightning::chain::chainmonitor::{self, Persist};
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
// use bitcoin::BlockHash;
use chrono::Utc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use lightning::chain;
use lightning::chain::channelmonitor::{ChannelMonitor, ChannelMonitorUpdate};
use lightning::chain::keysinterface::{KeysInterface, Sign};
use lightning::chain::transaction::OutPoint;
use lightning::routing::network_graph::NetworkGraph;
use lightning::routing::scoring::{ProbabilisticScorer, ProbabilisticScoringParameters};
use lightning::util::logger::{Logger, Record};
use lightning::util::ser::{Readable, ReadableArgs, Writeable, Writer};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Error;
use std::io::{BufRead, BufReader, BufWriter};
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard};
use std::time::Instant;

/*
pub struct YourPersister<ChannelSigner: Sign> {
	path_to_channel_data: String,
	//chan_manager_cache: RwLock<HashMap<OutPoint, MonitorHolder<ChannelSigner>>>,
	//chan_manager_cache: RwLock<HashMap<OutPoint, ChannelMonitor<ChannelSigner>>>,
	chan_manager_cache: RwLock<HashMap<OutPoint, BufWriter<W>>>>,
}
*/
pub struct YourPersister {
	path_to_channel_data: String,
	//chan_manager_cache: RwLock<HashMap<OutPoint, MonitorHolder<ChannelSigner>>>,
	//chan_manager_cache: RwLock<HashMap<OutPoint, ChannelMonitor<ChannelSigner>>>,
	chan_manager_cache: RwLock<HashMap<OutPoint, Vec<u8>>>,
}

struct MonitorHolder<ChannelSigner: Sign> {
	monitor: ChannelMonitor<ChannelSigner>,
	/// The full set of pending monitor updates for this Channel.
	///
	/// Note that this lock must be held during updates to prevent a race where we call
	/// update_persisted_channel, the user returns a TemporaryFailure, and then calls
	/// channel_monitor_updated immediately, racing our insertion of the pending update into the
	/// contained Vec.
	///
	/// Beyond the synchronization of updates themselves, we cannot handle user events until after
	/// any chain updates have been stored on disk. Thus, we scan this list when returning updates
	/// to the ChannelManager, refusing to return any updates for a ChannelMonitor which is still
	/// being persisted fully to disk after a chain update.
	///
	/// This avoids the possibility of handling, e.g. an on-chain claim, generating a claim monitor
	/// event, resulting in the relevant ChannelManager generating a PaymentSent event and dropping
	/// the pending payment entry, and then reloading before the monitor is persisted, resulting in
	/// the ChannelManager re-adding the same payment entry, before the same block is replayed,
	/// resulting in a duplicate PaymentSent event.
	pending_monitor_updates: Mutex<Vec<chainmonitor::MonitorUpdateId>>,
	/// When the user returns a PermanentFailure error from an update_persisted_channel call during
	/// block processing, we inform the ChannelManager that the channel should be closed
	/// asynchronously. In order to ensure no further changes happen before the ChannelManager has
	/// processed the closure event, we set this to true and return PermanentFailure for any other
	/// chain::Watch events.
	channel_perm_failed: AtomicBool,
	/// The last block height at which no [`UpdateOrigin::ChainSync`] monitor updates were present
	/// in `pending_monitor_updates`.
	/// If it's been more than [`LATENCY_GRACE_PERIOD_BLOCKS`] since we started waiting on a chain
	/// sync event, we let monitor events return to `ChannelManager` because we cannot hold them up
	/// forever or we'll end up with HTLC preimages waiting to feed back into an upstream channel
	/// forever, risking funds loss.
	last_chain_persist_height: AtomicUsize,
}

impl<ChannelSigner: Sign> MonitorHolder<ChannelSigner> {
	fn has_pending_offchain_updates(
		&self, pending_monitor_updates_lock: &MutexGuard<Vec<chainmonitor::MonitorUpdateId>>,
	) -> bool {
		pending_monitor_updates_lock.iter().any(|update_id| {
			if let chainmonitor::UpdateOrigin::OffChain(_) = update_id.contents {
				true
			} else {
				false
			}
		})
	}
	fn has_pending_chainsync_updates(
		&self, pending_monitor_updates_lock: &MutexGuard<Vec<chainmonitor::MonitorUpdateId>>,
	) -> bool {
		pending_monitor_updates_lock.iter().any(|update_id| {
			if let chainmonitor::UpdateOrigin::ChainSync(_) = update_id.contents {
				true
			} else {
				false
			}
		})
	}
}

impl<Signer: Sign> DiskWriteable for ChannelMonitor<Signer> {
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), Error> {
		self.write(writer)
	}
	fn write_to_memory<W: Writer>(&self, writer: &mut W) -> Result<(), Error> {
		self.write(writer)
	}
}

/*
impl<M: Deref, T: Deref, K: Deref, F: Deref, L: Deref> DiskWriteable for ChannelManager
where
	M::Target: chain::Watch<InMemorySigner>,
	T::Target: BroadcasterInterface,
	K::Target: KeysInterface<Signer = KeysManager>,
	F::Target: FeeEstimator,
	L::Target: Logger,
{
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error> {
		self.write(writer)
	}
}
*/

impl DiskWriteable for ChannelManager {
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error> {
		self.write(writer)
	}
	fn write_to_memory<W: Writer>(&self, writer: &mut W) -> Result<(), Error> {
		self.write(writer)
	}
}

impl YourPersister {
	/// Initialize a new FilesystemPersister and set the path to the individual channels'
	/// files.
	pub fn new(path_to_channel_data: String) -> Self {
		return Self { path_to_channel_data, chan_manager_cache: RwLock::new(HashMap::new()) };
	}

	/// Get the directory which was provided when this persister was initialized.
	pub fn get_data_dir(&self) -> String {
		self.path_to_channel_data.clone()
	}

	pub(crate) fn path_to_monitor_data(&self) -> PathBuf {
		let mut path = PathBuf::from(self.path_to_channel_data.clone());
		path.push("monitors");
		path
	}

	/*
	/// Writes the provided `ChannelManager` to the path provided at `FilesystemPersister`
	/// initialization, within a file called "manager".
	pub fn persist_manager<Signer: Sign, M: Deref, T: Deref, K: Deref, F: Deref, L: Deref>(
		data_dir: String, manager: &channelmanager::ChannelManager<Signer, M, T, K, F, L>,
	) -> Result<(), std::io::Error>
	where
		M::Target: chain::Watch<Signer>,
		T::Target: BroadcasterInterface,
		K::Target: KeysInterface<Signer = Signer>,
		F::Target: FeeEstimator,
		L::Target: Logger,
	{
		let path = PathBuf::from(data_dir);
		write_to_file(path, "manager".to_string(), manager)
	}
		*/

	/// Read `ChannelMonitor`s from disk.
	pub fn read_channelmonitors<Signer: Sign, K: Deref>(
		&self, keys_manager: K,
	) -> Result<Vec<(BlockHash, ChannelMonitor<Signer>)>, std::io::Error>
	where
		K::Target: KeysInterface<Signer = Signer> + Sized,
	{
		let path = self.path_to_monitor_data();
		if !Path::new(&path).exists() {
			return Ok(Vec::new());
		}
		let mut res = Vec::new();
		for file_option in fs::read_dir(path).unwrap() {
			let file = file_option.unwrap();
			let owned_file_name = file.file_name();
			let filename = owned_file_name.to_str();
			if !filename.is_some() || !filename.unwrap().is_ascii() || filename.unwrap().len() < 65
			{
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Invalid ChannelMonitor file name",
				));
			}
			if filename.unwrap().ends_with(".tmp") {
				// If we were in the middle of committing an new update and crashed, it should be
				// safe to ignore the update - we should never have returned to the caller and
				// irrevocably committed to the new state in any way.
				continue;
			}

			let txid = Txid::from_hex(filename.unwrap().split_at(64).0);
			if txid.is_err() {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Invalid tx ID in filename",
				));
			}

			let index: Result<u16, std::num::ParseIntError> =
				filename.unwrap().split_at(65).1.parse();
			if index.is_err() {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Invalid tx index in filename",
				));
			}

			let contents = fs::read(&file.path())?;
			let mut buffer = Cursor::new(&contents);
			match <(BlockHash, ChannelMonitor<Signer>)>::read(&mut buffer, &*keys_manager) {
				Ok((blockhash, channel_monitor)) => {
					if channel_monitor.get_funding_txo().0.txid != txid.unwrap()
						|| channel_monitor.get_funding_txo().0.index != index.unwrap()
					{
						return Err(std::io::Error::new(
							std::io::ErrorKind::InvalidData,
							"ChannelMonitor was stored in the wrong file",
						));
					}
					res.push((blockhash, channel_monitor));
				}
				Err(e) => {
					return Err(std::io::Error::new(
						std::io::ErrorKind::InvalidData,
						format!("Failed to deserialize ChannelMonitor: {}", e),
					))
				}
			}
		}
		Ok(res)
	}

	pub(crate) fn save_file(&self) -> Result<(), chain::ChannelMonitorUpdateErr> {
		println!("Going to save file results from cache...");
		for (k, v) in self.chan_manager_cache.read().unwrap().iter() {
			fs::create_dir_all(self.path_to_monitor_data().clone()).unwrap();

			let filename = format!("{}_{}", k.txid.to_hex(), k.index);
			println!("Going to save for file {}", filename);

			fs::create_dir_all(self.path_to_monitor_data().clone()).unwrap();
			let filename_with_path = get_full_filepath(self.path_to_monitor_data(), filename);

			let mut file =
				OpenOptions::new().create(true).write(true).open(filename_with_path).unwrap();

			std::io::Write::write_all(&mut file, &v);
		}
		Ok(())
	}
}

impl<ChannelSigner: Sign> chainmonitor::Persist<ChannelSigner> for YourPersister {
	fn persist_new_channel(
		&self, funding_txo: OutPoint, monitor: &ChannelMonitor<ChannelSigner>,
		_update_id: chainmonitor::MonitorUpdateId,
	) -> Result<(), chain::ChannelMonitorUpdateErr> {
		/*
		let filename = format!("{}_{}", funding_txo.txid.to_hex(), funding_txo.index);
		write_to_file(self.path_to_monitor_data(), filename, monitor)
			.map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)
			*/
		let data_vec = monitor.encode();
		self.chan_manager_cache.write().unwrap().insert(funding_txo, data_vec);
		Ok(())
	}

	fn update_persisted_channel(
		&self, id: OutPoint, update: &Option<ChannelMonitorUpdate>,
		data: &ChannelMonitor<ChannelSigner>, update_id: chainmonitor::MonitorUpdateId,
	) -> Result<(), chain::ChannelMonitorUpdateErr> {
		/*
		let mut c = Cursor::new(Vec::new());
		let mut c = BufWriter::new(Vec::new());
		data.write_to_memory(&mut c).map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)
			*/
		let data_vec = data.encode();
		self.chan_manager_cache.write().unwrap().insert(id, data_vec);
		//println!("Added to chan manager cache");
		Ok(())
	}

	fn save_file(&self) -> Result<(), chain::ChannelMonitorUpdateErr> {
		println!("Going to save file results...");
		/*
		let mut c = BufWriter::new(Vec::new());
		write_to_file(self.path_to_monitor_data(), filename, v).unwrap();
		*/
		for (k, v) in self.chan_manager_cache.read().unwrap().iter() {
			fs::create_dir_all(self.path_to_monitor_data().clone()).unwrap();

			let filename = format!("{}_{}", k.txid.to_hex(), k.index);
			println!("Going to save for file {}", filename);

			fs::create_dir_all(self.path_to_monitor_data().clone()).unwrap();
			let filename_with_path = get_full_filepath(self.path_to_monitor_data(), filename);

			let mut file =
				OpenOptions::new().create(true).write(true).open(filename_with_path).unwrap();

			std::io::Write::write_all(&mut file, &v);
		}
		Ok(())
	}
}

pub(crate) trait DiskWriteable {
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error>;
	fn write_to_memory<W: Writer>(&self, writer: &mut W) -> Result<(), std::io::Error>;
}

pub(crate) fn get_full_filepath(mut filepath: PathBuf, filename: String) -> String {
	filepath.push(filename);
	filepath.to_str().unwrap().to_string()
}

/*
#[allow(bare_trait_objects)]
pub(crate) fn write_to_file<D: DiskWriteable>(
	path: PathBuf, filename: String, data: &D,
) -> std::io::Result<()> {
	let now = Instant::now();

	fs::create_dir_all(path.clone())?;
	// Do a crazy dance with lots of fsync()s to be overly cautious here...
	// We never want to end up in a state where we've lost the old data, or end up using the
	// old data on power loss after we've returned.
	// The way to atomically write a file on Unix platforms is:
	// open(tmpname), write(tmpfile), fsync(tmpfile), close(tmpfile), rename(), fsync(dir)
	let filename_with_path = get_full_filepath(path, filename);
	let tmp_filename = format!("{}.tmp", filename_with_path.clone());

	{
		// Note that going by rust-lang/rust@d602a6b, on MacOS it is only safe to use
		// rust stdlib 1.36 or higher.
		let mut f = fs::File::create(&tmp_filename)?;
		data.write_to_file(&mut f)?;
		f.sync_all()?;
	}
	// Fsync the parent directory on Unix.
	#[cfg(not(target_os = "windows"))]
	{
		fs::rename(&tmp_filename, &filename_with_path)?;
		/*
		let path = Path::new(&filename_with_path).parent().unwrap();
		let dir_file = fs::OpenOptions::new().read(true).open(path)?;
		unsafe {
			libc::fsync(dir_file.as_raw_fd());
		}
				*/
	}

	println!("Writing {} took {}s", filename_with_path, now.elapsed().as_secs_f64());
	Ok(())
}
*/

pub(crate) struct FilesystemLogger {
	data_dir: String,
}
impl FilesystemLogger {
	pub(crate) fn new(data_dir: String) -> Self {
		let logs_path = format!("{}/logs", data_dir);
		fs::create_dir_all(logs_path.clone()).unwrap();
		Self { data_dir: logs_path }
	}
}
impl Logger for FilesystemLogger {
	fn log(&self, record: &Record) {
		if record.level.to_string() != "INFO" && record.level.to_string() != "WARN" {
			return;
		}
		let raw_log = record.args.to_string();
		let log = format!(
			"{} {:<5} [{}:{}] {}\n",
			// Note that a "real" lightning node almost certainly does *not* want subsecond
			// precision for message-receipt information as it makes log entries a target for
			// deanonymization attacks. For testing, however, its quite useful.
			Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"),
			record.level.to_string(),
			record.module_path,
			record.line,
			raw_log
		);
		let logs_file_path = format!("{}/logs.txt", self.data_dir.clone());
		std::io::Write::write_all(
			&mut fs::OpenOptions::new().create(true).append(true).open(logs_file_path).unwrap(),
			log.as_bytes(),
		)
		.unwrap();
	}
}
pub(crate) fn persist_channel_peer(path: &Path, peer_info: &str) -> std::io::Result<()> {
	let now = Instant::now();
	let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
	std::io::Write::write_all(&mut file, format!("{}\n", peer_info).as_bytes()).unwrap();
	println!("Writing channel_peer took {}s", now.elapsed().as_secs_f64());
	Ok(())
}

pub(crate) fn read_channel_peer_data(
	path: &Path,
) -> Result<HashMap<PublicKey, SocketAddr>, std::io::Error> {
	let now = Instant::now();
	let mut peer_data = HashMap::new();
	if !Path::new(&path).exists() {
		return Ok(HashMap::new());
	}
	let file = File::open(path)?;
	let reader = BufReader::new(file);
	for line in reader.lines() {
		match cli::parse_peer_info(line.unwrap()) {
			Ok((pubkey, socket_addr)) => {
				peer_data.insert(pubkey, socket_addr);
			}
			Err(e) => return Err(e),
		}
	}
	println!("Reading {:?} took {}s", path, now.elapsed().as_secs_f64());
	Ok(peer_data)
}

pub(crate) fn persist_network(path: &Path, network_graph: &NetworkGraph) -> std::io::Result<()> {
	let now = Instant::now();
	let mut tmp_path = path.to_path_buf().into_os_string();
	tmp_path.push(".tmp");
	let file = fs::OpenOptions::new().write(true).create(true).open(&tmp_path)?;
	let write_res = network_graph.write(&mut BufWriter::new(file));
	if let Err(e) = write_res.and_then(|_| fs::rename(&tmp_path, path)) {
		let _ = fs::remove_file(&tmp_path);
		Err(e)
	} else {
		println!("Writing network took {}s", now.elapsed().as_secs_f64());
		Ok(())
	}
}

pub(crate) fn read_network(path: &Path, genesis_hash: BlockHash) -> NetworkGraph {
	let now = Instant::now();
	if let Ok(file) = File::open(path) {
		if let Ok(graph) = NetworkGraph::read(&mut BufReader::new(file)) {
			println!("Reading {:?} took {}s", path, now.elapsed().as_secs_f64());
			return graph;
		}
	}
	NetworkGraph::new(genesis_hash)
}

pub(crate) fn persist_scorer(
	path: &Path, scorer: &ProbabilisticScorer<Arc<NetworkGraph>>,
) -> std::io::Result<()> {
	let now = Instant::now();
	let mut tmp_path = path.to_path_buf().into_os_string();
	tmp_path.push(".tmp");
	let file = fs::OpenOptions::new().write(true).create(true).open(&tmp_path)?;
	let write_res = scorer.write(&mut BufWriter::new(file));
	if let Err(e) = write_res.and_then(|_| fs::rename(&tmp_path, path)) {
		let _ = fs::remove_file(&tmp_path);
		Err(e)
	} else {
		println!("Writing scorer took {}s", now.elapsed().as_secs_f64());
		Ok(())
	}
}

pub(crate) fn read_scorer(
	path: &Path, graph: Arc<NetworkGraph>,
) -> ProbabilisticScorer<Arc<NetworkGraph>> {
	let now = Instant::now();
	let params = ProbabilisticScoringParameters::default();
	if let Ok(file) = File::open(path) {
		if let Ok(scorer) =
			ProbabilisticScorer::read(&mut BufReader::new(file), (params, Arc::clone(&graph)))
		{
			println!("Reading {:?} took {}s", path, now.elapsed().as_secs_f64());
			return scorer;
		}
	}
	ProbabilisticScorer::new(params, graph)
}
