use std::collections::HashMap;
use std::ffi::{CString, OsStr};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use libcontainer::cgroups::Manager as CgroupManager;
use libcontainer::cgroups::fs::Manager as FsManager;

use std::path::Path;
use std::ptr::null;
use std::sync::MutexGuard;

use std::ffi::CStr;

use libc::{c_char, c_void, mount};
use nix::mount::MsFlags;

use crate::protocols::agent::Storage;
use crate::Sandbox;

const DRIVER9PTYPE: &'static str = "9p";
const DRIVERVIRTIOFSTYPE: &'static str = "virtio-fs";
const DRIVERBLKTYPE: &'static str = "blk";
const DRIVERMMIOBLKTYPE: &'static str = "mmioblk";
const DRIVERSCSITYPE: &'static str = "scsi";
const DRIVERNVDIMMTYPE: &'static str = "nvdimm";
const DRIVEREPHEMERALTYPE: &'static str = "ephemeral";

const ROOTBUSPATH: &'static str = "/devices/pci0000:00";

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

// StorageHandler is the type of callback to be defined to handle every
// type of storage driver.
type StorageHandler = fn(&Storage, &mut MutexGuard<Sandbox>) -> Result<String, String>;

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

    pub fn mount(&self) -> Result<(), String> {
        let mut source = null();
        let mut dest = null();
        let mut fs_type = null();
        let mut options = null();
        let cstr_options;
        let cstr_source;
        let cstr_dest;
        let cstr_fs_type;

        if self.source.len() == 0 {
            return Err("need mount source".to_string());
        }

        if self.destination.len() == 0 {
            return Err("need mount destination".to_string());
        }

        match CString::new(self.source) {
            Ok(s) => cstr_source = s,
            Err(err) => return Err(err.to_string()),
        }
        source = cstr_source.as_ptr();

        match CString::new(self.destination) {
            Ok(d) => cstr_dest = d,
            Err(e) => return Err(e.to_string()),
        }
        dest = cstr_dest.as_ptr();

        if self.fs_type.len() == 0 {
            return Err("need mount FS type".to_string());
        }

        match CString::new(self.fs_type) {
            Ok(fs_type) => cstr_fs_type = fs_type,
            Err(err) => return Err(err.to_string()),
        }
        fs_type = cstr_fs_type.as_ptr();

        if self.options.len() > 0 {
            match CString::new(self.options) {
                Ok(opts) => cstr_options = opts,
                Err(err) => return Err(err.to_string()),
            }
            options = cstr_options.as_ptr() as *const c_void;
        }

        let rc = unsafe { mount(source, dest, fs_type, self.flags.bits(), options) };

        if rc < 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        Ok(())
    }
}

fn ephemeral_storage_handler(
    storage: &Storage,
    sandbox: &mut MutexGuard<Sandbox>,
) -> Result<String, String> {
    let new_storage = sandbox.set_sandbox_storage(&storage.mount_point);

    if !new_storage {
        return Ok("".to_string());
    }

    if let Err(err) = fs::create_dir_all(Path::new(&storage.mount_point)) {
        return Err(err.to_string());
    }

    common_storage_handler(storage)
}

fn virtio9p_storage_handler(
    storage: &Storage,
    sandbox: &mut MutexGuard<Sandbox>,
) -> Result<String, String> {
    common_storage_handler(storage)
}

// virtioMmioBlkStorageHandler handles the storage for mmio blk driver.
fn virtiommio_blk_storage_handler(
    storage: &Storage,
    sandbox: &mut MutexGuard<Sandbox>,
) -> Result<String, String> {
    //The source path is VmPath
    common_storage_handler(storage)
}

// virtioFSStorageHandler handles the storage for virtio-fs.
fn virtiofs_storage_handler(
    storage: &Storage,
    sandbox: &mut MutexGuard<Sandbox>,
) -> Result<String, String> {
    common_storage_handler(storage)
}

fn common_storage_handler(storage: &Storage) -> Result<String, String> {
    // Mount the storage device.
    let mount_point = storage.mount_point.to_string();

    mount_storage(storage).and(Ok(mount_point))
}

// mount_storage performs the mount described by the storage structure.
fn mount_storage(storage: &Storage) -> Result<(), String> {
    let options_vec = storage.options.to_vec();
    let (flags, options) = parse_mount_flags_and_options(options_vec);
    let bare_mount = BareMount::new(
        storage.source.as_str(),
        storage.mount_point.as_str(),
        storage.fstype.as_str(),
        flags,
        options.as_str(),
    );

    bare_mount.mount()
}

fn parse_mount_flags_and_options(options_vec: Vec<String>) -> (MsFlags, String) {
    let mut flags = MsFlags::empty();
    let mut options: String = "".to_string();

    for opt in options_vec {
        if opt.len() != 0 {
            match FLAGS.get(opt.as_str()) {
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
pub fn add_storages(
    storages: Vec<Storage>,
    sandbox: &mut MutexGuard<Sandbox>,
) -> Result<Vec<String>, String> {
    let mut mount_list = Vec::new();

    for storage in storages {
        let handler = match STORAGEHANDLERLIST.get(storage.driver.as_str()) {
            None => {
                return Err(format!(
                    "Failed to find the storage handler {}",
                    storage.driver
                ))
            }
            Some(f) => f,
        };

        let mount_point = match handler(&storage, sandbox) {
            Err(e) => return Err(e),
            Ok(m) => m,
        };

        if mount_point.len() > 0 {
            mount_list.push(mount_point);
        }
    }

    Ok(mount_list)
}
