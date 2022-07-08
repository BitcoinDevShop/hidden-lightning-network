// Imports that need to be added manually
use lightning::util::logger::Logger;
use lightning_rapid_gossip_sync::RapidGossipSync;

use utils::test_logger;

use std::sync::Arc;

/// Actual fuzz test, method signature and name are fixed
fn do_test<Out: test_logger::Output>(data: &[u8], out: Out) {
	let block_hash = bitcoin::BlockHash::default();
	let logger = test_logger::TestLogger::new("".to_owned(), out);
	let network_graph = lightning::routing::gossip::NetworkGraph::new(block_hash, &logger);
	let rapid_sync = RapidGossipSync::new(&network_graph);
	let _ = rapid_sync.update_network_graph(data);
}

/// Method that needs to be added manually, {name}_test
pub fn process_network_graph_test<Out: test_logger::Output>(data: &[u8], out: Out) {
	do_test(data, out);
}

/// Method that needs to be added manually, {name}_run
#[no_mangle]
pub extern "C" fn process_network_graph_run(data: *const u8, datalen: usize) {
	do_test(unsafe { std::slice::from_raw_parts(data, datalen) }, test_logger::DevNull {});
}
