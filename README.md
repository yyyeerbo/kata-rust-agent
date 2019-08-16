# Kata Agent in Rust

This is a rust version of the [`kata-agent`](https://github.com/kata-containers/kata-agent).

In Denver PTG, [we discussed about re-writing agent in rust](https://etherpad.openstack.org/p/katacontainers-2019-ptg-denver-agenda):

> In general, we all think about re-write agent in rust to reduce the footprint of agent. Moreover, Eric mentioned the possibility to stop using gRPC, which may have some impact on footprint. We may begin to do some PoC to show how much we could save by re-writing agent in rust.

After that, we drafted the initial code here, and any contributions are welcome.

## Features

- [x] create/start container
- [x] signal/wait process
- [x] I/O
- [x] Health API
- [x] exec/list process
- [x] network, interface/routes
- [x] Cgroups and container stats(api: update\_container, stats\_container)
- [x] initAgentAsInit(mount fs, udev, setup lo(easy to impl use netlink))
- [x] Capabilities, rlimit, readonly path, masked path, users
- [ ] Validator
- [ ] Hooks
- [x] api: `reseed_random_device`, `copy_file`, `online_cpu_memory`, `mem_hotplug_probe`, `set_guet_data_time`
- [ ] Refactor code
- [ ] ci
- [ ] Debug Console
- [ ] cmdline
- [ ] Tracing

## Getting Started

### Dependencies

### Build from Source

```bash
git submodule update --init --recursive  
sudo ln -s /usr/bin/g++ /bin/musl-g++  
cargo build --target x86_64-unknown-linux-musl --release
```

## Run Kata CI with rust-agent
   * First, install kata as noted by ["how to install Kata"](https://github.com/kata-containers/documentation/blob/master/install/README.md)
   * Second, build your own kata initrd/image following the steps in ["how to build your own initrd/image"](https://github.com/kata-containers/documentation/blob/master/Developer-Guide.md#create-and-install-rootfs-and-initrd-image).
notes: Please use your rust agent instead of the go agent when building your initrd/image.
   * Clone the kata ci test cases from: https://github.com/kata-containers/tests.git, and then run the cri test with: 
````bash
$sudo -E PATH=$PATH -E GOPATH=$GOPATH   scripts/kata-e2e-node-test.sh
````

## Mini Benchmark
The memory consumed by the go-agent and rust-agent as below:
go-agent: about 11M
rust-agent: about 1.1M
