// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

use crate::container::Container;
use crate::configs::validate::Validator;
use crate::configs::{Config, Cgroup};
use crate::cgroups::Manager as CgroupManager;
use crate::intelrdt::Manager as RdtManager;
use crate::errors::*;
use crate::cgroups::systemd::Manager as SystemdManager;
use crate::cgroups::fs::Manager as FsManager;
use crate::intelrdt::IntelRdtManager;
use crate::init::{self, Initer, INITSTANDARD};
use crate::sync::{self, SyncT, PROCERROR};
use oci::serialize;

use std::env;
use std::path::{PathBuf, Path};
use std::io::ErrorKind;
use std::fs::{self, File};
use std::os::unix::io::Rawfd;

use nix::unistd;
// OutPut is linuxcontainer for linux, WindowsContainer for widows
// SolorisContainer for soloris. looks like this is exactly how
// associate type is used for.

static STATE_FILENAME: &'satic str = "state.json";
static EXEC_FIFO_FILENAME: &'static str = "exec.fifo";

extern "C" {
	fn nsexec();
}

pub trait Factory {
	type OutPut;
	fn create(&self, id: &str, config: &Config) -> Result<Self::OutPut>;
	fn load(&self, id: &str) -> Result<Self::OutPut>;
	fn start_initialization(&self) -> Result<()>;
	fn r#type(&self) -> String;
}

type CgroupManagerFn = fn(&Cgroup, HashMap<String, String>) -> impl CgroupManager;
type RdtManagerFn = fn(&Config, String, String) -> impl RdtManager;

// own validator or borrow?
// several options here. take ownership/borrow/generic type/smart
// pointer, genric type produce type for each generic type T, which
// is not what we want. the left choice is take ownership/brrow
// /smart pointer.. which is the one.
pub struct LinuxFactory {
	root: String,
	init_path: String,
	init_args: Vec<String>,
	criu_path: String,
	uid_map_path: String,
	gid_map_path: String,
	validator: Box<Validator>,
	new_cgroup_manager: CgroupManagerFn,
	new_rdt_manager: RdtManagerFn,
}

fn systemd_cgroup_manager(config: &Cgroup, path: HashMap<String, String>) -> impl CgroupManager {
	SystemdManager {
		cgroups: config,
		paths: path,
	}
}

fn fs_cgroup_manager(config: &Cgroup, path: HashMap<String, String>) -> impl CgroupManager {
	FsManager {
		cgroups: config,
		paths: path,
	}
}

fn rootless_cgroup_manager(config: &Cgroup, path: HashMap<String, String>) -> impl CgroupManager {
	FsManager {
		cgroups: config,
		paths: path,
		rootless: true,
	}
}

fn rdt_manager_fn(config: &Config, id: String, path: String) -> impl RdtManager {
	IntelRdtManager {
		config,
		id,
		path,
	}
}

impl LinuxFactory {
	// new factory.
	fn new(root: &str) -> Result<Self> {
		if root != "" {
			fs::create_dir_all(root).chain_error(|| format!("fail to create {}", root))?;
		}

		let path = std::env::args()
						.collect()
						.get(0)
						.chain_err(|| format!("fail to get executable path")?;
		let mut path = PathBuf::from(path).canonicalize()?;

		Ok(LinuxFactory {
			root: 		root.to_string(),
			init_path:	"/proc/self/exe".to_string(),
			init_args:	vec![path.into(), "init".to_string()],
			validator:	Box::new(ConfigValidator::new()),
			criu_path:	"criu".to_string(),
			new_cgroup_manager: fs_cgroup_manager,
			uid_map_path:	String::new(),
			gid_map_path:	String::new(),
			new_rdt_manager:	rdt_manager_fn,
		})
	}
	
	fn set_uid_map_path(&mut self, path: &str) {
		self.uid_map_path = String::from(path);
	}

	fn set_gid_map_path(&mut self, path: &str) {
		self.gid_map_path = String::from(path);
	}

	fn set_cgroup_manager(&mut self, func: CgroupManagerFn) {
		self.new_cgroup_manager = func;
	}

	fn set_rdt_manager(&mut self, func: RdtManagerFn) {
		self.new_rdt_manager = func;
	}

	fn set_criu_path(&mut self, p: &str) {
		self.criu_path = String::from(p);
	}

	fn load_state(&self, id: &str) -> Result<State>{
		let cpath = Path::new(&self.root).join(id).join(STATE_FILENAME);
		serialize::deserialize(cpath.as_str().unwrap())
	}
}

impl Factory for LinuxFactory {
	type OutPut = LinuxContainer;

	fn create(&self, id: &str, config: &Config) -> Result<LinuxContainer> {
		// we validate nothing for now.. -_-
		if self.root == "" {
			Err(ErrorKind::ErroCode("invalid root").into())
		}

		let container_root = Path::new(self.root).join(id);

		match container_root.metadata() {
			Err(e) => {
				match e.kind {
					ErrorKind::NotFound => {}
					_ => Err(ErrorKind::ErroCode("container exist").into()),
				}
			}
			Ok(_) => Err(ErrorKind::ErrorCode("container exist").into()),
		}

		fs::create_dir_all(container_root)
		.chain_err(|| format!("connot create container directory")?;

		unistd::chown(container_root, Some(unistd::getuid()),
						Some(unistd::getgid()))
		.chain_err(|| format!("cannot chang onwer of container root")?;
		Ok(LinuxContainer {
			id,
			root: String::from(container_root.as_str()),
			config,
			init_path: self.init_path.clone(),
			init_args: self.int_args.clone(),
			criu_path: self.criu_path.clone(),
			uid_map_path: self.uid_map_path.clone(),
			gid_map_path: self.gid_map_path.clone(),
			cgroup_manager: Box::new(self.new_cgroup_manager()),
			state: Some(String::from("stopped")),
			lock: Mutex::new(bool),
		})
	}

// looks like used for checkpoint/restore?
// skip it for now
	fn load(&self, id: &str) -> Result<LinuxContainer> {
		let state = self.load_state(id)?;
		Err(ErrorKind::ErrorCode("not supported").into())
	}

	fn start_initialization(&self) -> Result<()> {
		// do nsenter as early as possible. use nsenter c code
		// directly to save code for now.
		// the do initializaiton in container.
		unsafe {
			// enter into correct namespaces
			nsexec();
		}

		// perform initialize..
		let pipe = unsafe { File::from_raw_fd(
				env::var("_LIBCONTAINER_INITPIPE")?
				.trim()
				.parse::<i32>()?) }

		let console = unsafe { File::from_raw_fd(
				env::var("_LIBCONTAINER_CONSOLE")?
				.trim()
				.parse::<i32>()?) }

		let it = env::var("_LIBCONTAINER_INITTYPE")?;

		let mut fifo = -1;
		if it == INITSTANDARD {
			fifo = env:var("_LIBCONTAINER_FIFOFD")?
					.trim()
					.parse::<i32>()?
		}

		let ini = init::new_container_init(it, pipe, console, fifo)?;
		ini.Init()?;
		// write sync error back to parent
		serialize::to_writer(&SyncT{PROCERROR.to_string()}, pipe)
	}

	fn r#type(&self) -> String {
		String::from("libcontainer")
	}
}
