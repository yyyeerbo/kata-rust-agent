// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

//extern crate oci;
//extern crate libcontainer;
#![feature(map_get_key_value)]
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate libcontainer;
extern crate protocols;
extern crate prctl;
extern crate serde_json;
extern crate signal_hook;
extern crate regex;
#[macro_use]
extern crate scan_fmt;
extern crate oci;

use futures::sync::oneshot;
use futures::*;
use log::LevelFilter;
use std::fs;
use std::io::Read;
use std::{io, thread, time};
//use lazy_static::{self, initialize};
use std::env;
use unistd::Pid;
use std::time::Duration;
use std::path::Path;
use prctl::set_child_subreaper;
use std::sync::mpsc::{self, Sender, Receiver};
use signal_hook::{iterator::Signals, SIGCHLD};
use nix::sys::wait::{self, WaitStatus};
use std::os::unix::io::AsRawFd;
use nix::unistd;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use libcontainer::errors::*;

mod mount;
mod namespace;
mod network;
mod sandbox;
mod version;
mod uevent;
mod device;
pub mod netlink;
pub mod random;

use namespace::Namespace;
use network::Network;
use sandbox::Sandbox;
use uevent::watch_uevents;
use mount::{general_mount, cgroups_mount};

mod grpc;

const VSOCK_ADDR: &'static str = "vsock://-1";
const VSOCK_PORT: u16 = 1024;

lazy_static! {
    static ref GLOBAL_DEVICE_WATCHER: Arc<Mutex<HashMap<String, Sender<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

fn main() -> Result<()> {
    simple_logging::log_to_stderr(LevelFilter::Info);
	// simple_logging::log_to_file("/run/log.agent", LevelFilter::Info);
    env::set_var("RUST_BACKTRACE", "full");

    if unistd::getpid() == Pid::from_raw(1) {
        init_agent_as_init()?;
    }

    // Initialize unique sandbox structure.
    let s = Sandbox::new().map_err(|e| {
        error!("Failed to create sandbox with error: {:?}", e);
        e
    })?;

    let sandbox = Arc::new(Mutex::new(s));

    setup_signal_handler(sandbox.clone()).unwrap();
    watch_uevents(sandbox.clone());

    let (tx, rx) = mpsc::channel::<i32>();
	sandbox.lock().unwrap().sender = Some(tx);

    //vsock:///dev/vsock, port
    let mut server = grpc::start(sandbox.clone(), VSOCK_ADDR, VSOCK_PORT);

/*
    let _ = fs::remove_file("/tmp/testagent");
	let _ = fs::remove_dir_all("/run/agent");
    let mut server = grpc::start(sandbox.clone(), "unix:///tmp/testagent", 1);
*/

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

    Ok(())
}

fn setup_signal_handler(sandbox: Arc<Mutex<Sandbox>>) -> Result<()>{
    set_child_subreaper(true)
        .map_err(|err | format!("failed  to setup agent as a child subreaper, failed with {}", err));

    let signals = Signals::new(&[SIGCHLD])?;

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

// init_agent_as_init will do the initializations such as setting up the rootfs
// when this agent has been run as the init process.
fn init_agent_as_init() -> Result<()> {
    general_mount()?;
    cgroups_mount()?;

    fs::remove_file(Path::new("/dev/ptmx"))?;
    fs::soft_link(Path::new("/dev/pts/ptmx"), Path::new("/dev/ptmx"))?;

    unistd::setsid()?;

    unsafe { libc::ioctl(io::stdin().as_raw_fd(), libc::TIOCSCTTY, 1); }

    env::set_var("PATH", "/bin:/sbin/:/usr/bin/:/usr/sbin/");

    Ok(())
}
