// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

use rustjail::cgroups::fs::Manager as FsManager;
use rustjail::cgroups::Manager as CgroupManager;
use rustjail::errors::*;
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
use nix::mount::{self, MsFlags};

use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::device::{get_pci_device_name, get_scsi_device_name, online_device};
use crate::protocols::agent::Storage;
use crate::Sandbox;

const DRIVER9PTYPE: &'static str = "9p";
const DRIVERVIRTIOFSTYPE: &'static str = "virtio-fs";
const DRIVERBLKTYPE: &'static str = "blk";
const DRIVERMMIOBLKTYPE: &'static str = "mmioblk";
const DRIVERSCSITYPE: &'static str = "scsi";
const DRIVERNVDIMMTYPE: &'static str = "nvdimm";
const DRIVEREPHEMERALTYPE: &'static str = "ephemeral";
const DRIVERLOCALTYPE: &'static str = "local";

pub const TYPEROOTFS: &'static str   = "rootfs";

pub const PROCMOUNTSTATS: &'static str = "/proc/self/mountstats";

const ROOTBUSPATH: &'static str = "/devices/pci0000:00";

const CGROUPPATH: &'static str = "/sys/fs/cgroup";
const PROCCGROUPS: &'static str = "/proc/cgroups";

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
lazy_static!{
    static ref CGROUPS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("cpu", "/sys/fs/cgroup/cpu");
        m.insert("cpuacct", "/sys/fs/cgroup/cpuacct");
        m.insert("blkio", "/sys/fs/cgroup/blkio");
        m.insert("cpuset", "/sys/fs/cgroup/cpuset");
        m.insert("memory", "/sys/fs/cgroup/memory");
        m.insert("devices", "/sys/fs/cgroup/devices");
        m.insert("freezer", "/sys/fs/cgroup/freezer");
        m.insert("net_cls", "/sys/fs/cgroup/net_cls");
        m.insert("perf_event", "/sys/fs/cgroup/perf_event");
        m.insert("net_prio", "/sys/fs/cgroup/net_prio");
        m.insert("hugetlb", "/sys/fs/cgroup/hugetlb");
        m.insert("pids", "/sys/fs/cgroup/pids");
        m.insert("rdma", "/sys/fs/cgroup/rdma");
        m
    };
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
    let blk: StorageHandler = virtio_blk_storage_handler;
        m.insert(DRIVERBLKTYPE, blk);
	let p9: StorageHandler= virtio9p_storage_handler;
        m.insert(DRIVER9PTYPE, p9);
	let virtiofs: StorageHandler = virtiofs_storage_handler;
        m.insert(DRIVERVIRTIOFSTYPE, virtiofs);
    let ephemeral: StorageHandler = ephemeral_storage_handler;
        m.insert(DRIVEREPHEMERALTYPE, ephemeral);
    let virtiommio: StorageHandler = virtiommio_blk_storage_handler;
        m.insert(DRIVERMMIOBLKTYPE, virtiommio);
    let local: StorageHandler = local_storage_handler;
        m.insert(DRIVERLOCALTYPE, local);
    let scsi: StorageHandler = virtio_scsi_storage_handler;
        m.insert(DRIVERSCSITYPE, scsi);
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

