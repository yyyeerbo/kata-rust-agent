//extern crate oci;
//extern crate libcontainer;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate libcontainer;
extern crate protocols;
extern crate prctl;
extern crate serde_json;
extern crate signal_hook;

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
use std::sync::mpsc::{self, Sender, Receiver};
use signal_hook::{iterator::Signals, SIGCHLD};
use nix::sys::wait::{self, WaitStatus};
use nix::unistd;
use std::sync::{Arc, Mutex};


mod mount;
mod namespace;
mod network;
mod sandbox;
mod version;
pub mod netlink;

use namespace::Namespace;
use network::Network;
use sandbox::Sandbox;

mod grpc;

const VSOCK_ADDR: &'static str = "vsock://-1";
const VSOCK_PORT: u16 = 1024;

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Trace);
    env::set_var("RUST_BACKTRACE", "1");

    // Initialize unique sandbox structure.
    let sandbox = Arc::new(Mutex::new(Sandbox::new()));

    setup_signal_handler(sandbox.clone()).unwrap();

    let (tx, rx) = mpsc::channel::<i32>();
	sandbox.lock().unwrap().sender = Some(tx);

    //vsock:///dev/vsock, port
    let mut server = grpc::start(sandbox.clone(), VSOCK_ADDR, VSOCK_PORT);

    let handle = thread::spawn(move || {
        // info!("Press ENTER to exit...");
        // let _ = io::stdin().read(&mut [0]).unwrap();
        // thread::sleep(Duration::from_secs(3000));

        let _ = rx.recv().unwrap();
    });
	// receive something from destroy_sandbox here?
	// or in the thread above? It depneds whether grpc request
	// are run in another thread or in the main thead?
    // let _ = rx.wait();
	
	handle.join().unwrap();

    let _ = server.shutdown().wait();
    let _ = fs::remove_file("/tmp/testagent");
}

fn setup_signal_handler(sandbox: Arc<Mutex<Sandbox>>) -> Result<(), String>{
    set_child_subreaper(true)
        .map_err(|err | format!("failed  to setup agent as a child subreaper, failed with {}", err));

    let signals = match Signals::new(&[SIGCHLD]) {
        Ok(s) => s,
        Err(err) => return Err(format!("failed to setup singal handler with err {}", err.to_string()))
    };

    let mut s = sandbox.clone();

    thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received signal {:?}", sig);

            let wait_status = wait::wait().unwrap();
            let pid = wait_status.pid();
            if pid.is_some() {
                let raw_pid = pid.unwrap().as_raw();
                let mut sandbox = s.lock().unwrap();
                let mut process = sandbox.find_process(raw_pid);
                if process.is_none() {
                    info!("unexpected child exited {}", raw_pid);
                    continue;
                }

                let mut p = process.unwrap();

                if p.exit_pipe_w.is_none() {
                    error!("the process's exit_pipe_w isn't set");
                    continue;
                }
                let pipe_write = p.exit_pipe_w.unwrap();
                let mut ret: i32 = 0;

                match wait_status {
                    WaitStatus::Exited(_, c) => ret = c,
                    WaitStatus::Signaled(_, sig, _) => ret = sig as i32,
                    _ => {
                        info!("got wrong status for process {}", raw_pid);
                        continue;
                    }
                }

                p.exit_code = ret;
                unistd::close(pipe_write);
            }
        }
    });
	Ok(())
}

fn setup_signal_handler() -> Result<(), String>{
    set_child_subreaper(true)
        .map_err(|err | format!("failed  to setup agent as a child subreaper, failed with {}", err))
}
