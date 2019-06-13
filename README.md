git submodule update --init --recursive  
sudo ln -s /usr/bin/g++ /bin/musl-g++  
cargo build --target x86_64-unknown-linux-musl --release
