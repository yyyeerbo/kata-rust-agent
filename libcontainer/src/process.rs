// use std::process::{Stdio, Command, ExitStatus};
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
use nix::unistd::{self, Pid};
use nix::sys::signal::{self, Signal};
use nix::Result;
use nix::sys::socket::{self, AddressFamily, SockType, SockProtocol, SockFlag};

use protocols::oci::Process as OCIProcess;
use nix::Error;
use nix::errno::Errno;

#[derive(Debug)]
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
	pub console_width: u32,
	pub console_height: u32,
//	pub caps: Capabilities,
	pub apparmor: String,
	pub label: String,
	pub no_new_privileges: bool,
//	pub rlimits: Vec<Rlimit>,
	pub console_socket: Option<RawFd>,
	pub term_master: Option<RawFd>,
// parent end of fds
	pub parent_console_socket: Option<RawFd>,
	pub parent_stdin: Option<RawFd>,
	pub parent_stdout: Option<RawFd>,
	pub parent_stderr: Option<RawFd>,
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
		wait::waitpid(Some(self.pid()), None)
	}

	fn signal(&self, sig: Signal) -> Result<()> {
		signal::kill(self.pid(), Some(sig))
	}
}

impl Process {
	pub fn new(ocip: &OCIProcess, id: &str, init: bool) -> Result<Self> {
		let mut p = Process {
			exec_id: String::from(id),
			args: ocip.Args.to_vec(),
			env: ocip.Env.to_vec(),
			user: String::from(""),
			additional_groups: Vec::new(),
			cwd: ocip.Cwd.clone(),
			stdin: None,
			stdout: None,
			stderr: None,
			console_width: 0,
			console_height: 0,
			extra_files: Vec::new(),
			apparmor: ocip.ApparmorProfile.clone(),
			label: ocip.SelinuxLabel.clone(),
			no_new_privileges: ocip.NoNewPrivileges,
			console_socket: None,
			term_master: None,
			parent_console_socket: None,
			parent_stdin: None,
			parent_stdout: None,
			parent_stderr: None,
			init,
			pid: -1,
		};

		info!("before create console socket!\n");

		if ocip.Terminal {
			let (psocket, csocket) = match socket::socketpair(
					AddressFamily::Unix,
					SockType::Stream,
					None,
					SockFlag::SOCK_CLOEXEC) {
				Ok((u, v)) => (u, v),
				Err(e) => {
					match e {
						Error::Sys(errno) => {
							info!("socketpair: {}", errno.desc());
						}
						_ => {
							info!("socketpair: other error!");
						}
					}
					return Err(e);
				}
			};
			p.parent_console_socket = Some(psocket);
			p.console_socket = Some(csocket);
		}

		info!("created console socket!\n");

		if ocip.ConsoleSize.is_some() {
			let console = ocip.ConsoleSize.as_ref().unwrap();
			p.console_width = console.Width;
			p.console_height = console.Height;
		}

		if ocip.User.is_some() {
			let user = ocip.User.as_ref().unwrap();
			p.user = user.Username.clone();
			
			let mut gids = Vec::new();
			for g in &user.AdditionalGids {
				gids.push(format!("{}", *g));
			}
			p.additional_groups = gids;
		}

		let (stdin, pstdin) = unistd::pipe()?;
		p.parent_stdin = Some(pstdin);
		p.stdin = Some(stdin);

		let (pstdout, stdout) = unistd::pipe()?;
		p.parent_stdout = Some(pstdout);
		p.stdout = Some(stdout);

		let (pstderr, stderr) = unistd::pipe()?;
		p.parent_stderr = Some(pstderr);
		p.stderr = Some(stderr);

		Ok(p)
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
