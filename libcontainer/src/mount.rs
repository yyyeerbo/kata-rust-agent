use std::fs::{self, OpenOptions};
use std::path::{self, Path, PathBuf};
use oci::{self, Spec, LinuxDevice, Mount};
use std::os::unix;
use std::collections::HashMap;
use nix::unistd::{self, Uid, Gid};
use nix::mount::{self, MsFlags, MntFlags};
use nix::sys::stat::{self, Mode, SFlag};
use nix::fcntl::{self, OFlag};
use nix::NixPath;

use lazy_static;
use crate::container::DEFAULT_DEVICES;
use crate::errors::*;

pub struct Info {
	id: i32,
	parent: i32,
	major: i32,
	minor: i32,
	root: String,
	mount_point: String,
	opts: String,
	optional: String,
	fstype: String,
	source: String,
	vfs_opts: String,
}

lazy_static!{
	static ref PROPAGATION: HashMap<&'static str, MsFlags> = {
		let mut m = HashMap::new();
		m.insert("shared", MsFlags::MS_SHARED | MsFlags::MS_REC);
		m.insert("private", MsFlags::MS_PRIVATE | MsFlags::MS_REC);
		m.insert("slave", MsFlags::MS_SLAVE | MsFlags::MS_REC);
		m
	};

    static ref OPTIONS: HashMap<&'static str, (bool, MsFlags)> = {
        let mut m = HashMap::new();
        m.insert("defaults",      (false, MsFlags::empty()));
        m.insert("ro",            (false, MsFlags::MS_RDONLY));
        m.insert("rw",            (true,  MsFlags::MS_RDONLY));
        m.insert("suid",          (true,  MsFlags::MS_NOSUID));
        m.insert("nosuid",        (false, MsFlags::MS_NOSUID));
        m.insert("dev",           (true,  MsFlags::MS_NODEV));
        m.insert("nodev",         (false, MsFlags::MS_NODEV));
        m.insert("exec",          (true,  MsFlags::MS_NOEXEC));
        m.insert("noexec",        (false, MsFlags::MS_NOEXEC));
        m.insert("sync",          (false, MsFlags::MS_SYNCHRONOUS));
        m.insert("async",         (true,  MsFlags::MS_SYNCHRONOUS));
        m.insert("dirsync",       (false, MsFlags::MS_DIRSYNC));
        m.insert("remount",       (false, MsFlags::MS_REMOUNT));
        m.insert("mand",          (false, MsFlags::MS_MANDLOCK));
        m.insert("nomand",        (true,  MsFlags::MS_MANDLOCK));
        m.insert("atime",         (true,  MsFlags::MS_NOATIME));
        m.insert("noatime",       (false, MsFlags::MS_NOATIME));
        m.insert("diratime",      (true,  MsFlags::MS_NODIRATIME));
        m.insert("nodiratime",    (false, MsFlags::MS_NODIRATIME));
        m.insert("bind",          (false, MsFlags::MS_BIND));
        m.insert("rbind",         (false, MsFlags::MS_BIND | MsFlags::MS_REC));
        m.insert("unbindable",    (false, MsFlags::MS_UNBINDABLE));
        m.insert("runbindable",   (false, MsFlags::MS_UNBINDABLE | MsFlags::MS_REC));
        m.insert("private",       (false, MsFlags::MS_PRIVATE));
        m.insert("rprivate",      (false, MsFlags::MS_PRIVATE | MsFlags::MS_REC));
        m.insert("shared",        (false, MsFlags::MS_SHARED));
        m.insert("rshared",       (false, MsFlags::MS_SHARED | MsFlags::MS_REC));
        m.insert("slave",         (false, MsFlags::MS_SLAVE));
        m.insert("rslave",        (false, MsFlags::MS_SLAVE | MsFlags::MS_REC));
        m.insert("relatime",      (false, MsFlags::MS_RELATIME));
        m.insert("norelatime",    (true,  MsFlags::MS_RELATIME));
        m.insert("strictatime",   (false, MsFlags::MS_STRICTATIME));
        m.insert("nostrictatime", (true,  MsFlags::MS_STRICTATIME));
        m
    };
}

