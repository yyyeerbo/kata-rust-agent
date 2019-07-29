use libcontainer::cgroups::fs::Manager as FsManager;
use libcontainer::cgroups::Manager as CgroupManager;
use libcontainer::errors::*;
use std::collections::HashMap;
use std::ffi::{CString, OsStr};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::iter::FromIterator;

use std::path::Path;
use std::ptr::null;
use std::sync::{Arc, Mutex};

use std::ffi::CStr;

use libc::{c_char, c_void, mount};
use nix::mount::MsFlags;

use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::device::get_pci_device_name;
use crate::protocols::agent::Storage;
use crate::Sandbox;

const DRIVER9PTYPE: &'static str = "9p";
const DRIVERVIRTIOFSTYPE: &'static str = "virtio-fs";
const DRIVERBLKTYPE: &'static str = "blk";
const DRIVERMMIOBLKTYPE: &'static str = "mmioblk";
const DRIVERSCSITYPE: &'static str = "scsi";
const DRIVERNVDIMMTYPE: &'static str = "nvdimm";
const DRIVEREPHEMERALTYPE: &'static str = "ephemeral";
pub const TYPEROOTFS: &'static str   = "rootfs";

pub const PROCMOUNTSTATS: &'static str = "/proc/self/mountstats";

const ROOTBUSPATH: &'static str = "/devices/pci0000:00";

pub const TIMEOUT_HOTPLUG: u64 = 3;

#[cfg_attr(rustfmt, rustfmt_skip)]
lazy_static! {
    pub static ref FLAGS: HashMap<&'static str, (bool, MsFlags)> = {
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

pub struct INIT_MOUNT {
    fstype: &'static str,
    src: &'static str,
    dest: &'static str,
    options: Vec<&'static str>
}

#[cfg_attr(rustfmt, rustfmt_skip)]
lazy_static! {
    pub static ref INIT_ROOTFS_MOUNTS: Vec<INIT_MOUNT> = vec![
        INIT_MOUNT{fstype: "proc", src: "proc", dest: "/proc", options: vec!["nosuid", "nodev", "noexec"]},
        INIT_MOUNT{fstype: "sysfs", src: "sysfs", dest: "/sys", options: vec!["nosuid", "nodev", "noexec"]},
        INIT_MOUNT{fstype: "devtmpfs", src: "dev", dest: "/dev", options: vec!["nosuid"]},
        INIT_MOUNT{fstype: "tmpfs", src: "tmpfs", dest: "/dev/shm", options: vec!["nosuid", "nodev"]},
        INIT_MOUNT{fstype: "devpts", src: "devpts", dest: "/dev/pts", options: vec!["nosuid", "noexec"]},
        INIT_MOUNT{fstype: "tmpfs", src: "tmpfs", dest: "/run", options: vec!["nosuid", "nodev"]},
    ];
}

// StorageHandler is the type of callback to be defined to handle every
// type of storage driver.
type StorageHandler = fn(&Storage, Arc<Mutex<Sandbox>>) -> Result<String>;

// StorageHandlerList lists the supported drivers.
#[cfg_attr(rustfmt, rustfmt_skip)]
lazy_static! {
    pub static ref STORAGEHANDLERLIST: HashMap<&'static str, StorageHandler> = {
    	let mut m = HashMap::new();
	let p9: StorageHandler= virtio9p_storage_handler;
        m.insert(DRIVER9PTYPE, p9);
	let virtiofs: StorageHandler = virtiofs_storage_handler;
        m.insert(DRIVERVIRTIOFSTYPE, virtiofs);
    let ephemeral: StorageHandler = ephemeral_storage_handler;
        m.insert(DRIVEREPHEMERALTYPE, ephemeral);
    let virtiommio: StorageHandler = virtiommio_blk_storage_handler;
        m.insert(DRIVERMMIOBLKTYPE, virtiommio);
        m
    };
}

#[derive(Debug, Clone)]
pub struct BareMount<'a> {
    source: &'a str,
    destination: &'a str,
    fs_type: &'a str,
    flags: MsFlags,
    options: &'a str,
}

