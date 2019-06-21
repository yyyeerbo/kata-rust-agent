//extern crate oci;
//extern crate libcontainer;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate libcontainer;
extern crate protocols;

use futures::sync::oneshot;
use futures::*;
use log::LevelFilter;
use std::fs;
use std::io::Read;
use std::{io, thread};
//use lazy_static::{self, initialize};
use std::env;
use std::time::Duration;

mod mount;
mod namespace;
mod network;
mod sandbox;

use namespace::Namespace;
use network::Network;
use sandbox::Sandbox;

mod grpc;

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Trace);
    env::set_var("RUST_BACKTRACE", "1");

    // Initialize unique sandbox structure.
    let s:Sandbox = Sandbox::new();

    let mut server = grpc::start(s, "unix:///tmp/testagent", 1);

    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        // info!("Press ENTER to exit...");
        // let _ = io::stdin().read(&mut [0]).unwrap();
        thread::sleep(Duration::from_secs(3000));

        tx.send(())
    });
    let _ = rx.wait();
    let _ = server.shutdown().wait();
    let _ = fs::remove_file("/tmp/testagent");
}
