use crate::cli;
use crate::ChannelManager;
use crate::NetworkGraph;
use bitcoin::hash_types::Txid;
use bitcoin::hashes::hex::{FromHex, ToHex};
use bitcoin::secp256k1::PublicKey;
use bitcoin::BlockHash;
use chrono::Utc;
use lightning::chain::chainmonitor;
use std::io::Cursor;
extern crate libc;

use lightning::chain;
use lightning::chain::channelmonitor::{ChannelMonitor, ChannelMonitorUpdate};
use lightning::chain::keysinterface::Sign;
use lightning::chain::transaction::OutPoint;
// use lightning::routing::network_graph::NetworkGraph;
use lightning::routing::scoring::{ProbabilisticScorer, ProbabilisticScoringParameters};
use lightning::util::logger::{Logger, Record};
use lightning::util::ser::{Readable, ReadableArgs, Writeable, Writer};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Error;
use std::io::{BufRead, BufReader, BufWriter};
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

pub struct YourPersister {
	path_to_channel_data: String,
	chan_update_cache: RwLock<HashMap<OutPoint, Vec<Vec<u8>>>>,
}

impl<Signer: Sign> DiskWriteable for ChannelMonitor<Signer> {
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), Error> {
		self.write(writer)
	}
	fn write_to_memory<W: Writer>(&self, writer: &mut W) -> Result<(), Error> {
		self.write(writer)
	}
}

impl DiskWriteable for ChannelManager {
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error> {
		self.write(writer)
	}
	fn write_to_memory<W: Writer>(&self, writer: &mut W) -> Result<(), Error> {
		self.write(writer)
	}
}

impl DiskWriteable for ChannelMonitorUpdate {
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
		return Self { path_to_channel_data, chan_update_cache: RwLock::new(HashMap::new()) };
	}

	pub(crate) fn path_to_monitor_data(&self) -> PathBuf {
		let mut path = PathBuf::from(self.path_to_channel_data.clone());
		path.push("monitors");
		path
	}
	pub(crate) fn path_to_monitor_data_updates(&self) -> PathBuf {
		let mut path = PathBuf::from(self.path_to_channel_data.clone());
		path.push("updates");
		path
	}

	/// Read `ChannelMonitor` updates from disk.
	pub fn read_channelmonitor_updates(
		&self,
	) -> Result<HashMap<Txid, Vec<ChannelMonitorUpdate>>, std::io::Error> {
		let mut tx_id_channel_map = HashMap::new();
		let path = self.path_to_monitor_data_updates();
		if !Path::new(&path).exists() {
			return Ok(tx_id_channel_map);
		}
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

			let filename_str = filename.unwrap();
			let filename_vec: Vec<&str> = filename_str.split('_').collect();
			let txid = Txid::from_hex(filename_vec[0]);
			if txid.is_err() {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					format!("Invalid tx ID in filename {}", filename_vec[0]),
				));
			}

			let index: Result<u16, std::num::ParseIntError> = filename_vec[1].parse();
			if index.is_err() {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					format!("Invalid tx index in filename {}", filename_vec[1]),
				));
			}

			let contents = fs::read(&file.path())?;
			let mut buffer = Cursor::new(&contents);
			match <ChannelMonitorUpdate>::read(&mut buffer) {
				Ok(channel_monitor_update) => {
					// see if we already have this key
					match tx_id_channel_map.get_mut(&txid.unwrap()) {
						Some(map) => map.push(channel_monitor_update),
						None => {
							tx_id_channel_map.insert(txid.unwrap(), vec![channel_monitor_update]);
						}
					}
				}
				Err(e) => {
					return Err(std::io::Error::new(
						std::io::ErrorKind::InvalidData,
						format!("Failed to deserialize ChannelMonitorUpdate: {}", e),
					))
				}
			}
		}
		Ok(tx_id_channel_map)
	}
}

impl<ChannelSigner: Sign> chainmonitor::Persist<ChannelSigner> for YourPersister {
	fn persist_new_channel(
		&self, funding_txo: OutPoint, monitor: &ChannelMonitor<ChannelSigner>,
		_update_id: chainmonitor::MonitorUpdateId,
	) -> Result<(), chain::ChannelMonitorUpdateErr> {
		let filename = format!("{}_{}", funding_txo.txid.to_hex(), funding_txo.index);
		let write_res = write_to_file(self.path_to_monitor_data(), filename, monitor)
			.map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure);
		if write_res.is_err() {
			return write_res;
		}
		// anytime monitor data is written, delete the update dir
		fs::create_dir_all(self.path_to_monitor_data_updates().clone()).unwrap();
		fs::remove_dir_all(self.path_to_monitor_data_updates()).unwrap();
		fs::create_dir(self.path_to_monitor_data_updates()).unwrap();
		Ok(())
	}

	fn update_persisted_channel(
		&self, id: OutPoint, update: &Option<ChannelMonitorUpdate>,
		data: &ChannelMonitor<ChannelSigner>, _update_id: chainmonitor::MonitorUpdateId,
	) -> Result<(), chain::ChannelMonitorUpdateErr> {
		if update.is_some() {
			fs::create_dir_all(self.path_to_monitor_data_updates().clone()).unwrap();
			let filename =
				format!("{}_{}_{}", id.txid.to_hex(), id.index, update.clone().unwrap().update_id);
			write_to_file(self.path_to_monitor_data_updates(), filename, &update.clone().unwrap())
				.map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)
				.unwrap();
		} else {
			// save the entire manager for block related updates
			let filename = format!("{}_{}", id.txid.to_hex(), id.index);
			write_to_file(self.path_to_monitor_data(), filename, data)
				.map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)
				.unwrap();

			// then delete the updates file since manager includes them
			self.chan_update_cache.write().unwrap().remove(&id);

			// also delete the update dir

			fs::create_dir_all(self.path_to_monitor_data_updates().clone()).unwrap();
			fs::remove_dir_all(self.path_to_monitor_data_updates()).unwrap();
			fs::create_dir(self.path_to_monitor_data_updates()).unwrap();
		}
		Ok(())
	}
}

