use std::{fs::File, io::BufWriter};

use anyhow::Result;
use reqwest;
use serde::{Deserialize, Serialize};

const SEGWIT_START_HEIGHT: usize = 477120;

async fn get_text(query: &str) -> Result<String> {
	let text =
		reqwest::get(format!("https://blockstream.info/api/{}", query)).await?.text().await?;

	return Ok(text);
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Tx {
	vout: Vec<Vout>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TxOutspent {
	spent: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Vout {
	scriptpubkey_type: String,
	value: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ScrapeResult {
	block_hash: String,
	block_height: usize,
	id: String,
	block_index: usize,
	transaction_index: usize,
	amount: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
	let tip_height = get_text("blocks/tip/height").await?.parse::<usize>()?;

	println!("{:#?}", tip_height);

	// for height in STARTING_HEIGHT..tip_height {
	for block_height in (SEGWIT_START_HEIGHT..tip_height).rev() {
		println!("=====");
		println!("Block height: {}", block_height);
		let block_hash = get_text(&format!("block-height/{}", block_height)).await?;

		let transactions_text = get_text(&format!("block/{}/txids", block_hash.clone())).await?;

		let writer = BufWriter::new(
			File::create(format!("./data/transactions/{}.json", block_height)).unwrap(),
		);

		let transactions: Vec<String> = serde_json::from_str(&transactions_text)?;

		let mut scrape_results: Vec<ScrapeResult> = vec![];

		let mut num_results = 0;

		for (y, tx) in transactions.iter().enumerate() {
			let transaction_details_text = get_text(&format!("tx/{}", tx)).await?;

			let tx_details: Tx = serde_json::from_str(&transaction_details_text)?;

			for (z, vout) in tx_details.vout.iter().enumerate() {
				if vout.scriptpubkey_type != "v0_p2wsh" {
					continue;
				}

				let tx_vout_status_text = get_text(&format!("tx/{}/outspend/{}", tx, z)).await?;
				let tx_outspent: TxOutspent = serde_json::from_str(&tx_vout_status_text)?;

				if tx_outspent.spent {
					continue;
				}

				scrape_results.push(ScrapeResult {
					block_hash: block_hash.clone(),
					block_height,
					id: tx.into(),
					block_index: y,
					transaction_index: z,
					amount: vout.value,
				});

				println!("{}:{}", tx, z);

				num_results += 1;
			}
		}

		serde_json::to_writer_pretty(writer, &scrape_results).unwrap();
		println!("Wrote {} transactions for block {}", num_results, block_height);
	}

	Ok(())
}
