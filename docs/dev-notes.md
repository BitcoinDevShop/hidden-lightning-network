# Dev Notes

## Process for finding all P2WSH transactions and necessary information

Using Blockstream's esplora indexer [here](https://github.com/blockstream/esplora/blob/master/API.md)


Go through each block, get all transactions, look for each p2wsh output that is not currently spent.

### Go through each block

Starting with block height 477120 when segwit activated

Stop at height from API: `GET /blocks/tip/height`



For each block height `X` from `start_height` to `end_height`:
	Get the `hash` of the block currently parsing `GET /block-height/:height` replacing `height` with `X`

	Get the list of all transactions from this block `GET /block/:hash/txids`

	For each transaction index `Y`:
		Get the transaction details: `GET /tx/:txid`

		for each transaction vout `Z`:
			if vout.scriptpubkey_type != v0_p2wsh
				continue

			Get the transaction vout status `GET /tx/:txid/outspend/:vout`
			if spent
				continue

			Save blockheight `X`, transaction index `Y`, vout `Z`



Example:

```
$ curl localhost:3000/blocks/tip/height
732041

---
$ curl localhost:3000/block-height/729899
000000000000000000076863fb7af5de2954b803dca0ee294bd603abf2306330


---
$ curl localhost:3000/block/000000000000000000076863fb7af5de2954b803dca0ee294bd603abf2306330/txids | jq '.[0:4]'

[
  "cd80552b92222110f237f6f7f6cf27c14d3e72a0c3cb314ba694fe9df6dea20e",
  "4a83e492e9fbf58be7c9b98696a2063964a6fb5e63f5a3e7c539fbf902929780",
  "004d8a042436ed18429ba7d1c17277ac6883effa593979edbad25d978b85308e",
  "dfe94a1e20d11e2da2a4d3038f5f0b11dd7a17de4a1634b31751214cb9556114"
]


---
$ curl localhost:3000/tx/dfe94a1e20d11e2da2a4d3038f5f0b11dd7a17de4a1634b31751214cb9556114 | jq


{
  "txid": "dfe94a1e20d11e2da2a4d3038f5f0b11dd7a17de4a1634b31751214cb9556114",
  "version": 2,
  "locktime": 0,
  "vin": [
    {
      "txid": "1ed0cf9c461f182f6d2b60680fe5d36c599be3b02b6ab9073074b82f3822103c",
      "vout": 0,
      "prevout": {
        "scriptpubkey": "00142b138e148ed4275e3e40969c71a7af069d3655fc",
        "scriptpubkey_asm": "OP_0 OP_PUSHBYTES_20 2b138e148ed4275e3e40969c71a7af069d3655fc",
        "scriptpubkey_type": "v0_p2wpkh",
        "scriptpubkey_address": "bc1q9vfcu9yw6sn4u0jqj6w8rfa0q6wnv40uy28zgd",
        "value": 8993
      },
      "scriptsig": "",
      "scriptsig_asm": "",
      "witness": [
        "30450221009db70075bf721fce632e080acfc392be2dd23b0912eac420738a0fdf57e3b8c502202b9d9788e050278ac49d27bfef071d5927f8ff40b8587c6325c0582afec9be6101",
        "03af62d69eceffb831dc2b5a0ab00b5f81d0b231654ae4056815ceddbed917b347"
      ],
      "is_coinbase": false,
      "sequence": 0
    },
    {
      "txid": "601e23e3378108b88c127527120e63f9d56ecdfb63b2cb7ddf2273085318c0cc",
      "vout": 0,
      "prevout": {
        "scriptpubkey": "0014720cd9dee343996e171b634c96ef0cd457c54854",
        "scriptpubkey_asm": "OP_0 OP_PUSHBYTES_20 720cd9dee343996e171b634c96ef0cd457c54854",
        "scriptpubkey_type": "v0_p2wpkh",
        "scriptpubkey_address": "bc1qwgxdnhhrgwvku9cmvdxfdmcv63tu2jz5wlsezf",
        "value": 1315655
      },
      "scriptsig": "",
      "scriptsig_asm": "",
      "witness": [
        "3044022001b092e5a26d12febe7c3fbf039d562536607d31f215b8b63ec6408e757cca6002202291ed0a6b9ee23a960b3ceb6861ac5a3c7a2cc7cfd5aafe2a22295baf907bc001",
        "0359f5f2f07a02e25cd43aa09b2ae5882630f72ca6fb7689dd7971b20eba389ee0"
      ],
      "is_coinbase": false,
      "sequence": 0
    },
    {
      "txid": "a0b858ba3add74f8e9ec103f1c0cc812e589312acdadbfd788d00e6c4b9e1d22",
      "vout": 0,
      "prevout": {
        "scriptpubkey": "00149ccb833877b9f1202a0e122b30713f4e76ce8cab",
        "scriptpubkey_asm": "OP_0 OP_PUSHBYTES_20 9ccb833877b9f1202a0e122b30713f4e76ce8cab",
        "scriptpubkey_type": "v0_p2wpkh",
        "scriptpubkey_address": "bc1qnn9cxwrhh8cjq2swzg4nquflfemvar9trrapp2",
        "value": 4300949
      },

      "scriptsig": "",
      "scriptsig_asm": "",
      "witness": [
        "304402202e7568a8ccf39cf2372c96b90c4a4f2b13ae5033f49b4877cb4316bb264dce440220093cc9c3b9d9397f47d421d0c6f7fcde15d4628f7c51c2a1ed1ef47015e7341b01",
        "0220fed0aff9acab19dfc6c7ba19a808a80fe2a52b65012e44dbfa4daf4fb21b2f"
      ],
      "is_coinbase": false,
      "sequence": 0
    },
    {
      "txid": "aa128e447ca1a8dabae0a46af2b0e78fea8fe56e6f6c015f919d6270e92c018b",
      "vout": 0,
      "prevout": {
        "scriptpubkey": "0014dc1766259f9e10c753b33c269d16550e29d338b6",
        "scriptpubkey_asm": "OP_0 OP_PUSHBYTES_20 dc1766259f9e10c753b33c269d16550e29d338b6",
        "scriptpubkey_type": "v0_p2wpkh",
        "scriptpubkey_address": "bc1qmstkvfvlncgvw5an8snf69j4pc5axw9kz59cmr",
        "value": 98349
      },
      "scriptsig": "",
      "scriptsig_asm": "",
      "witness": [
        "3045022100ef6df117c0a34255470a0642e6a37f287a7d6c455537950541a6c173618249fe0220786ba54ac2a6847d9d2f9f79f57534b8e70a31f36b4e2c43d07fbcb032bb9b9c01",
        "030b0ad6b872ee8efd6a10b61b849573e2bfa442d369c67c46551076090b7e3bfd"
      ],
      "is_coinbase": false,
      "sequence": 0
    }
  ],
  "vout": [
    {
      "scriptpubkey": "001491900ab44b448aad4bbf7f57e179d4dc225ee144",
      "scriptpubkey_asm": "OP_0 OP_PUSHBYTES_20 91900ab44b448aad4bbf7f57e179d4dc225ee144",
      "scriptpubkey_type": "v0_p2wpkh",
      "scriptpubkey_address": "bc1qjxgq4dztgj926jal0at7z7w5ms39ac2y4nk6ak",
      "value": 722286
    },
    {
      "scriptpubkey": "0020b66e5b2c56414d5471903d27085a59ae79c31df718f3125965ada3e860d07bd4",
      "scriptpubkey_asm": "OP_0 OP_PUSHBYTES_32 b66e5b2c56414d5471903d27085a59ae79c31df718f3125965ada3e860d07bd4",
      "scriptpubkey_type": "v0_p2wsh",
      "scriptpubkey_address": "bc1qkeh9ktzkg9x4guvs85nsskje4euux80hrre3ykt94k37scxs002qhrnws7",
      "value": 5000000
    }
  ],
  "size": 680,
  "weight": 1424,
  "fee": 1660,
  "status": {
    "confirmed": true,
    "block_height": 729899,
    "block_hash": "000000000000000000076863fb7af5de2954b803dca0ee294bd603abf2306330",
    "block_time": 1648771792
  }
}

----
$ curl localhost:3000/tx/dfe94a1e20d11e2da2a4d3038f5f0b11dd7a17de4a1634b31751214cb9556114/outspend/1 | jq

{
  "spent": false
}
```