// mount mounts a source in to a destination. This will do some bookkeeping:
// * evaluate all symlinks
// * ensure the source exists
impl<'a> BareMount<'a> {
    pub fn new(s: &'a str, d: &'a str, fs_type: &'a str, flags: MsFlags, options: &'a str) -> Self {
        BareMount {
            source: s,
            destination: d,
            fs_type: fs_type,
            flags: flags,
            options: options,
        }
    }

    pub fn mount(&self) -> Result<()> {
        let mut source = null();
        let mut dest = null();
        let mut fs_type = null();
        let mut options = null();
        let cstr_options: CString;
        let cstr_source: CString;
        let cstr_dest: CString;
        let cstr_fs_type: CString;

        if self.source.len() == 0 {
            return Err(ErrorKind::ErrorCode("need mount source".to_string()).into());
        }

        if self.destination.len() == 0 {
            return Err(ErrorKind::ErrorCode("need mount destination".to_string()).into());
        }

        cstr_source = CString::new(self.source)?;
        source = cstr_source.as_ptr();

        cstr_dest = CString::new(self.destination)?;
        dest = cstr_dest.as_ptr();

        if self.fs_type.len() == 0 {
            return Err(ErrorKind::ErrorCode("need mount FS type".to_string()).into());
        }

        cstr_fs_type = CString::new(self.fs_type)?;
        fs_type = cstr_fs_type.as_ptr();

        if self.options.len() > 0 {
            cstr_options = CString::new(self.options)?;
            options = cstr_options.as_ptr() as *const c_void;
        }

        info!("mount source={:?}, dest={:?}, fs_type={:?}, options={:?}", self.source, self.destination, self.fs_type, self.options);
        let rc = unsafe { mount(source, dest, fs_type, self.flags.bits(), options) };

        if rc < 0 {
            return Err(ErrorKind::ErrorCode(format!("failed to mount {:?} to {:?}, with error: {}", self.source, self.destination, io::Error::last_os_error())).into());
        }
        Ok(())
    }
}

fn ephemeral_storage_handler(
    storage: &Storage,
    sandbox: Arc<Mutex<Sandbox>>,
) -> Result<String> {
    let s = sandbox.clone();
    let mut sb = s.lock().unwrap();
    let new_storage = sb.set_sandbox_storage(&storage.mount_point);

    if !new_storage {
        return Ok("".to_string());
    }

    if let Err(err) = fs::create_dir_all(Path::new(&storage.mount_point)) {
        return Err(err.into());
    }

    common_storage_handler(storage)
}

fn virtio9p_storage_handler(storage: &Storage, sandbox: Arc<Mutex<Sandbox>>) -> Result<String> {
    common_storage_handler(storage)
}

// virtiommio_blk_storage_handler handles the storage for mmio blk driver.
fn virtiommio_blk_storage_handler(
    storage: &Storage,
    sandbox: Arc<Mutex<Sandbox>>,
) -> Result<String> {
    //The source path is VmPath
    common_storage_handler(storage)
}

// virtiofs_storage_handler handles the storage for virtio-fs.
fn virtiofs_storage_handler(storage: &Storage, sandbox: Arc<Mutex<Sandbox>>) -> Result<String> {
    common_storage_handler(storage)
}

// virtio_blk_storage_handler handles the storage for blk driver.
fn virtio_blk_storage_handler(
    storage: &Storage,
    sandbox: Arc<Mutex<Sandbox>>,
) -> Result<String> {

    let mut storage = storage.clone();
    // If hot-plugged, get the device node path based on the PCI address else
    // use the virt path provided in Storage Source
    if storage.source.starts_with("/dev") {
        let metadata = fs::metadata(&storage.source)?;

        let mode = metadata.permissions().mode();
        if mode & libc::S_IFBLK == 0 {
            return Err(ErrorKind::ErrorCode(format!("Invalid device {}", &storage.source)).into());
        }
    } else {
        let dev_path = get_pci_device_name(sandbox, &storage.source)?;
        storage.source = dev_path;
    }

    common_storage_handler(&storage)
}

fn common_storage_handler(storage: &Storage) -> Result<String> {
    // Mount the storage device.
    let mount_point = storage.mount_point.to_string();

    mount_storage(storage).and(Ok(mount_point))
}

