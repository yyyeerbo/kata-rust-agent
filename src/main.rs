// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

//extern crate oci;
//extern crate rustjail;
#![feature(map_get_key_value)]
#![allow(non_camel_case_types)]
#![allow(unused_parens)]
// #![allow(deprecated)]
// #![allow(unused_imports)]
// #![allow(unreachable_code)]
// #![allow(unused_assignments)]
// #![allow(unused_variables)]
// #![allow(unused_mut)]
// #![allow(unused_must_use)]
#![allow(unused_unsafe)]
#![allow(dead_code)]
#![allow(non_snake_case)]
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate rustjail;
extern crate protocols;
extern crate prctl;
extern crate serde_json;
extern crate signal_hook;
extern crate regex;
#[macro_use]
extern crate scan_fmt;
extern crate oci;

use futures::*;
use log::LevelFilter;
use std::fs;
//use std::io::Read;
use std::{io, thread};
//use lazy_static::{self, initialize};
use std::env;
use unistd::Pid;
use std::path::Path;
use prctl::set_child_subreaper;
use std::sync::mpsc::{self, Sender};
use signal_hook::{iterator::Signals, SIGCHLD};
use nix::sys::wait::{self, WaitStatus};
use std::os::unix::io::AsRawFd;
use nix::unistd;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use rustjail::errors::*;
use std::os::unix::fs::{self as unixfs};

mod mount;
mod namespace;
mod network;
mod sandbox;
mod version;
mod uevent;
mod device;
pub mod netlink;
pub mod random;

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

use std::mem::MaybeUninit;

fn main() -> Result<()> {
    simple_logging::log_to_stderr(LevelFilter::Info);
	// simple_logging::log_to_file("/run/log.agent", LevelFilter::Info);
    env::set_var("RUST_BACKTRACE", "full");

	lazy_static::initialize(&SHELLS);
	parse_cmdline()?;

	let shell_handle = if unsafe { DEBUG_CONSOLE } {
		thread::spawn(move || {
			let _ = setup_debug_console();
		})
	} else {
		unsafe { MaybeUninit::zeroed().assume_init() }
	};

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

	if unsafe { DEBUG_CONSOLE } {
		shell_handle.join().unwrap();
	}

    let _ = server.shutdown().wait();
    let _ = fs::remove_file("/tmp/testagent");

    Ok(())
}

fn setup_signal_handler(sandbox: Arc<Mutex<Sandbox>>) -> Result<()>{
    set_child_subreaper(true)
        .map_err(|err | format!("failed  to setup agent as a child subreaper, failed with {}", err))?;

    let signals = Signals::new(&[SIGCHLD])?;

    let s = sandbox.clone();

    thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received signal {:?}", sig);

            let wait_status = wait::wait().unwrap();
            let pid = wait_status.pid();
            if pid.is_some() {
                let raw_pid = pid.unwrap().as_raw();
                let mut sandbox = s.lock().unwrap();
                let process = sandbox.find_process(raw_pid);
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
                let ret: i32;

                match wait_status {
                    WaitStatus::Exited(_, c) => ret = c,
                    WaitStatus::Signaled(_, sig, _) => ret = sig as i32,
                    _ => {
                        info!("got wrong status for process {}", raw_pid);
                        continue;
                    }
                }

                p.exit_code = ret;
                let _ = unistd::close(pipe_write);
            }
        }
    });
	Ok(())
}

// init_agent_as_init will do the initializations such as setting up the rootfs
// when this agent has been run as the init process.
fn init_agent_as_init() -> Result<()> {
    general_mount()?;
    cgroups_mount()?;

    fs::remove_file(Path::new("/dev/ptmx"))?;
    unixfs::symlink(Path::new("/dev/pts/ptmx"), Path::new("/dev/ptmx"))?;

    unistd::setsid()?;

    unsafe { libc::ioctl(io::stdin().as_raw_fd(), libc::TIOCSCTTY, 1); }

    env::set_var("PATH", "/bin:/sbin/:/usr/bin/:/usr/sbin/");

    Ok(())
}

const LOG_LEVEL_FLAG: &'static str = "agent.log";
const DEV_MODE_FLAG: &'static str = "agent.devmode";
const TRACE_MODE_FLAG: &'static str = "agent.trace";
const USE_VSOCK_FLAG: &'static str = "agent.use_vsock";
const DEBUG_CONSOLE_FLAG: &'static str = "agent.debug_console";
const KERNEL_CMDLINE_FILE: &'static str = "/proc/cmdline";
const CONSOLE_PATH: &'static str = "/dev/console";

lazy_static! {
	static ref SHELLS: Vec<String> = {
		let mut v = Vec::new();
		v.push("/bin/bash".to_string());
		v.push("/bin/sh".to_string());
		v
	};
}

pub static mut DEBUG_CONSOLE: bool = false;
// pub static mut LOG_LEVEL: ;
pub static mut DEV_MODE: bool = false;
// pub static mut TRACE_MODE: ;

fn parse_cmdline() -> Result<()> {
	let cmdline = fs::read_to_string(KERNEL_CMDLINE_FILE)?;
	let params: Vec<&str> = cmdline.split_ascii_whitespace().collect();
	for param in params.iter() {
		if param.starts_with(DEBUG_CONSOLE_FLAG) {
			unsafe { DEBUG_CONSOLE = true; }
		}

		if param.starts_with(DEV_MODE_FLAG) {
			unsafe { DEV_MODE = true; }
		}
	}

	Ok(())
}

use std::process::{Command, Stdio};
use std::path::PathBuf;
use nix::fcntl::{self, OFlag};
use nix::sys::stat::Mode;
use std::os::unix::io::{RawFd, FromRawFd};

fn setup_debug_console() -> Result<()> {
	for shell in SHELLS.iter() {
		let binary = PathBuf::from(shell);
		if binary.exists() {
			let f: RawFd = fcntl::open(CONSOLE_PATH, OFlag::O_RDWR, Mode::empty())?;
			let mut cmd = Command::new(shell)
						.stdin(unsafe { Stdio::from_raw_fd(f) })
						.stdout(unsafe { Stdio::from_raw_fd(f) })
						.stderr(unsafe { Stdio::from_raw_fd(f) })
						.spawn()?;
			cmd.wait()?;

			return Ok(())
		}
	}
	
	// no shell
	Err(ErrorKind::ErrorCode("no shell".to_string()).into())
}
