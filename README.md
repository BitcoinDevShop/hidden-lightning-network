# hidden-lightning-network

Use LDK to probe the lightning network for the detection of private channels.

## A vague graphical explanation

<img width="594" alt="Screen Shot 2022-03-21 at 6 05 18 PM" src="https://user-images.githubusercontent.com/543668/159377381-b325476d-7380-432d-afb5-bd1a40e3ef10.png">

## Reason

We can look at public lightning node stats at places like 1ml.com. But "private channels are private". Except they are not, and we can figure it out.

## How

Create our own lightning paths and guess channel IDs. If we guess correct, we know a node has a certain private channel based on error code. Then, we can even guess which node is on the other side of the path.

### Hackathon results:

- Used LDK sample node as a starting point
- Connected it to our LND based polar instance
- Queried Routes to the destination target
- Figured out how to make fake lightning payments through a route
- Appended our guess route to the end of the path
- Sent our fake payment, interpreted the results
- Saved payment attempt state, can infer if our fake payment revealed a private channel

### Working notes:

```
cargo run <bitcoind-rpc-username>:<bitcoind-rpc-password>@<bitcoind-rpc-host>:<bitcoind-rpc-port> <ldk_storage_directory_path> [<ldk-peer-listening-port>] [bitcoin-network] [announced-listen-addr announced-node-name]
```

```
source .env.local && cargo run $RPC_USER:$RPC_PASS@$RPC_HOST $STORAGE $LISTEN $NETWORK $NAME $ADDR
```

#### For parsing raw utxo transaction files 

```
cargo run --bin scraper ./data/utxodump.csv ./data/iterate.txt
```

## Tony's notes

```
ALICE p2p
connectpeer 0258cc6d50ed15d05938261e209e5bee11948eda838d66b5481b3d6e839733cf87@127.0.0.1:9735

bob pubkey
sendfakepayment 0231014817072d627ef0772b5212e73a8f32190e1bad485418938e093f0f479768

correct channel id, correct pubkey
probeprivate 0258cc6d50ed15d05938261e209e5bee11948eda838d66b5481b3d6e839733cf87 03aa4f7f215d551f3bd6e852122d85d0da6b34753ebe03a94b2b7fc092694c6ff5 645413325570048

wrong pubkey, correct channel id
probeprivate 0231014817072d627ef0772b5212e73a8f32190e1bad485418938e093f0f479768 030ac3e942e8407243c62423c7f0d68787ff112b7831c9cd2c7c1639c781591d94 645413325570048

wrong channel id, correct pubkey
probeprivate 0231014817072d627ef0772b5212e73a8f32190e1bad485418938e093f0f479768 03aa4f7f215d551f3bd6e852122d85d0da6b34753ebe03a94b2b7fc092694c6ff5 158329674465285

probeall
probeall assumptions data/nodes.json data/transactions
probeall assumptions all data/transactions
```

- [x] Lightning Node
- [x] Connects to another lightning node
- [x] Needs gossip about the network
- [x] Can create routes
- [x] "query routes"
- [x] "send to route"
- [x] Send fake payments (with fake payment hash)
- [x] Fork LDK
- [x] Figure out the error code (16399)
- [x] Handle unhandled error reason
- [x] Interpret payment failure errors
- [x] Open private channel between bob and carol
- [x] Create special hops
- [x] Detect difference (programatically) between when we use a real channel and not
- [x] Write results to the DB
- [x] Iterate over a list of UTXOs and see if any are a private channel
- [x] Should get different error when channel_id is correct but pubkey is wrong (unknown_next_peer) should be PERM|10, instead we're getting PERM|15
- [x] Only apply channel updates onto the channel it is meant for
- [x] Resume where it left off
- [x] Scrap a list of all public nodes
- [x] Scrap a list of all public channels
- [x] Filter out public channels from possible private channel list
- [x] If it can't reach a target node, skip the rest of the probes to it
- [x] Get a mainnet proxy node setup with a private channel with another node
- [x] Update datasets
- [x] Run on proxy node looking for a single private channel 
- [x] Run against some other targetted node
- [ ] Find out what % of public channels would fall into the assumption set
- [ ] Write up how combining parallel channel probing could work
- [ ] Retry payments if error is retriable and not what I'm looking for
- [ ] Work with multiple channels and multiple paths to increase speed
- [ ] Dump the results 
- [ ] Run through found channels to find the other node


# ldk-sample

Sample node implementation using LDK.

## Installation

```

git clone https://github.com/lightningdevkit/ldk-sample

```

## Usage

```

cd ldk-sample
cargo run <bitcoind-rpc-username>:<bitcoind-rpc-password>@<bitcoind-rpc-host>:<bitcoind-rpc-port> <ldk_storage_directory_path> [<ldk-peer-listening-port>] [bitcoin-network] [announced-listen-addr announced-node-name]

```

`bitcoind`'s RPC username and password likely can be found through `cat ~/.bitcoin/.cookie`.

`bitcoin-network`: defaults to `testnet`. Options: `testnet`, `regtest`, and `signet`.

`ldk-peer-listening-port`: defaults to 9735.

`announced-listen-addr` and `announced-node-name`: default to nothing, disabling any public announcements of this node.
`announced-listen-addr` can be set to an IPv4 or IPv6 address to announce that as a publicly-connectable address for this node.
`announced-node-name` can be any string up to 32 bytes in length, representing this node's alias.

## License

Licensed under either:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

```

```