pub fn init_rootfs(spec: &Spec, bind_device: bool) -> Result<()> {
	lazy_static::initialize(&OPTIONS);
	lazy_static::initialize(&PROPAGATION);
	lazy_static::initialize(&LINUXDEVICETYPE);

	let linux = spec.linux.as_ref().unwrap();
	let mut flags = MsFlags::MS_REC;
	match PROPAGATION.get(&linux.rootfs_propagation.as_str()) {
		Some(fl) => flags |= *fl,
		None => flags |= MsFlags::MS_SLAVE,
	}

	let rootfs = spec.root.as_ref().unwrap().path.as_str();
	let root = fs::canonicalize(rootfs)?;
	let rootfs = root.to_str().unwrap();

	mount::mount(None::<&str>, "/", None::<&str>, flags, None::<&str>)?;
	mount::mount(Some(rootfs), rootfs, None::<&str>,
				MsFlags::MS_BIND | MsFlags::MS_REC, None::<&str>)?;
	
	for m in &spec.mounts {
		let (mut flags, data) = parse_mount(&m);
		if m.r#type == "cgroup" {
			continue;
		}

		if m.destination == "/dev" {
			flags &= !MsFlags::MS_RDONLY;
		}

		mount_from(&m, &rootfs, flags, &data, "")?;
	}

	let olddir = unistd::getcwd()?;
	unistd::chdir(rootfs)?;

	default_symlinks()?;
	create_devices(&linux.devices, bind_device)?;
	ensure_ptmx()?;

	unistd::chdir(&olddir)?;

	Ok(())
}

pub fn pivot_rootfs<P: ?Sized + NixPath>(path: &P) -> Result<()> {
    let oldroot =
        fcntl::open("/", OFlag::O_DIRECTORY | OFlag::O_RDONLY, Mode::empty())?;
    defer!(unistd::close(oldroot).unwrap());
    let newroot =
        fcntl::open(path, OFlag::O_DIRECTORY | OFlag::O_RDONLY, Mode::empty())?;
    defer!(unistd::close(newroot).unwrap());
    unistd::pivot_root(path, path)?;
    mount::umount2("/", MntFlags::MNT_DETACH)?;
    unistd::fchdir(newroot)?;
	stat::umask(Mode::from_bits_truncate(0o022));
    Ok(())
}

fn parse_mount(m: &Mount) -> (MsFlags, String) {
	let mut flags = MsFlags::empty();
	let mut data = Vec::new();

	for o in &m.options {
		match OPTIONS.get(o.as_str()) {
			Some(v) => { 
				let (clear, fl) = *v;
				if clear {
					flags &= !fl;
				} else {
					flags |= fl;
				}
			}

			None => data.push(o.clone()),
		}
	}

	(flags, data.join(","))
}

