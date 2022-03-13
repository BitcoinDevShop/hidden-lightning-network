```
cargo run <bitcoind-rpc-username>:<bitcoind-rpc-password>@<bitcoind-rpc-host>:<bitcoind-rpc-port> <ldk_storage_directory_path> [<ldk-peer-listening-port>] [bitcoin-network] [announced-listen-addr announced-node-name]
```

```
source .env.local && cargo run $RPC_USER:$RPC_PASS@$RPC_HOST $STORAGE $LISTEN $NETWORK $NAME $ADDR
```

```
ALICE p2p
connectpeer 031d07c6730b1df65158754e76133deba307feb95526f0c3ca2971d77e0eb6d9a9@127.0.0.1:9735

bob pubkey
sendfakepayment 02c10ef3fcde4f4b15d1edda68726908a4d2f7f6f7159b99747c35d77fbc2902e1

correct channel id
probeprivate 02c10ef3fcde4f4b15d1edda68726908a4d2f7f6f7159b99747c35d77fbc2902e1 158329674465280

wrong channel id
probeprivate 02c10ef3fcde4f4b15d1edda68726908a4d2f7f6f7159b99747c35d77fbc2902e1 158329674465285
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
- [ ] Detect difference (programatically) between when we use a real channel and not
  - [ ] Want "final node" to only say true if it's actually the message from the final node
    - [ ] why doesn't it do that right now?
  - [ ] How do we escape from the "event" thing that lightning has?
  - [ ] Should we put fake info into the route hints instead of a fake hop?

MAYBE
- [ ] Iterate over multiple UTXOs and try them out with above
- [ ] GOAL: Find invisible channels!


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

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
