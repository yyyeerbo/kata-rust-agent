use serde;
#[macro_use]
use serde_derive;
use serde_json;
use oci::{self, Spec};
use std::time::SystemTime;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::path::Path;

use libcontainer::configs::{NamespaceType};
use libcontainer::process::{Stats, Process};
use libcontainer::cgroups::Manager as CgroupManager;
use libcontainer::intelrdt::Manager as RdtManager;
use libcontainer::specconv::CreateOpts;

use nix::sys::stat::{self, Mode};
use nix::sys::socket::{self, AddressFamily, SockType, SockProtocol, SockFlag};
use nix::fcntl;
use nix::fcntl::{OFlag};
use nix::Error;
use nix::errno::Errno;

use std::os::unix::io::Rawfd;
use std::io::{Error};

const STATE_FILENAME: &'static str = "state.json";
const EXEC_FIFO_FILENAME: &'static str = "exec.fifo";

type Status = Option<String>;
type Config = CreateOpts;

impl Status {
	fn to_string(&self) -> String {
		match *self {
			Some(ref v) => v.to_string(),
			None => "Unknown Status".to_string(),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BaseState {
#[serde(default, skip_serializing_if = "String::is_empty")]
	id: String,
#[serde(default)]
	init_process_pid: i32,
#[serde(default)]
	init_process_start: u64,
#[serde(default)]
	created: SystemTime,
#[serde(default)]
	config: Config,
}

pub trait BaseContainer {
	fn id(&self) -> String;
	fn status(&self) -> Result<Status>;
	fn state(&self) -> Result<&State>;
	fn oci_state(&self) -> Result<&oci::State>;
	fn config(&self) -> Result<Config>;
	fn processes(&self) -> Result<Vec<i32>>;
	fn stats(&self) -> Result<&Stats>;
	fn set(&self, config: Config) -> Result<()>;
	fn start(&self, p: Process) -> Result<()>;
	fn run(&self, p: Process) -> Result<()>;
	fn destroy(&self) -> Result<()>;
	fn signal(&self, sig: , all: bool) -> Result<()>;
	fn exec(&self) -> Result<()>;
}

// LinuxContainer protected by Mutex
// Arc<Mutex<Innercontainer>> or just Mutex<InnerContainer>?
// Or use Mutex<xx> as a member of struct, like C?
// a lot of String in the struct might be &str
struct LinuxContainer<T, 'a>
where T: CgroupManager
{
	id: String,
	root: String,
	config: &'a Config,
	cgroup_manager: Option<T>,
	init_process_start_time: u64,
	uid_map_path: String,
	gid_map_path: String,
	lock: Mutex<bool>,
	status: Status,
	State: State,
	created: SystemTime,
}

pub struct State {
	base: BaseState,
#[serde(default)]
	rootless: bool,
#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	cgroup_paths: HashMap<String, String>,
#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	namespace_paths: HashMap<NamespaceType, String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	external_descriptors: Vec<String>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	intel_rdt_path: String,
}

pub trait Container: BaseContainer {
	fn checkpoint(&self, opts: &CriuOpts) -> Result<()>;
	fn restore(&self, p: &Process, opts: &CriuOpts) -> Result<()>;
	fn pause(&self) -> Result<()>;
	fn resume(&self) -> Result<()>;
	fn notify_oom(&self) -> Result<(Sender, Receiver)>;
	fn notify_memory_pressure(&self, lvl: PressureLevel) -> Result<(Sender, Receiver)>;
}

impl LinuxContainer for BaseContainer {
	fn id(&self) -> String {
		self.id.clone()
	}

	fn status(&self) -> Result<Status> {
		Ok(self.status)
	}

	fn state(&self) -> Result<State> {
		Err(ErrorKind::ErrorCode("not suppoerted"))
	}

	fn oci_state(&self) -> Result<oci::State> {
		Err(ErrorKind::ErrorCode("not supported"))
	}

	fn config(&self) -> Result<Config> {
		Ok(self.config.clone())
	}

	fn processes(&self) -> Result<Vec<i32>> {
		Err(ErrorKind::ErrorCode("not supported"))
	}

	fn stats(&self) -> Result<Stats> {
		Err(ErrorKind::ErrorCode("not supported"))
	}

	fn set(&self, config: Config) -> Result<()> {
		self.config = &config;
		Ok(())
	}

	fn start(&self, p: Process) -> Result<()> {
		let _ = self.lock.lock();
		if p.init {
			let fifo_file = format!("{}/{}", self.root, EXEC_FIFO_FILENAME);
			if let Ok(_) = stat::stat(fifo_file) {
				return Err(ErrorKind::ErrorCode("exec fifo exists").into());
			}
			unistd::mkfifo(fifo_file, Mode::from_bits(0o622).unwrap())?;
		}
		// new and the stat parent process
		self.state = Some("created".to_string());
		Ok(())
	}

	fn run(&self, p: Process) -> Result<()> {
	}

	fn destroy(&self) -> Result<()> {
	}

	fn signal(&self, sig: , all: bool) -> Result<()> {
	}

	fn exec(&self) -> Result<()> {
	}
}

impl<U> LinuxContainer<U> {
	fn new<T: Into<String>>(id: T, base: T, config: &Config) -> Result<Self> {
		let root = format!("{}/{}", base.into(), id.into());

		if let Err(e) = fs::create_dir_all(root) {
			if e.kind() == std::io::ErrorKind::AlreadyExists {
				return Err(e).chain_err(|| format!("container {} already exists", id));
			}

			return Err(e).chain_err(|| format!("fail to create container directory {}", root));
		}

		unistd:chown(root, Some(unistd::getuid()),
				Some(unistd::getgid()))
		.chain_err(|| format!("cannot change onwer of container {} root", id))?;

		Ok(LinuxContainer {
			id: id.into(),
			root,
			cgroup_manager: None,
			status: Some("stopped".to_string()),
			lock: Mutex::new(false),
			uid_map_path: String::from(""),
			gid_map_path: "".to_string(),
			config,
		})
	}

	fn load<T: Into<String>>(id: T, base: T) -> Result<Self> {
		Err(ErrorKind::ErrorCode("not supported".into())
	}

	fn new_parent_process(&self, p: &Process) -> Result<Box<ParentProcess>> {
		let (pfd, cfd) = socket::socketpair(AddressFamily::Unix,
						SockType::Stream, SockProtocol::Tcp,
						SockFlag::SOCK_CLOEXEC)?;

		let cmd = Command::new(self.init_path)
						.args(self.init_args[1..])
						.env("_LIBCONTAINER_INITPIPE", format!("{}",
								cfd))
						.env("_LIBCONTAINER_STATEDIR", self.root)
						.current_dir(Path::new(self.config.rootfs))
						.stdin(p.stdin)
						.stdout(p.stdout)
						.stderr(p.stderr);

		if p.console_socket.is_some() {
			cmd.env("_LIBCONTAINER_CONSOLE", format!("{}", 
					unsafe { p.console_socket.unwrap().as_raw_fd() }));
		}

		if !p.init {
			return self.new_setns_process(p, cmd, pfd, cfd);
		}

		let fifo_file = format!("{}/{}", self.root, EXEC_FIFO_FILENAME);
		let fifofd = fcntl::open(fifo_file,
				OFlag::O_PATH | OFlag::O_CLOEXEC,
				Mode::from_bits(0).unwrap())?;

		cmd.env("_LIBCONTAINER_FIFOFD", format!("{}", fifofd));

		self.new_init_process(p, cmd, pfd, cfd)
	}

	fn new_setns_process(&self, p: &Process, cmd: &mut Command, pfd: Rawfd, cfd: Rawfd) -> Result<SetnsProcess> {
	}

	fn new_init_process(&self, p: &Process, cmd: &mut Command, pfd: Rawfd, cfd: Rawfd) -> Result<InitProcess> {
		cmd.env("_LINCONTAINER_INITTYPE", INITSTANDARD);
	}
}
