use std::process::{Stdio, Command, ExitStatus};
use std::fs::File;
use std::io;
use std::io::{Stdin, Stdout, Stderr, Read};

use libcontainer::configs::{Capabilities, Rlimit};
use libcontainer::cgroups::Manager as CgroupManager;
use libcontainer::intelrdt::Manager as RdtManager;

pub struct Process {
	args: Vec<String>,
	env: Vec<String>,
	user: String,
	additional_groups: Vec<String>,
	cwd: String,
	stdin: Stdio;
	stdout: Stdio,
	stderr: Stdio,
	extra_files: Vec<File>,
	console_width: u16,
	console_height: u16,
	caps: &Capabilities,
	apparmor: String,
	label: String,
	no_new_privileges: bool,
	rlimits: Vec<Rlimit>,
	console_socket: Option<File>,
	init: bool,
	// pid of the init/exec process. since we have no command
	// struct to store pid, we must store pid here.
	pid: pid_t,
}

pub struct Io {
	stdin: Stdin,
	stdout: Stdout,
	stderr: Stderr,
}

pub trait ParentProcess {
	fn pid(&self) -> i32;
	fn start(&self) -> Result<()>;
	fn terminate(&self) -> Result<()>;
	fn wait(&self) -> Result<ExitStatus>;
	fn start_time(&self) -> Result<u64>;
	fn signal(&self, i32) -> Result<()>;
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
