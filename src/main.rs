//extern crate oci;
//extern crate libcontainer;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate libcontainer;
extern crate protocols;
extern crate prctl;

use futures::sync::oneshot;
use futures::*;
use log::LevelFilter;
use std::fs;
use std::io::Read;
use std::{io, thread, time};
//use lazy_static::{self, initialize};
use std::env;
use std::time::Duration;
use std::path::Path;
use prctl::set_child_subreaper;


mod mount;
mod namespace;
mod network;
mod sandbox;
mod version;

use namespace::Namespace;
use network::Network;
use sandbox::Sandbox;

mod grpc;

const VSOCK_DEV_PATH: &'static str = "/dev/vsock";
const VSOCK_EXIST_MAX_TRIES: u32   = 200;
const VSOCK_EXIST_WAIT_TIME: time::Duration    = time::Duration::from_millis(50);
const VSOCK_ADDR: &'static str = "vsock://-1";
const VSOCK_PORT: u16 = 1024;

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Trace);
    env::set_var("RUST_BACKTRACE", "1");

    setup_signal_handler().unwrap();

    // Initialize unique sandbox structure.
    let s:Sandbox = Sandbox::new();

    wait_for_vsock_dev(VSOCK_DEV_PATH);

    //vsock:///dev/vsock, port
    let mut server = grpc::start(s, VSOCK_ADDR, VSOCK_PORT);

    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        info!("Press ENTER to exit...");
        let _ = io::stdin().read(&mut [0]).unwrap();
        thread::sleep(Duration::from_secs(3000));

        tx.send(())
    });
	// receive something from destroy_sandbox here?
	// or in the thread above? It depneds whether grpc request
	// are run in another thread or in the main thead?
    let _ = rx.wait();
    let _ = server.shutdown().wait();
    let _ = fs::remove_file("/tmp/testagent");
}

fn wait_for_vsock_dev(vsock: &str) {
    for n in 1..=VSOCK_EXIST_MAX_TRIES {
        if Path::new(vsock).exists() {
            return
        } else {
            thread::sleep(VSOCK_EXIST_WAIT_TIME);
        }

        if n == VSOCK_EXIST_MAX_TRIES {
            warn!("the vsock device is not available")
        }
    }
}

fn setup_signal_handler() -> Result<(), String>{
    set_child_subreaper(true)
        .map_err(|err | format!("failed  to setup agent as a child subreaper, failed with {}", err))
}