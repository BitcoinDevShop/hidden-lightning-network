# Dev Notes

## Process for finding all P2WSH transactions and necessary information

Use a combination of several tools to get every single p2wsh and necessary information.

If I was a smarter person, I would figure out how to parse all of the transactions to get all of the information from a single script.

### bitcoin-utxo-dump

Get the current list of all unspent transactions from this [tool](https://github.com/in3rsha/bitcoin-utxo-dump)

I use a fork version with a bunch of continue statements for every non-p2wsh output


then run with :

```
bitcoin-utxo-dump -f count,txid,vout,height,coinbase,amount,script,type,address
```

```
count,txid,vout,height,coinbase,amount,script,type,address
1,d176d4960a78b41971f9d19207b59af6584b16ef323de55e983aec0100000000,2,684110,0,330,0020160d0000000000f0558db21dc3e8d765044120f3b6d18c22f5957ad83382521f,p2wsh,bc1qzcxsqqqqqqq0q4vdkgwu86xhv5zyzg8nkmgccgh4j4adsvuz2g0sjjkeu6
2,d176d4960a78b41971f9d19207b59af6584b16ef323de55e983aec0100000000,3,684110,0,330,00203f52bab5928e8e9388d8fe3c6c536faf8006b97a090501d035ef0eb9136d3868,p2wsh,bc1q8aft4dvj368f8zxclc7xc5m047qqdwt6pyzsr5p4au8tjymd8p5qezmq44
3,d176d4960a78b41971f9d19207b59af6584b16ef323de55e983aec0100000000,4,684110,0,330,00201698e842e20fa57ff8f72e6bf1533138fc0d0f41201b8b959b924ea19a53c809,p2wsh,bc1qz6vwsshzp7jhl78h9e4lz5e38r7q6r6pyqdch9vmjf82rxjneqysjdcsgk
4,d176d4960a78b41971f9d19207b59af6584b16ef323de55e983aec0100000000,5,684110,0,330,002019ae7a5b46cb44f12058461629eebf7b8b300d72f6017367e85ddb26f4c52f03,p2wsh,bc1qrxh85k6xedz0zgzcgctznm4l0w9nqrtj7cqhxelgthdjdax99upsc53hft
```


### bitcoin-iterate

Get a list of transactions and their block height from this [tool](https://github.com/rustyrussell/bitcoin-iterate). This program is needed to look up transaction id and block height since bitcoin-utxo-dump does not do block height.

```
# ./bitcoin-iterate -q --transaction=%th,%tN --start-hash=0000000000000000015411ca4b35f7b48ecab015b14de5627b647e262ba0ec40 --blockdir=/home/bitcoin/.bitcoin/mainnet/data/blocks > iterate.txt
./bitcoin-iterate -q --output=%th,%bN,%bH,%tN,%oN,%ol,%oa --start-hash=0000000000000000015411ca4b35f7b48ecab015b14de5627b647e262ba0ec40 --blockdir=/home/bitcoin/.bitcoin/mainnet/data/blocks > iterate.txt
```

example:
```
4b777745084ef83da587c7278db17f7a33ad5b831b5ee47b18c1c11c6165047c,0
e584dca0c8fe2df700c7c89021ab01de3bcc625b62c6b362a61c02f4c440a624,1
5393e535d9eaf10e8c87dca8f8bd05817dace0911ed25574c8257a21b603f23b,2
b66f62fae63243cc2141765387e51a950d982628cf3396107e743660bb3ec958,3
c977d774b272673f1abd0f4a52832bd8933894346b43a355014dd08a6249bfc7,4
d83894fcecb387d9813e5bdc0e19f2941406a5bd8ca80b27f19aeeb481752111,5
76f8d917a5a47cc3c92f26559c9cb9c3ee0f8027df8d6f0259ab5c65362e9048,6
2dfcb6cfff8ba9ed903f531a70d6443b9373c90868c94180152043aadc217185,7
8ba6d5674193f8f85433e4a85ce7a21b59357adf626ee70fa87355d9a6a9a32a,8
ad48e367144bb9b1cf1d668433268983254e551b4e58bfc3e257f5f7dbe4fb19,9
```


### Merge

Now bring the two files together by parsing the first list into a hashmap and then doing a lookup for each tx in the second list to see if is an unspent p2wsh - save the combination of the two data points if so. 


algorithm: 

```
p2wsh_map = [string]tx
for row in utxodump.csv
	p2wsh_map[row.txid] = row

for row in iterate.txt
	if p2wsh_map[row.txid]
		save json with row.txid, row.height and found p2wsh info
```


## Best guess for probing

- Over 100000 sats
- Divisible by 10000 with no remainder
- Under 2 for vout number
- Filter out UTXO's that are already locked in pub chan

### Advanced
- 