fn mount_from(m: &Mount, rootfs: &str,
			flags: MsFlags, data: &str,
			_label: &str) -> Result<()> {
	let d = String::from(data);
	let dest = format!("{}/{}", rootfs, &m.destination);

    let src = if m.r#type == "bind" {
        let src = fs::canonicalize(m.source.as_str())?;
        let dir = if src.is_file() {
            Path::new(&dest).parent().unwrap()
        } else {
            Path::new(&dest)
        };

        let _ = fs::create_dir_all(&dir);

        // make sure file exists so we can bind over it
        if src.is_file() {
 			let _ = OpenOptions::new().create(true).write(true).open(&dest);
        }
		src
    } else {
        let _ = fs::create_dir_all(&dest);
        PathBuf::from(&m.source)
    };

	mount::mount(Some(src.to_str().unwrap()), dest.as_str(), Some(m.r#type.as_str()), flags, Some(d.as_str()))?;

    if flags.contains(MsFlags::MS_BIND)
        && flags.intersects(
            !(MsFlags::MS_REC
                | MsFlags::MS_REMOUNT
                | MsFlags::MS_BIND
                | MsFlags::MS_PRIVATE
                | MsFlags::MS_SHARED
                | MsFlags::MS_SLAVE),
        ) {
        let chain = || format!("remount of {} failed", &dest);
        mount::mount(
            Some(dest.as_str()),
            dest.as_str(),
            None::<&str>,
            flags | MsFlags::MS_REMOUNT,
            None::<&str>,
        ).chain_err(chain)?;
    }
    Ok(())
}

static SYMLINKS: &'static [(&'static str, &'static str)] = &[
    ("/proc/self/fd", "dev/fd"),
    ("/proc/self/fd/0", "dev/stdin"),
    ("/proc/self/fd/1", "dev/stdout"),
    ("/proc/self/fd/2", "dev/stderr"),
];

fn default_symlinks() -> Result<()> {
    if Path::new("/proc/kcore").exists() {
        unix::fs::symlink("/proc/kcore", "dev/kcore")?;
    }
    for &(src, dst) in SYMLINKS {
        unix::fs::symlink(src, dst)?;
    }
    Ok(())
}
fn create_devices(devices: &[LinuxDevice], bind: bool) -> Result<()> {
    let op: fn(&LinuxDevice) -> Result<()> =
        if bind { bind_dev } else { mknod_dev };
    let old = stat::umask(Mode::from_bits_truncate(0o000));
    for dev in DEFAULT_DEVICES.iter() {
        op(dev)?;
    }
    for dev in devices {
        if !dev.path.starts_with("/dev") || dev.path.contains("..") {
            let msg = format!("{} is not a valid device path", dev.path);
            bail!(ErrorKind::ErrorCode(msg));
        }
        op(dev)?;
    }
    stat::umask(old);
    Ok(())
}

fn ensure_ptmx() -> Result<()> {
    let _ = fs::remove_file("dev/ptmx");
    unix::fs::symlink("pts/ptmx", "dev/ptmx")?;
    Ok(())
}

fn makedev(major: u64, minor: u64) -> u64 {
    (minor & 0xff)
        | ((major & 0xfff) << 8)
        | ((minor & !0xff) << 12)
        | ((major & !0xfff) << 32)
}

lazy_static!{
	static ref LINUXDEVICETYPE: HashMap<&'static str, SFlag> = {
		let mut m = HashMap::new();
		m.insert("c", SFlag::S_IFCHR);
		m.insert("b", SFlag::S_IFBLK);
		m.insert("p", SFlag::S_IFIFO);
		m
	};
}

fn mknod_dev(dev: &LinuxDevice) -> Result<()> {
    let f = match LINUXDEVICETYPE.get(dev.r#type.as_str()) {
		Some(v) => v,
		None => return Err(ErrorKind::ErrorCode("invalid spec".to_string()).into()),
	};

    stat::mknod(
        &dev.path[1..],
        *f,
        Mode::from_bits_truncate(dev.file_mode.unwrap_or(0)),
        makedev(dev.major as u64 , dev.minor as u64),
    )?;

    unistd::chown(
        &dev.path[1..],
        dev.uid.map(|n| Uid::from_raw(n)),
        dev.gid.map(|n| Gid::from_raw(n)),
    )?;

    Ok(())
}

fn bind_dev(dev: &LinuxDevice) -> Result<()> {
    let fd = fcntl::open(
        &dev.path[1..],
        OFlag::O_RDWR | OFlag::O_CREAT,
        Mode::from_bits_truncate(0o644),
    )?;

    unistd::close(fd)?;

    mount::mount(
        Some(&*dev.path),
        &dev.path[1..],
        None::<&str>,
        MsFlags::MS_BIND,
        None::<&str>,
    )?;
    Ok(())
}

