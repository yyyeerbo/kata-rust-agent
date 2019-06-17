#[allow(unused_imports)]
use serde;
#[macro_use]
use serde_derive;
use serde_json;
#[macro_use]
use lazy_static;
#[macro_use]
use error_chain;
use oci::{self, Spec, Linux, LinuxNamespace};
use std::time::SystemTime;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::path::Path;
use std::fs;
use std::os::unix::io::RawFd;
use std::ffi::CString;
use crate::sync::Cond;
use std::fs::File;
use std::process::{Command};
use oci::{LinuxDevice, LinuxIDMapping};
use std::os::unix::raw::pid_t;
use std::os::unix::io::FromRawFd;
use std::fmt::Display;
use std::clone::Clone;

// use crate::configs::namespaces::{NamespaceType};
use crate::process::{self, Process};
use crate::cgroups::Manager as CgroupManager;
// use crate::intelrdt::Manager as RdtManager;
use crate::specconv::CreateOpts;
use crate::errors::*;
use crate::stats::Stats;
use crate::mount;

use nix::sys::stat::{self, Mode};
use nix::sys::socket::{self, AddressFamily, SockType, SockProtocol, SockFlag, ControlMessage, MsgFlags};
use nix::fcntl;
use nix::fcntl::{OFlag};
use nix::Error;
use nix::errno::Errno;
use nix::sched::{self, CloneFlags};
use nix::unistd::{self, Uid, Gid, Pid, ForkResult};
use nix::pty;
use nix::sys::uio::IoVec;
use nix::sys::signal::{self, Signal};
use nix::sys::wait;
use libc;

use std::io::{Error as IOError};
use std::collections::HashMap;
use scopeguard;

const STATE_FILENAME: &'static str = "state.json";
const EXEC_FIFO_FILENAME: &'static str = "exec.fifo";

type Status = Option<String>;
type Config = CreateOpts;
type NamespaceType = String;

/*
impl Status {
	fn to_string(&self) -> String {
		match *self {
			Some(ref v) => v.to_string(),
			None => "Unknown Status".to_string(),
		}
	}
}
*/

lazy_static!{
	static ref NAMESPACES: HashMap<&'static str, CloneFlags> = {
		let mut m = HashMap::new();
		m.insert("user", CloneFlags::CLONE_NEWUSER);
		m.insert("ipc", CloneFlags::CLONE_NEWIPC);
		m.insert("pid", CloneFlags::CLONE_NEWPID);
		m.insert("network", CloneFlags::CLONE_NEWNET);
		m.insert("mount", CloneFlags::CLONE_NEWNS);
		m.insert("uts", CloneFlags::CLONE_NEWUTS);
		m.insert("cgroup", CloneFlags::CLONE_NEWCGROUP);
		m
	};

	pub static ref DEFAULT_DEVICES: Vec<LinuxDevice> = {
        let mut v = Vec::new();
        v.push(LinuxDevice {
            path: "/dev/null".to_string(),
            r#type: "c".to_string(),
            major: 1,
            minor: 3,
            file_mode: Some(0o066),
            uid: None,
            gid: None,
        });
        v.push(LinuxDevice {
            path: "/dev/zero".to_string(),
            r#type: "c".to_string(),
            major: 1,
            minor: 5,
            file_mode: Some(0o066),
            uid: None,
            gid: None,
        });
        v.push(LinuxDevice {
            path: "/dev/full".to_string(),
            r#type: String::from("c"),
            major: 1,
            minor: 7,
            file_mode: Some(0o066),
            uid: None,
            gid: None,
        });
        v.push(LinuxDevice {
            path: "/dev/tty".to_string(),
            r#type: "c".to_string(),
            major: 5,
            minor: 0,
            file_mode: Some(0o066),
            uid: None,
            gid: None,
        });
        v.push(LinuxDevice {
            path: "/dev/urandom".to_string(),
            r#type: "c".to_string(),
            major: 1,
            minor: 9,
            file_mode: Some(0o066),
            uid: None,
            gid: None,
        });
        v.push(LinuxDevice {
            path: "/dev/random".to_string(),
            r#type: "c".to_string(),
            major: 1,
            minor: 8,
            file_mode: Some(0o066),
            uid: None,
            gid: None,
        });
        v
	};
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BaseState {
#[serde(default, skip_serializing_if = "String::is_empty")]
	id: String,
#[serde(default)]
	init_process_pid: i32,
#[serde(default)]
	init_process_start: u64,
/*
#[serde(default)]
	created: SystemTime,
	config: Config,
*/
}

