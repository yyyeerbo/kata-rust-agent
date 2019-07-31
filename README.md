git submodule update --init --recursive  
sudo ln -s /usr/bin/g++ /bin/musl-g++  
cargo build --target x86\_64-unknown-linux-musl --release


# Feature List (Done)
  - create/start container
  - signal/wait process
  - I/O
  - Health API
  - exec/list process
  - network, interface/routes

# TODOS
  - Cgroups and container stats(api: update\_container, stats\_container)
  - initAgentAsInit(mount fs, udev, setup lo(easy to impl use netlink))
  - Capabilities, rlimit, readonly path, masked path, users
  - Validator
  - Hooks
  - api: reseed\_random\_device, copy\_file, online\_cpu\_memory,
  	mem\_hotplug\_probe, set\_guet\_data\_time
  - Refactor code
  - ci