// mount_storage performs the mount described by the storage structure.
fn mount_storage(storage: &Storage) -> Result<()> {
    match storage.fstype.as_str() {
        DRIVER9PTYPE | DRIVERVIRTIOFSTYPE => {
            let dest_path = Path::new(storage.mount_point.as_str());
            if !dest_path.exists() {
                fs::create_dir_all(dest_path).chain_err(|| "Create mount destination failed")?;
            }
        }
        _ => (),
    }

    let options_vec = storage.options.to_vec();
    let options_vec = Vec::from_iter(options_vec.iter().map(String::as_str));
    let (flags, options) = parse_mount_flags_and_options(options_vec);

    info!(
        "mount storage as: mount-source: {},
                mount-destination: {},
                mount-fstype:      {},
                mount-options:     {}",
        storage.source.as_str(),
        storage.mount_point.as_str(),
        storage.fstype.as_str(),
        options.as_str()
    );

    let bare_mount = BareMount::new(
        storage.source.as_str(),
        storage.mount_point.as_str(),
        storage.fstype.as_str(),
        flags,
        options.as_str(),
    );

    bare_mount.mount()
}

fn parse_mount_flags_and_options(options_vec: Vec<&str>) -> (MsFlags, String) {
    let mut flags = MsFlags::empty();
    let mut options: String = "".to_string();

    for opt in options_vec {
        if opt.len() != 0 {
            match FLAGS.get(opt) {
                Some(x) => {
                    let (_, f) = *x;
                    flags = flags | f;
                }
                None => {
                    if options.len() > 0 {
                        options.push_str(format!(",{}", opt).as_str());
                    } else {
                        options.push_str(format!("{}", opt).as_str());
                    }
                }
            };
        }
    }
    (flags, options)
}

// add_storages takes a list of storages passed by the caller, and perform the
// associated operations such as waiting for the device to show up, and mount
// it to a specific location, according to the type of handler chosen, and for
// each storage.
pub fn add_storages(storages: Vec<Storage>, sandbox: Arc<Mutex<Sandbox>>) -> Result<Vec<String>> {
    let mut mount_list = Vec::new();

    for storage in storages {
        let handler = match STORAGEHANDLERLIST.get(storage.driver.as_str()) {
            None => {
                return Err(ErrorKind::ErrorCode(format!(
                    "Failed to find the storage handler {}",
                    storage.driver
                ))
                .into());
            }
            Some(f) => f,
        };

        let mount_point = match handler(&storage, sandbox.clone()) {
            // Todo need to rollback the mounted storage if err met.
            Err(e) => return Err(e),
            Ok(m) => m,
        };

        if mount_point.len() > 0 {
            mount_list.push(mount_point);
        }
    }

    Ok(mount_list)
}

pub fn general_mount() -> Result<()> {
    for m in INIT_ROOTFS_MOUNTS.iter() {
        let options_vec: Vec<&str> = m.options.clone();

        let (flags, options) = parse_mount_flags_and_options(options_vec);

        let bare_mount = BareMount::new(
            m.src,
            m.dest,
            m.fstype,
            flags,
            options.as_str(),
        );

        fs::create_dir_all(Path::new(m.dest)).chain_err(|| "could not creat directory")?;

        if let Err(err) = bare_mount.mount() {
            if m.src != "dev" {
                return Err(err.into());
            }
            error!("Could not mount filesystem from {} to {}", m.src, m.dest);
        }
    }

    Ok(())
}

// get_mount_fs_type returns the FS type corresponding to the passed mount point and
// any error ecountered.
pub fn get_mount_fs_type(mount_point: &str) -> Result<String> {
    if mount_point == "" {
        return Err(ErrorKind::ErrorCode(format!("Invliad mount point {}", mount_point)).into());
    }

    let file = File::open(PROCMOUNTSTATS)?;
    let reader = BufReader::new(file);

    let re = Regex::new(format!("device .+ mounted on {} with fstype (.+)", mount_point).as_str()).unwrap();

    // Read the file line by line using the lines() iterator from std::io::BufRead.
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        let capes = match re.captures(line.as_str()) {
            Some(c) => c,
            None => continue
        };

        if capes.len() > 1 {
            return Ok(capes[0].to_string());
        }
    }

    Err(ErrorKind::ErrorCode(format!("failed to find FS type for mount point {}", mount_point)).into())
}