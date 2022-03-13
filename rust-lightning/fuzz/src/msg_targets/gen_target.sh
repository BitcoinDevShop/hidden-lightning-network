#!/bin/sh

GEN_TEST() {
	tn=msg_$(echo $1 | sed 's/\([a-z0-9]\)\([A-Z]\)/\1_\2/g' | tr '[:upper:]' '[:lower:]')
	fn=${tn}.rs
	cat msg_target_template.txt | sed s/MSG_TARGET/$1/ | sed "s/TARGET_NAME/$tn/" | sed "s/TEST_MSG/$2/" | sed "s/EXTRA_ARGS/$3/" > $fn
	echo "pub mod $tn;" >> mod.rs
}

echo "mod utils;" > mod.rs

# Note when adding new targets here you should add a similar line in src/bin/gen_target.sh

GEN_TEST AcceptChannel test_msg_simple ""
GEN_TEST AnnouncementSignatures test_msg_simple ""
GEN_TEST ClosingSigned test_msg_simple ""
GEN_TEST CommitmentSigned test_msg_simple ""
GEN_TEST FundingCreated test_msg_simple ""
GEN_TEST FundingLocked test_msg_simple ""
GEN_TEST FundingSigned test_msg_simple ""
GEN_TEST GossipTimestampFilter test_msg_simple ""
GEN_TEST Init test_msg_simple ""
GEN_TEST OnionHopData test_msg_simple ""
GEN_TEST OpenChannel test_msg_simple ""
GEN_TEST Ping test_msg_simple ""
GEN_TEST Pong test_msg_simple ""
GEN_TEST QueryChannelRange test_msg_simple ""
GEN_TEST ReplyShortChannelIdsEnd test_msg_simple ""
GEN_TEST RevokeAndACK test_msg_simple ""
GEN_TEST Shutdown test_msg_simple ""
GEN_TEST UpdateAddHTLC test_msg_simple ""
GEN_TEST UpdateFailHTLC test_msg_simple ""
GEN_TEST UpdateFailMalformedHTLC test_msg_simple ""
GEN_TEST UpdateFee test_msg_simple ""
GEN_TEST UpdateFulfillHTLC test_msg_simple ""

GEN_TEST ChannelReestablish test_msg ""
GEN_TEST DecodedOnionErrorPacket test_msg ""

GEN_TEST ChannelAnnouncement test_msg_exact ""
GEN_TEST NodeAnnouncement test_msg_exact ""
GEN_TEST QueryShortChannelIds test_msg ""
GEN_TEST ReplyChannelRange test_msg ""

GEN_TEST ErrorMessage test_msg_hole ", 32, 2"
GEN_TEST WarningMessage test_msg_hole ", 32, 2"
GEN_TEST ChannelUpdate test_msg_hole ", 108, 1"
