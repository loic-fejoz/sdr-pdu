#!/bin/bash
# To send on Uplink:
./pluto-tx-2fsk --frequency 145830000 --baud-rate 2400 --deviation 1200 --preamble 0x55 --preamble-repetition 8 --syncword 0x743F19E4
#To simulate Spino downlink:
#./pluto-tx-2fsk --frequency 435205000 --baud-rate 2400 --deviation 1200 --preamble 0x55 --preamble-repetition 10 --syncword 0x2efc9827
