ubuntu installation

git clone https://github.com/judica-org/char-bitcoin
cd char-bitcoin

sudo apt update
sudo apt-get install build-essential cmake pkgconf python3
sudo apt-get install libevent-dev libboost-dev
sudo apt install libsqlite3-dev
sudo apt-get install libcapnp-dev capnproto
sudo apt-get install libzmq3-dev

cmake -DWITH_ZMQ=ON -B build
cmake --build build -- -j6
mkdir ../char-data
build/bin/bitcoind -signet -charenable -datadir=../char-data


# vegas_2026_char_workshop