pub trait BaseContainer {
	fn id(&self) -> String;
	fn status(&self) -> Result<Status>;
	fn state(&self) -> Result<State>;
	fn oci_state(&self) -> Result<oci::State>;
	fn config(&self) -> Result<&Config>;
	fn processes(&self) -> Result<Vec<i32>>;
	fn stats(&self) -> Result<Stats>;
	fn set(&mut self, config: Config) -> Result<()>;
	fn start(&mut self, mut p: Process) -> Result<()>;
	fn run(&mut self, mut p: Process) -> Result<()>;
	fn destroy(&mut self) -> Result<()>;
	fn signal(&self, sig: Signal, all: bool) -> Result<()>;
	fn exec(&mut self) -> Result<()>;
}

// LinuxContainer protected by Mutex
// Arc<Mutex<Innercontainer>> or just Mutex<InnerContainer>?
// Or use Mutex<xx> as a member of struct, like C?
// a lot of String in the struct might be &str
struct LinuxContainer<T>
where T: CgroupManager
{
	pub id: String,
	pub root: String,
	pub config: Config,
	pub cgroup_manager: Option<T>,
	pub init_process_pid: pid_t,
	pub init_process_start_time: u64,
	pub uid_map_path: String,
	pub gid_map_path: String,
	pub processes: HashMap<pid_t, Process>,
	pub status: Status,
	pub created: SystemTime,
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncPC {
#[serde(default)]
	pid: pid_t,
}


pub trait Container: BaseContainer {
//	fn checkpoint(&self, opts: &CriuOpts) -> Result<()>;
//	fn restore(&self, p: &Process, opts: &CriuOpts) -> Result<()>;
	fn pause(&self) -> Result<()>;
	fn resume(&self) -> Result<()>;
//	fn notify_oom(&self) -> Result<(Sender, Receiver)>;
//	fn notify_memory_pressure(&self, lvl: PressureLevel) -> Result<(Sender, Receiver)>;
}

impl<T> BaseContainer for LinuxContainer<T>
where T: CgroupManager
{
	fn id(&self) -> String {
		self.id.clone()
	}

	fn status(&self) -> Result<Status> {
		Ok(self.status.clone())
	}

	fn state(&self) -> Result<State> {
		Err(ErrorKind::ErrorCode(String::from("not suppoerted")).into())
	}

	fn oci_state(&self) -> Result<oci::State> {
		Err(ErrorKind::ErrorCode("not supported".to_string()).into())
	}

	fn config(&self) -> Result<&Config> {
		Ok(&self.config)
	}

	fn processes(&self) -> Result<Vec<i32>> {
		Ok(self.processes.keys().cloned().collect())
	}

	fn stats(&self) -> Result<Stats> {
		Err(ErrorKind::ErrorCode("not supported".to_string()).into())
	}

	fn set(&mut self, config: Config) -> Result<()> {
		self.config = config;
		Ok(())
	}

	fn start(&mut self, mut p: Process) -> Result<()> {
		let fifo_file = format!("{}/{}", self.root, EXEC_FIFO_FILENAME);
		let mut fifofd: RawFd = -1;
		if p.init {
			if let Ok(_) = stat::stat(fifo_file.as_str()) {
				return Err(ErrorKind::ErrorCode("exec fifo exists".to_string()).into());
			}
			unistd::mkfifo(fifo_file.as_str(), Mode::from_bits(0o622).unwrap())?;
			// defer!(fs::remove_file(&fifo_file)?);

			fifofd = fcntl::open(fifo_file.as_str(),
				OFlag::O_PATH | OFlag::O_CLOEXEC,
				Mode::from_bits(0).unwrap())?;
		}

		lazy_static::initialize(&NAMESPACES);
		lazy_static::initialize(&DEFAULT_DEVICES);

		if self.config.spec.is_none() {
			return Err(ErrorKind::ErrorCode("no spec".to_string()).into());
		}

		let spec = self.config.spec.as_ref().unwrap();
		if spec.linux.is_none() {
			return Err(ErrorKind::ErrorCode("no linux config".to_string()).into());
		}

		let linux = spec.linux.as_ref().unwrap();
		// get namespace vector to join/new
		let nses = get_namespaces(&linux, p.init, self.init_process_pid)?;
		let mut to_new = CloneFlags::empty();
		let mut to_join = Vec::new();
		let mut pidns = false;
		let mut userns = false;
		for ns in &nses {
			let s = NAMESPACES.get(&ns.r#type.as_str());
			if s.is_none() {
				return Err(ErrorKind::ErrorCode("invalid ns type".to_string()).into());
			}
			let s = s.unwrap();

			if ns.path.is_empty() {
				to_new.set(*s, true);
			} else {
				let fd = fcntl::open(ns.path.as_str(), OFlag::empty(),
									Mode::empty())
						.chain_err(|| format!("fail to open ns {}", &ns.r#type))?;
				to_join.push((*s, fd));
			}

			if *s == CloneFlags::CLONE_NEWPID {
				pidns = true;
			}
		}

		if to_new.contains(CloneFlags::CLONE_NEWUSER) {
			userns = true;
		}

		let (child, mut cfile) = join_namespaces(&spec, to_new, &to_join,
							pidns, userns, p.init)?;
		if child != Pid::from_raw(-1) {
			// parent
			p.pid = child.as_raw();
			self.status = Some("created".to_string());
			if p.init {
				self.init_process_pid = p.pid;
			}
			self.processes.insert(p.pid, p);
			self.created = SystemTime::now();
			// parent process need to receive ptmx masterfd
			// and set it up in process struct
			return Ok(());
		}

		// setup stdio in child process
		// need fd to send master fd to parent... store the fd in
		// process struct?
		setup_stdio(&p)?;

		if !p.cwd.is_empty() {
			unistd::chdir(p.cwd.as_str())?;
		}

		// notify parent to run poststart hooks
		// cfd is closed when return from join_namespaces
		// should retunr cfile instead of cfd?
		serde_json::to_writer(&mut cfile, &SyncPC { pid: 0 })?;

		// new and the stat parent process
		// For init process, we need to setup a lot of things 
		// For exec process, only need to join existing namespaces,
		// the namespaces are got from init process or from
		// saved spec.
		if p.init {
			let fd = fcntl::open(
				format!("/proc/self/fd/{}", fifofd).as_str(),
				OFlag::O_RDONLY | OFlag::O_CLOEXEC,
				Mode::from_bits_truncate(0))?;
			unistd::close(fifofd)?;
			let mut buf: &mut [u8] = &mut [0];
			unistd::read(fd, &mut buf)?;
		}

		// exec process
		do_exec(&p.args[0], &p.args, &p.env)?;

		Err(ErrorKind::ErrorCode("fail to create container".to_string()).into())
	}

	fn run(&mut self, mut p: Process) -> Result<()> {
		let init = p.init;
		self.start(p)?;

		if init {
			self.exec()?;
			self.status = Some("running".to_string());
		}

		Ok(())
	}

	fn destroy(&mut self) -> Result<()> {
		for pid in self.processes.keys() {
			signal::kill(Pid::from_raw(*pid), Some(Signal::SIGKILL))?;
		}

		self.status = Some("stopped".to_string());

		Ok(())
	}

	fn signal(&self, sig: Signal, all: bool) -> Result<()> {
		if all {
			for pid in self.processes.keys() {
				signal::kill(Pid::from_raw(*pid), Some(sig))?;
			}
		}

		signal::kill(Pid::from_raw(self.init_process_pid),
					Some(sig))?;

		Ok(())
	}

	fn exec(&mut self) -> Result<()> {
		let fifo = format!("{}/{}", &self.root, EXEC_FIFO_FILENAME);
		let fd = fcntl::open(fifo.as_str(), OFlag::O_WRONLY,
				Mode::from_bits_truncate(0))?;
		let data: &[u8] = &[0];
		unistd::write(fd, &data)?;
		self.init_process_start_time = SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH).unwrap()
			.as_secs();

		self.status = Some("running".to_string());

		Ok(())
	}
}

fn do_exec(path: &str, args: &[String], env: &[String]) -> Result<()> {
    let p = CString::new(path.to_string()).unwrap();
    let a: Vec<CString> = args
        .iter()
        .map(|s| CString::new(s.to_string()).unwrap_or_default())
        .collect();
    let env: Vec<CString> = env
        .iter()
        .map(|s| CString::new(s.to_string()).unwrap_or_default())
        .collect();
    // execvp doesn't use env for the search path, so we set env manually
    unistd::execve(&p, &a, &env).chain_err(|| "failed to exec")?;
    // should never reach here
    Ok(())
}

fn get_namespaces(linux: &Linux, init: bool, init_pid: pid_t) -> Result<Vec<LinuxNamespace>>
{
	let mut ns: Vec<LinuxNamespace> = Vec::new();
	if init {
		for i in &linux.namespaces {
			ns.push(LinuxNamespace { r#type: i.r#type.clone(),
						path: i.path.clone() });
		}
	} else {
		for i in NAMESPACES.keys() {
			ns.push(LinuxNamespace { r#type: i.to_string(),
				path: format!("/proc/{}/ns/{}", init_pid, i) });
		}
	}
	Ok(ns)
}

fn join_namespaces(spec: &Spec, to_new: CloneFlags, to_join: &Vec<(CloneFlags, RawFd)>, pidns: bool, userns: bool, init: bool) -> Result<(Pid, File)>
{
	let ccond = Cond::new().chain_err(|| "create cond failed")?;
	let pcond = Cond::new().chain_err(|| "create cond failed")?;
	let (pfd, cfd) = unistd::pipe2(OFlag::O_CLOEXEC).chain_err(
				|| "failed to create pipe")?;

	let linux = spec.linux.as_ref().unwrap();
	
	match unistd::fork()? {
		ForkResult::Parent {child} => {
			let mut pfile = unsafe { File::from_raw_fd(pfd) };
			unistd::close(cfd)?;
			ccond.wait()?;

			if userns {
				// setup uid/gid mappings
				write_mappings(&format!("/proc/{}/uid_map", child.as_raw()), &linux.uid_mappings)?;
				write_mappings(&format!("/proc/{}/gid_map", child.as_raw()), &linux.gid_mappings)?;
			}

			// apply cgroups
			pcond.notify()?;

			let mut pid = child.as_raw();
			if pidns {
				let msg: SyncPC = serde_json::from_reader(&mut pfile)?;
				pid = msg.pid;
				// notify child continue
				serde_json::to_writer(&mut pfile, &SyncPC { pid: 0 })?;
				// wait for child to exit
				let _ = wait::waitpid(Some(child), None)?;
			}
			// read out child pid here. we don't use
			// cgroup to get it
			// and the wait for child exit to get grandchild

			let _ = serde_json::from_reader(&pfile)?;
			if init {
				// run prestart hook
				let _ = serde_json::from_reader(&pfile)?;
				//run poststart hook
			}

			return Ok((Pid::from_raw(pid), pfile));
		}
		ForkResult::Child => {
			unistd::close(pfd)?;
			// set oom_score_adj
			// set rlimit
			if userns {
				sched::unshare(CloneFlags::CLONE_NEWUSER)?;
			}

			ccond.notify()?;
			pcond.wait()?;

			if userns {
				setid(Uid::from_raw(0), Gid::from_raw(0))?;
			}
		}
	}

	// child process continues
	let mut cfile = unsafe { File::from_raw_fd(cfd) };
	let mut mount_fd = -1;
	let mut bind_device = false;
	for &(s, fd) in to_join {
		if s == CloneFlags::CLONE_NEWNS {
			mount_fd = fd;
			continue;
		}

		sched::setns(fd, s)?;
		unistd::close(fd)?;

		if s == CloneFlags::CLONE_NEWUSER {
			setid(Uid::from_raw(0), Gid::from_raw(0))?;
			bind_device = true;
		}
	}

	sched::unshare(to_new & !CloneFlags::CLONE_NEWUSER)?;

	if userns {
		bind_device = true;
	}

	if pidns {
		match unistd::fork()? {
			ForkResult::Parent { child } => {
				// set child pid to topmost parent and the exit
				serde_json::to_writer(&mut cfile, &SyncPC { pid: child.as_raw() })?;
				// wait for parent read it and the continue
				let _ = serde_json::from_reader(&cfile)?;
				std::process::exit(0);
			}
			ForkResult::Child => {
			}
		}
	}

	if to_new.contains(CloneFlags::CLONE_NEWUTS) {
		unistd::sethostname(&spec.hostname)?;
	}

	let rootfs = spec.root.as_ref().unwrap().path.as_str();
	let root = fs::canonicalize(rootfs)?;
	let rootfs = root.to_str().unwrap();

	if to_new.contains(CloneFlags::CLONE_NEWNS) {
		// setup rootfs
		mount::init_rootfs(&spec, bind_device)?;
	}

	// notify parent to run prestart hooks
	if init {
		serde_json::to_writer(&mut cfile, &SyncPC { pid: 0 })?;
	}

	if mount_fd != -1 {
		sched::setns(mount_fd, CloneFlags::CLONE_NEWNS)?;
		unistd::close(mount_fd)?;
	}

	if to_new.contains(CloneFlags::CLONE_NEWNS) {
		// pivot root
		mount::pivot_rootfs(rootfs)?;
	}

	// notify parent to continue before block on exec fifo

	// block on exec fifo
	

	Ok((Pid::from_raw(-1), cfile))
}

fn setup_stdio(p: &Process) -> Result<()> {
	if p.console_socket.is_some() {
		// we can setup ptmx master for process
		let pseduo = pty::openpty(None, None)?;
		defer!(unistd::close(pseduo.master).unwrap());
		let data: &[u8] = b"/dev/ptmx";
		let iov = [IoVec::from_slice(&data)];
		let fds = [pseduo.master];
		let cmsg = ControlMessage::ScmRights(&fds);
		let mut console_fd = p.console_socket.unwrap();

		socket::sendmsg(console_fd,
				&iov, &[cmsg], MsgFlags::empty(),
				None)?;

		unistd::close(console_fd)?;
		console_fd = pseduo.slave;

		unistd::setsid()?;
		unsafe { libc::ioctl(console_fd, libc::TIOCSCTTY); }
		unistd::dup2(console_fd, 0)?;
		unistd::dup2(console_fd, 1)?;
		unistd::dup2(console_fd, 2)?;

		if console_fd > 2 {
			unistd::close(console_fd)?;
		}
	} else {
		// dup stdin/stderr/stdout
		unistd::dup2(p.stdin.unwrap(), 0)?;
		unistd::dup2(p.stdout.unwrap(), 1)?;
		unistd::dup2(p.stderr.unwrap(), 2)?;

		if p.stdin.unwrap() > 2 {
			unistd::close(p.stdin.unwrap())?;
		}

		if p.stdout.unwrap() > 2 {
			unistd::close(p.stdout.unwrap())?;
		}
		if p.stderr.unwrap() > 2 {
			unistd::close(p.stderr.unwrap())?;
		}
	}

	Ok(())
}

fn write_mappings(path: &str, maps: &[LinuxIDMapping]) -> Result<()> {
    let mut data = String::new();
    for m in maps {
        let val = format!("{} {} {}\n", m.container_id, m.host_id, m.size);
        data = data + &val;
    }
    if !data.is_empty() {
        let fd = fcntl::open(path, OFlag::O_WRONLY, Mode::empty())?;
        defer!(unistd::close(fd).unwrap());
        unistd::write(fd, data.as_bytes())?;
    }
    Ok(())
}

fn setid(uid: Uid, gid: Gid) -> Result<()> {
    // set uid/gid
    if let Err(e) = prctl::set_keep_capabilities(true) {
        bail!(format!("set keep capabilities returned {}", e));
    };
    {
        unistd::setresgid(gid, gid, gid)?;
    }
    {
        unistd::setresuid(uid, uid, uid)?;
    }
    // if we change from zero, we lose effective caps
    // if uid != Uid::from_raw(0) {
    //    capabilities::reset_effective()?;
    // }
    if let Err(e) = prctl::set_keep_capabilities(false) {
        bail!(format!("set keep capabilities returned {}", e));
    };
    Ok(())
}


impl<U> LinuxContainer<U>
where U: CgroupManager
{
	fn new<T: Into<String> + Display + Clone>(id: T, base: T, config: Config) -> Result<Self> {
		let root = format!("{}/{}", base.into(), id.clone().into());

		if let Err(e) = fs::create_dir_all(root.as_str()) {
			if e.kind() == std::io::ErrorKind::AlreadyExists {
				return Err(e).chain_err(|| format!("container {} already exists", id));
			}

			return Err(e).chain_err(|| format!("fail to create container directory {}", root));
		}

		unistd::chown(root.as_str(), Some(unistd::getuid()),
				Some(unistd::getgid()))
		.chain_err(|| format!("cannot change onwer of container {} root", id))?;

		Ok(LinuxContainer {
			id: id.into(),
			root,
			cgroup_manager: None,
			status: Some("stopped".to_string()),
			uid_map_path: String::from(""),
			gid_map_path: "".to_string(),
			config,
			processes: HashMap::new(),
			created: SystemTime::now(),
			init_process_pid: -1,
			init_process_start_time: SystemTime::now()
					.duration_since(SystemTime::UNIX_EPOCH)
					.unwrap().as_secs(),
		})
	}

	fn load<T: Into<String>>(id: T, base: T) -> Result<Self> {
		Err(ErrorKind::ErrorCode("not supported".to_string()).into())
	}
/*
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
*/
}
