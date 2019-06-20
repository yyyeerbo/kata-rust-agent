//extern crate oci;
//extern crate libcontainer;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate libcontainer;

use futures::sync::oneshot;
use std::{io, thread};
use std::io::Read;
use futures::*;
use log::LevelFilter;
use std::fs;
//use lazy_static::{self, initialize};
use std::env;
use std::time::Duration;

mod grpc;

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Trace);
	env::set_var("RUST_BACKTRACE", "1");

    let mut server = grpc::start("unix:///tmp/testagent", 1);

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
