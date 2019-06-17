use std::process::{Stdio, Command, ExitStatus};
use std::fs::File;
use std::io;
use std::io::{Stdin, Stdout, Stderr, Read};
use std::os::unix::io::RawFd;
use std::os::unix::raw::pid_t;
use std::collections::HashMap;

// use crate::configs::{Capabilities, Rlimit};
// use crate::cgroups::Manager as CgroupManager;
// use crate::intelrdt::Manager as RdtManager;

use nix::sys::wait::{self, WaitStatus, WaitPidFlag};
use nix::unistd::Pid;
use nix::sys::signal::{self, Signal};
use nix::Result;

pub struct Process {
	pub exec_id: String,
	pub args: Vec<String>,
	pub env: Vec<String>,
	pub user: String,
	pub additional_groups: Vec<String>,
	pub cwd: String,
	pub stdin: Option<RawFd>,
	pub stdout: Option<RawFd>,
	pub stderr: Option<RawFd>,
	pub extra_files: Vec<File>,
	pub console_width: u16,
	pub console_height: u16,
//	pub caps: Capabilities,
	pub apparmor: String,
	pub label: String,
	pub no_new_privileges: bool,
//	pub rlimits: Vec<Rlimit>,
	pub console_socket: Option<RawFd>,
	pub term_master: Option<RawFd>,
	pub init: bool,
	// pid of the init/exec process. since we have no command
	// struct to store pid, we must store pid here.
	pub pid: pid_t,
}

pub trait ProcessOperations {
	fn pid(&self) -> Pid;
	fn wait(&self) -> Result<WaitStatus>;
	fn signal(&self, sig: Signal) -> Result<()>;
}

impl ProcessOperations for Process {
	fn pid(&self) -> Pid {
		Pid::from_raw(self.pid)
	}

	fn wait(&self) -> Result<WaitStatus> {
		wait::waitpid(Some(self.pid()), Some(WaitPidFlag::WNOHANG))
	}

	fn signal(&self, sig: Signal) -> Result<()> {
		signal::kill(self.pid(), Some(sig))
	}
}

pub struct Io {
	stdin: Stdin,
	stdout: Stdout,
	stderr: Stderr,
}

/*
pub trait ParentProcess {
	fn pid(&self) -> i32;
	fn start(&self) -> Result<()>;
	fn terminate(&self) -> Result<()>;
	fn wait(&self) -> Result<ExitStatus>;
	fn start_time(&self) -> Result<u64>;
	fn signal(&self, sig: i32) -> Result<()>;
	fn external_descriptors(&self) -> Vec<String>;
	fn set_external_descriptors(&self, fds: Vec<String>);
}

pub struct SetnsProcess {
	cmd: &Command,
	parent_pipe: File,
	child_pipe: File,
	cgroup_paths: HashMap<String, String>,
	rootless_cgroups: bool,
	intel_rdt_path: String,
	config: &InitConfig,
	fds: Vec<String>,
	process: &Process,
	bootstrap_data: Box<Read>,
}

pub struct InitProcess {
	cmd: &Command,
	parent_pipe: File,
	child_pipe: File,
	config: &InitConfig,
	cgroup_manager: Box<CgroupManager>,
	intel_rdt_manager: Box<RdtManager>,
	container: &LinuxContainer,
	fds: Vec<String>,
	process: &Process,
	bootstrap_data: Box<Read>,
	share_pidns: bool,
}
*/
