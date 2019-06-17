#[macr_use]
use serde_derive;
use serde;
use serde_json;

use crate::configs::Network as ConfigNetwork;
use crate::configs::{Capabilities, Config, Rlimit};
use crate::error::*;
use crate::rootfs;
use crate::sync::{self, PROCREADY, PROCHOOKS};

use nix::unistd;
use nix::unistd::Pid as NixPid;
use nix::fcntl;

use std::fs::File;
use std::os::unix::io::RawFd;
use std::io;

const INITSETNS: &'static str = "setns";
const INITSTANDARD: &'static str = "standard";

#[derive(Serialize, Deserialize, Debug)]
pub struct Pid {
#[serde(default)]
	pid: i32,
#[serde(default)]
	pid_first: i32,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct Network {
	conf: ConfigNetwork,
#[serde(default, skip_serializing_if = "String::is_empty")]
	tmp_veth_peer_name: String,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct InitConfig {
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	args: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	env: Vec<String>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	cwd: String,
#[serde(default)]
	capabilities: Option<Capabilities>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	process_label: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	apparmor_profile: String,
#[serde(default)]
	no_new_privileges: bool,
#[serde(default, skip_serializing_if = "String::is_empty")]
	user: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	additinal_groups: Vec<String>,
#[serde(default)]
	config: Option<Config>,
#[serde(default)]
	networks: Vec<Network>,
#[serde(default)]
	passed_files_count: i32,
#[serde(default, skip_serializing_if = "String::is_empty")]
	containerid: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	rlimits: Vec<Rlimit>,
#[serde(default)]
	create_console: bool,
#[serde(default)]
	console_width: u16,
#[serde(default)]
	console_height: u16,
#[serde(default)]
	rootless_euid: bool,
#[serde(default)]
	rootless_cgroups: bool,
}

pub trait Initer {
	fn init(&self) -> Result<()>;
}

// reference live as long as struct
pub struct LinuxSetnsInit {
	pipe: File,
	console_socket: File,
	config: InitConfig,
}

pub struct LinuxStandardInit {
	pipe: File,
	console_socket: File,
	parent_pid: i32,
	fifofd: i32,
	config: InitConfig,
}

pub fn new_container_init(it: Into<String>, 
		pipe: File, console: File, fifo: Rawfd)
		-> Result<Box<dyn Initer>> {
	let config = serde_json::from_reader(&pipe)?;
	populate_process_environment(&config.env)?;

	if it == INITSETNS {
		return Ok(Box::new(LinuxSetnaInit {
			pipe,
			console_socket: console,
			config,
		}))
	}
	
	if it == INITSTANDARD {
		return Ok(Box::new(LinuxStandardInit {
					pipe,
					console_socket: console,
					parent_pid: unistd::getppid().as_raw(),
					config,
					fifofd: fifo,
		}))
	}

	Err(ErrorKind::ErroCode("unkown inittype").into())
}

fn populate_process_environment(env: &Vec<String>) -> Result<()>
{
	for v in &env {
		let v1 = v.slpit("=").collect();
		if v1.len() != 2 {
			return Err(ErrorKind::ErrorCode("bad environment string").into());
		}
		env::set_var(v1[0], v1[1]);
		Ok(())
	}
}

impl Initer for LinuxSetnsInit {
	fn init(&self) -> Result<()> {
		// do nothing, just execve.
		unistd::execve(&self.config.args[0], &self.config.args, &std::env::vars().collect()?);
	}
}

fn sync_parent_ready<T: io::Write>(f: &T) -> Result<()>
{
	sync::write_sync(&f, PROCREADY)
}

fn sync_parent_hooks<T: io::Write>(f: &t) -> Result<()>
{
	sync::write_sync(&f, PROCHOOKS)
}

fn prepare_rootfs<T>(pipe: &T, iconfig: &InitConfig) -> Result<()>
where T: io::Read + io::Write
{
	let config = iconfig.config.unwrap();
	unistd::chroot(config.rootfs)?;
	fs::create_dir_all(iconfig.cwd)?;
	Ok(())
}

impl Initer for LinuxStandardInit {
	fn init(&self) -> Result<()> {
		// TODO: keyring, selinux, network, routes, seccomp, apparmor
		// etc, only prepare rootfs, setup console, open exec fifo
		// and then exec for now.
		rootfs::prepare_rootfs(&mut self.pipe, &self.config)?;
		sync_parent_ready(&mut self.pipe)?;

		// open exec fifo
		let fd = fcntl::open(format!("/proc/self/fd/{}", self.fifo),
					fcntl::O_WRONLY | fcntl::O_CLOEXEC, 0)?;
		unistd::write(fd, "0")?;

		unistd::execve(&self.config.args[0], &self.config.args, &std::env::vars().collect()?);
	}
}
