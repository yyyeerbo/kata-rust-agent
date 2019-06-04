//extern crate oci;
//extern crate libcontainer;
//#[macro_use]
//extern crate lazy_static;
#[macro_use]
extern crate log;

use futures::sync::oneshot;
use std::{io, thread};
use std::io::Read;
use futures::*;
use log::LevelFilter;
//use lazy_static::{self, initialize};

mod grpc;

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Trace);

    let mut server = grpc::start("unix:///var/run/kata-containers/cache2.sock", 1);

    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        info!("Press ENTER to exit...");
        let _ = io::stdin().read(&mut [0]).unwrap();
        tx.send(())
    });
    let _ = rx.wait();
    let _ = server.shutdown().wait();
}