fn local_storage_handler(
    storage: &Storage,
    sandbox: Arc<Mutex<Sandbox>>,
) -> Result<String> {
    let s = sandbox.clone();
    let mut sb = s.lock().unwrap();
    let new_storage = sb.set_sandbox_storage(&storage.mount_point);

    if !new_storage {
        return Ok("".to_string());
    }

    fs::create_dir_all(&storage.mount_point)?;

    let opts_vec: Vec<String> = storage.options.to_vec();

    let opts = parse_options(opts_vec);
    let mode = opts.get("mode");
    if mode.is_some() {
        let mode = mode.unwrap();
        let mut permission = fs::metadata(&storage.mount_point)?.permissions();

        let o_mode = u32::from_str_radix(mode, 8)?;
        permission.set_mode(o_mode);

        fs::set_permissions(&storage.mount_point, permission);
    }

    Ok("".to_string())
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

// virtio_scsi_storage_handler handles the storage for scsi driver.
fn virtio_scsi_storage_handler(
    storage: &Storage,
    sandbox: Arc<Mutex<Sandbox>>,
) -> Result<String> {

    let mut storage = storage.clone();

    // Retrieve the device path from SCSI address.
    let dev_path = get_scsi_device_name(sandbox, &storage.source)?;
    storage.source = dev_path;

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
        _ => {
            ensure_destination_exists( storage.mount_point.as_str(), storage.fstype.as_str())?;
        },
    }

    let options_vec = storage.options.to_vec();
    let options_vec = Vec::from_iter(options_vec.iter().map(String::as_str));
    let (flags, options) = parse_mount_flags_and_options(options_vec);

    info!(
        "mount storage as: mount-source: {},
                mount-destination: {},
                mount-fstype:      {},
                mount-options:     {}\n",
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

fn mount_to_rootfs(m: &INIT_MOUNT) -> Result<()> {
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

    Ok(())
}

pub fn general_mount() -> Result<()> {
    for m in INIT_ROOTFS_MOUNTS.iter() {
        mount_to_rootfs(m)?;
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
            return Ok(capes[1].to_string());
        }
    }

    Err(ErrorKind::ErrorCode(format!("failed to find FS type for mount point {}", mount_point)).into())
}

pub fn get_cgroup_mounts(cg_path: &str) -> Result<Vec<INIT_MOUNT>> {
    let file = File::open(&cg_path)?;
    let reader = BufReader::new(file);

    let mut has_device_cgroup = false;
    let mut cg_mounts: Vec<INIT_MOUNT> = vec![INIT_MOUNT {
        fstype: "tmpfs",
        src: "tmpfs",
        dest: CGROUPPATH,
        options: vec!["nosuid", "nodev", "noexec", "mode=755"]
    }];

    for (_, line) in reader.lines().enumerate() {
        let line = line?;

        let fields: Vec<&str> = line.split("\t").collect();
        // #subsys_name    hierarchy       num_cgroups     enabled
        // fields[0]       fields[1]       fields[2]       fields[3]
        match CGROUPS.get_key_value(fields[0]){
            Some((key, value)) => {
                if *key == "" || key.starts_with("#") || (fields.len() > 3 && fields[3] == "0") {
                    continue;
                }

                if *key == "devices" {
                    has_device_cgroup = true;
                }

                cg_mounts.push(INIT_MOUNT {
                    fstype: "cgroup",
                    src: "cgroup",
                    dest: *value,
                    options: vec!["nosuid", "nodev", "noexec", "relatime", *key]
                });
            }
            None => continue
        }
    };

    if !has_device_cgroup {
        warn!("The system didn't support device cgroup, which is dangerous, thus agent initialized without cgroup support!\n");
        return Ok(Vec::new());
    }

    cg_mounts.push(INIT_MOUNT {
        fstype: "tmpfs",
        src: "tmpfs",
        dest: CGROUPPATH,
        options: vec!["remount", "ro", "nosuid", "nodev", "noexec", "mode=755"]
    });

    Ok(cg_mounts)
}

pub fn cgroups_mount() -> Result<()> {
   let cgroups = get_cgroup_mounts(PROCCGROUPS)?;

    for cg in cgroups.iter() {
        mount_to_rootfs(cg)?;
    }

    // Enable memory hierarchical account.
    // For more information see https://www.kernel.org/doc/Documentation/cgroup-v1/memory.txt
    online_device("/sys/fs/cgroup/memory//memory.use_hierarchy")?;
    Ok(())
}

pub fn remove_mounts(mounts: &Vec<String>) -> Result<()> {
    for m in mounts.iter() {
        mount::umount(m.as_str())?;
    }
    Ok(())
}

// ensureDestinationExists will recursively create a given mountpoint. If directories
// are created, their permissions are initialized to mountPerm
fn ensure_destination_exists(destination: &str, fs_type: &str)  -> Result<()> {
    let d = Path::new(destination);
    if !d.exists() {
        let dir = match d.parent() {
            Some(d) => d,
            None => return Err(ErrorKind::ErrorCode(format!("mount destination {} doesn't exist", destination)).into())
        };
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
    }

    if fs_type  != "bind" || d.is_dir() {
        fs::create_dir_all(d)?;
    } else {
        fs::OpenOptions::new().create(true).open(d)?;
    }

    Ok(())

}

fn parse_options(option_list: Vec<String>) -> HashMap<String, String> {
    let mut options = HashMap::new();
    for opt in option_list.iter() {
        let fields: Vec<&str> = opt.split("=").collect();
        if fields.len() != 2 {
            continue;
        }

        options.insert(fields[0].to_string(), fields[1].to_string());
    }

    options
}