#[allow(bare_trait_objects)]
pub(crate) fn write_to_file<D: DiskWriteable>(
	path: PathBuf, filename: String, data: &D,
) -> std::io::Result<()> {
	// let now = Instant::now();

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
		let path = Path::new(&filename_with_path).parent().unwrap();
		let dir_file = fs::OpenOptions::new().read(true).open(path)?;
		unsafe {
			libc::fsync(dir_file.as_raw_fd());
		}
	}

	// 	println!("Writing {} took {}s", filename_with_path, now.elapsed().as_secs_f64());
	Ok(())
}

pub(crate) trait DiskWriteable {
	fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error>;
	fn write_to_memory<W: Writer>(&self, writer: &mut W) -> Result<(), std::io::Error>;
}

pub(crate) fn get_full_filepath(mut filepath: PathBuf, filename: String) -> String {
	filepath.push(filename);
	filepath.to_str().unwrap().to_string()
}

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
	// let now = Instant::now();
	let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
	std::io::Write::write_all(&mut file, format!("{}\n", peer_info).as_bytes()).unwrap();
	// println!("Writing channel_peer took {}s", now.elapsed().as_secs_f64());
	Ok(())
}

pub(crate) fn read_channel_peer_data(
	path: &Path,
) -> Result<HashMap<PublicKey, SocketAddr>, std::io::Error> {
	// let now = Instant::now();
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
	// println!("Reading {:?} took {}s", path, now.elapsed().as_secs_f64());
	Ok(peer_data)
}

pub(crate) fn persist_network(path: &Path, network_graph: &NetworkGraph) -> std::io::Result<()> {
	// let now = Instant::now();
	let mut tmp_path = path.to_path_buf().into_os_string();
	tmp_path.push(".tmp");
	let file = fs::OpenOptions::new().write(true).create(true).open(&tmp_path)?;
	let write_res = network_graph.write(&mut BufWriter::new(file));
	if let Err(e) = write_res.and_then(|_| fs::rename(&tmp_path, path)) {
		let _ = fs::remove_file(&tmp_path);
		Err(e)
	} else {
		// println!("Writing network took {}s", now.elapsed().as_secs_f64());
		Ok(())
	}
}

pub(crate) fn read_network(
	path: &Path, genesis_hash: BlockHash, logger: Arc<FilesystemLogger>,
) -> NetworkGraph {
	if let Ok(file) = File::open(path) {
		if let Ok(graph) = NetworkGraph::read(&mut BufReader::new(file), logger.clone()) {
			// println!("Reading {:?} took {}s", path, now.elapsed().as_secs_f64());
			return graph;
		}
	}
	NetworkGraph::new(genesis_hash, logger)
}

pub(crate) fn persist_scorer(
	path: &Path, scorer: &ProbabilisticScorer<Arc<NetworkGraph>, Arc<FilesystemLogger>>,
) -> std::io::Result<()> {
	// let now = Instant::now();
	let mut tmp_path = path.to_path_buf().into_os_string();
	tmp_path.push(".tmp");
	let file = fs::OpenOptions::new().write(true).create(true).open(&tmp_path)?;
	let write_res = scorer.write(&mut BufWriter::new(file));
	if let Err(e) = write_res.and_then(|_| fs::rename(&tmp_path, path)) {
		let _ = fs::remove_file(&tmp_path);
		Err(e)
	} else {
		// println!("Writing scorer took {}s", now.elapsed().as_secs_f64());
		Ok(())
	}
}

pub(crate) fn read_scorer(
	path: &Path, graph: Arc<NetworkGraph>, logger: Arc<FilesystemLogger>,
) -> ProbabilisticScorer<Arc<NetworkGraph>, Arc<FilesystemLogger>> {
	let params = ProbabilisticScoringParameters::default();
	if let Ok(file) = File::open(path) {
		let args = (params.clone(), Arc::clone(&graph), Arc::clone(&logger));
		if let Ok(scorer) = ProbabilisticScorer::read(&mut BufReader::new(file), args) {
			return scorer;
		}
	}
	ProbabilisticScorer::new(params, graph, logger)
}
