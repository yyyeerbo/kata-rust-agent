extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::collections::HashMap;
// use std::io::Write;
use libc::mode_t;
// use std::any::Any;

pub mod serialize;

#[allow(dead_code)]
fn is_false(b: bool) -> bool {
	!b
}

#[allow(dead_code)]
fn is_default<T>(d: &T) -> bool
where T: Default + PartialEq
{
	*d == T::default()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Spec {
#[serde(default, rename = "ociVersion", skip_serializing_if = "String::is_empty")]
	version: String,
	process: Option<Process>,
	root: Option<Root>,
#[serde(default, skip_serializing_if = "String:: is_empty")]
	hostname: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	mounts: Vec<Mount>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	hooks: Option<Hooks>,
#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	annotations: HashMap<String, String>,
#[serde(skip_serializing_if = "Option::is_none")]
	linux: Option<Linux>,
#[serde(skip_serializing_if = "Option::is_none")]
	solaris: Option<Solaris>,
#[serde(skip_serializing_if = "Option::is_none")]
	windows: Option<Windows<String>>,
#[serde(skip_serializing_if = "Option::is_none")]
	vm: Option<VM>,
}

#[allow(dead_code)]
type LinuxRlimit = POSIXRlimit;

#[derive(Serialize, Deserialize, Debug)]
pub struct Process {
#[serde(default)]
	terminal: bool,
#[serde(default, rename = "consoleSize")]
	console_size: Option<Box>,
	user: User,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	args: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	env: Vec<String>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	cwd: String,
#[serde(default, skip_serializing_if = "Option::is_none")]
	capabilities: Option<LinuxCapabilities>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	rlimits: Vec<POSIXRlimit>,
#[serde(default, rename = "noNewPrivileges")]
	no_new_privileges: bool,
#[serde(default, rename = "apparmorProfile", skip_serializing_if = "String::is_empty")]
	apparmor_profile: String,
#[serde(default, rename = "oomScoreAdj", skip_serializing_if = "Option::is_none")]
	oom_score_adj: Option<i32>,
#[serde(default, rename = "selinuxLabel", skip_serializing_if = "String::is_empty")]
	selinux_label: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxCapabilities {
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	bounding: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	effective: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	inheritable: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	permitted: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	ambient: Vec<String>,
}

#[derive(Default, PartialEq, Serialize, Deserialize, Debug)]
pub struct Box {
#[serde(default)]
	height: u32,
#[serde(default)]
	width: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
#[serde(default)]
	uid: u32,
#[serde(default)]
	gid: u32,
#[serde(default, rename = "addtionalGids", skip_serializing_if = "Vec::is_empty")]
	additional_gids: Vec<u32>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	username: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Root {
#[serde(default)]
	path: String,
#[serde(default)]
	readonly: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Mount {
#[serde(default)]
	destination: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	r#type: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	source: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	options: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Hook {
#[serde(default, skip_serializing_if = "String::is_empty")]
	path: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	args: Vec<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	env: Vec<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	timeout: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Hooks {
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	prestart: Vec<Hook>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	poststart: Vec<Hook>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	poststop: Vec<Hook>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Linux {
#[serde(default, rename = "uidMappings", skip_serializing_if = "Vec::is_empty")]
	uid_mappings: Vec<LinuxIDMapping>,
#[serde(default, rename = "gidMappings", skip_serializing_if = "Vec::is_empty")]
	gid_mappings: Vec<LinuxIDMapping>,
#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	sysctl: HashMap<String, String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	resources: Option<LinuxResources>,
#[serde(default, rename = "cgroupsPath", skip_serializing_if = "String::is_empty")]
	cgroups_path: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	namespaces: Vec<LinuxNamespace>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	devices: Vec<LinuxDevice>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	seccomp: Option<LinuxSeccomp>,
#[serde(default, rename = "rootfsPropagation", skip_serializing_if = "String::is_empty")]
	rootfs_propagation: String,
#[serde(default, rename = "maskedPaths", skip_serializing_if = "Vec::is_empty")]
	masked_paths: Vec<String>,
#[serde(default, rename = "readonlyPaths", skip_serializing_if = "Vec::is_empty")]
	readonly_paths: Vec<String>,
#[serde(default, rename = "mountLabel", skip_serializing_if = "String::is_empty")]
	mount_label: String,
#[serde(default, rename = "intelRdt", skip_serializing_if = "Option::is_none")]
	intel_rdt: Option<LinuxIntelRdt>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxNamespace {
#[serde(default, skip_serializing_if = "String::is_empty")]
	r#type: LinuxNamespaceType,
#[serde(default, skip_serializing_if = "String::is_empty")]
	path: String,
}

type LinuxNamespaceType = String;

#[allow(dead_code)]
const PIDNAMESPACE: &'static str = "pid";
#[allow(dead_code)]
const NETWORKNAMESPACE: &'static str = "network";
#[allow(dead_code)]
const MOUNTNAMSPACE: &'static str = "mount";
#[allow(dead_code)]
const IPCNAMESPACE: &'static str = "ipc";
#[allow(dead_code)]
const USERNAMESPACE: &'static str = "user";
#[allow(dead_code)]
const UTSNAMESPACE: &'static str = "uts";
#[allow(dead_code)]
const CGROUPNAMESPACE: &'static str = "cgroup";

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxIDMapping {
#[serde(default, rename = "containerID")]
	container_id: u32,
#[serde(default, rename = "hostID")]
	host_id: u32,
#[serde(default)]
	size: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct POSIXRlimit {
#[serde(default)]
	r#type: String,
#[serde(default)]
	hard: u64,
#[serde(default)]
	soft: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxHugepageLimit {
#[serde(default, rename = "pageSize", skip_serializing_if = "String::is_empty")]
	page_size: String,
#[serde(default)]
	limit: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxInterfacePriority {
#[serde(default, skip_serializing_if = "String::is_empty")]
	name: String,
#[serde(default)]
	priority: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxBlockIODevice {
#[serde(default)]
	major: i64,
#[serde(default)]
	minor: i64
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxWeightDevice {
	blk: LinuxBlockIODevice,
#[serde(default, skip_serializing_if = "Option::is_none")]
	weight: Option<u16>,
#[serde(default, rename = "leafWeight", skip_serializing_if = "Option::is_none")]
	leaf_weight: Option<u16>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxThrottleDevice {
	blk: LinuxBlockIODevice,
#[serde(default)]
	rate: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxBlockIO {
#[serde(default, skip_serializing_if = "Option::is_none")]
	weight: Option<u16>,
#[serde(default, rename = "leafWeight", skip_serializing_if = "Option::is_none")]
	leaf_weight: Option<u16>,
#[serde(default, rename = "weightDevice", skip_serializing_if = "Vec::is_empty")]
	weight_device: Vec<LinuxWeightDevice>,
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "throttleReadBpsDevice")]
	throttle_read_bps_device: Vec<LinuxThrottleDevice>,
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "throttleWriteBpsDevice")]
	throttle_write_bps_device: Vec<LinuxThrottleDevice>,
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "throttleReadIOPSDevice")]
	throttle_read_iops_device: Vec<LinuxThrottleDevice>,
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "throttleWriteIOPSDevice")]
	throttle_write_iops_device: Vec<LinuxThrottleDevice>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxMemory {
#[serde(default, skip_serializing_if = "Option::is_none")]
	limit: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	reservation: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	swap: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	kernel: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "kernelTCP")]
	kernel_tcp: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	swapiness: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "disableOOMKiller")]
	disable_oom_killer: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxCPU {
#[serde(default, skip_serializing_if = "Option::is_none")]
	shares: Option<u64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	quota: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	period: Option<u64>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "realtimeRuntime")]
	realtime_runtime: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "realtimePeriod")]
	realtime_period: Option<u64>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	cpus: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	mems: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxPids {
#[serde(default)]
	limit: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxNetwork {
#[serde(default, skip_serializing_if = "Option::is_none", rename = "classID")]
	class_id: Option<u32>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	priorities: Vec<LinuxInterfacePriority>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxRdma {
#[serde(default, skip_serializing_if = "Option::is_none", rename = "hcaHandles")]
	hca_handles: Option<u32>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "hcaObjects")]
	hca_objects: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxResources {
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	devices: Vec<LinuxDeviceCgroup>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	memory: Option<LinuxMemory>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	cpu: Option<LinuxCPU>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	pids: Option<LinuxPids>,
#[serde(skip_serializing_if = "Option::is_none", rename = "blockIO")]
	block_io: Option<LinuxBlockIO>,
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "hugepageLimits")]
	hugepage_limits: Vec<LinuxHugepageLimit>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	network: Option<LinuxNetwork>,
#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	rdma: HashMap<String, LinuxRdma>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxDevice {
#[serde(default, skip_serializing_if = "String::is_empty")]
	path: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	r#type: String,
#[serde(default)]
	major: i64,
#[serde(default)]
	minor: i64,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "fileMode")]
	file_mode: Option<mode_t>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	uid: Option<u32>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	gid: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxDeviceCgroup {
#[serde(default)]
	allow: bool,
#[serde(default, skip_serializing_if = "String::is_empty")]
	r#type: String,
#[serde(default, skip_serializing_if = "Option::is_none")]
	major: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	minor: Option<i64>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	access: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Solaris {
#[serde(default, skip_serializing_if = "String::is_empty")]
	milestone: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	limitpriv: String,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "maxShmMemory")]
	max_shm_memory: String,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	anet: Vec<SolarisAnet>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "cappedCPU")]
	capped_cpu: Option<SolarisCappedCPU>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "cappedMemory")]
	capped_memory: Option<SolarisCappedMemory>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SolarisCappedCPU {
#[serde(default, skip_serializing_if = "String::is_empty")]
	ncpus: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SolarisCappedMemory {
#[serde(default, skip_serializing_if = "String::is_empty")]
	physical: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	swap: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SolarisAnet {
#[serde(default, skip_serializing_if = "String::is_empty", rename = "linkname")]
	link_name: String,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "lowerLink")]
	lower_link: String,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "allowdAddress")]
	allowed_addr: String,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "configureAllowedAddress")]
	config_allowed_addr: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	defrouter: String,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "linkProtection")]
	link_protection: String,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "macAddress")]
	mac_address: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Windows<T> {
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "layerFolders")]
	layer_folders: Vec<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	resources: Option<WindowsResources>,
#[serde(default, rename = "credentialSpec")]
	credential_spec: T,
#[serde(default)]
	servicing: bool,
#[serde(default, rename = "ignoreFlushesDuringBoot")]
	ignore_flushes_during_boot: bool,
#[serde(default, skip_serializing_if = "Option::is_none")]
	hyperv: Option<WindowsHyperV>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	network: Option<WindowsNetwork>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsResources {
#[serde(default, skip_serializing_if = "Option::is_none")]
	memory: Option<WindowsMemoryResources>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	cpu: Option<WindowsCPUResources>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	storage: Option<WindowsStorageResources>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsMemoryResources {
#[serde(default, skip_serializing_if = "Option::is_none")]
	limit: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsCPUResources {
#[serde(default, skip_serializing_if = "Option::is_none")]
	count: Option<u64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	shares: Option<u64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	maximum: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsStorageResources {
#[serde(default, skip_serializing_if = "Option::is_none")]
	iops: Option<u64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
	bps: Option<u64>,
#[serde(default, skip_serializing_if = "Option::is_none", rename = "sandboxSize")]
	sandbox_size: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsNetwork {
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "endpointList")]
	endpoint_list: Vec<String>,
#[serde(default, rename = "allowUnqualifiedDNSQuery")]
	allow_unqualified_dns_query: bool,
#[serde(default, skip_serializing_if = "Vec::is_empty", rename = "DNSSearchList")]
	dns_search_list: Vec<String>,
#[serde(default, skip_serializing_if = "String::is_empty", rename = "nwtworkSharedContainerName")]
	network_shared_container_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsHyperV {
#[serde(default, skip_serializing_if = "String::is_empty", rename = "utilityVMPath")]
	utility_vm_path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VM {
	hypervisor: VMHypervisor,
	kernel: VMKernel,
	image: VMImage,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VMHypervisor {
#[serde(default)]
	path: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	parameters: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VMKernel {
#[serde(default)]
	path: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	parameters: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	initrd: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VMImage {
#[serde(default)]
	path: String,
#[serde(default)]
	format: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxSeccomp {
#[serde(default, rename = "defaultAction")]
	default_action: LinuxSeccompAction,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	architectures: Vec<Arch>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	syscalls: Vec<LinuxSyscall>,
}

type Arch = String;

#[allow(dead_code)]
const ARCHX86: &'static str = "SCMP_ARCH_X86";
#[allow(dead_code)]
const ARCHX86_64: &'static str = "SCMP_ARCH_X86_64";
#[allow(dead_code)]
const ARCHX32: &'static str = "SCMP_ARCH_X32";
#[allow(dead_code)]
const ARCHARM: &'static str = "SCMP_ARCH_ARM";
#[allow(dead_code)]
const ARCHAARCH64: &'static str = "SCMP_ARCH_AARCH64";
#[allow(dead_code)]
const ARCHMIPS: &'static str = "SCMP_ARCH_MIPS";
#[allow(dead_code)]
const ARCHMIPS64: &'static str = "SCMP_ARCH_MIPS64";
#[allow(dead_code)]
const ARCHMIPS64N32: &'static str = "SCMP_ARCH_MIPS64N32";
#[allow(dead_code)]
const ARCHMIPSEL: &'static str = "SCMP_ARCH_MIPSEL";
#[allow(dead_code)]
const ARCHMIPSEL64: &'static str = "SCMP_ARCH_MIPSEL64";
#[allow(dead_code)]
const ARCHMIPSEL64N32: &'static str = "SCMP_ARCH_MIPSEL64N32";
#[allow(dead_code)]
const ARCHPPC: &'static str = "SCMP_ARCH_PPC";
#[allow(dead_code)]
const ARCHPPC64: &'static str = "SCMP_ARCH_PPC64";
#[allow(dead_code)]
const ARCHPPC64LE: &'static str = "SCMP_ARCH_PPC64LE";
#[allow(dead_code)]
const ARCHS390: &'static str = "SCMP_ARCH_S390";
#[allow(dead_code)]
const ARCHS390X: &'static str = "SCMP_ARCH_S390X";
#[allow(dead_code)]
const ARCHPARISC: &'static str = "SCMP_ARCH_PARISC";
#[allow(dead_code)]
const ARCHPARISC64: &'static str = "SCMP_ARCH_PARISC64";

type LinuxSeccompAction = String;

#[allow(dead_code)]
const ACTKILL: &'static str = "SCMP_ACT_KILL";
#[allow(dead_code)]
const ACTTRAP: &'static str = "SCMP_ACT_TRAP";
#[allow(dead_code)]
const ACTERRNO: &'static str = "SCMP_ACT_ERRNO";
#[allow(dead_code)]
const ACTTRACE: &'static str = "SCMP_ACT_TRACE";
#[allow(dead_code)]
const ACTALLOW: &'static str = "SCMP_ACT_ALLOW";

type LinuxSeccompOperator = String;

#[allow(dead_code)]
const OPNOTEQUAL: &'static str = "SCMP_CMP_NE";
#[allow(dead_code)]
const OPLESSTHAN: &'static str = "SCMP_CMP_LT";
#[allow(dead_code)]
const OPLESSEQUAL: &'static str = "SCMP_CMP_LE";
#[allow(dead_code)]
const OPEQUALTO: &'static str = "SCMP_CMP_EQ";
#[allow(dead_code)]
const OPGREATEREQUAL: &'static str = "SCMP_CMP_GE";
#[allow(dead_code)]
const OPGREATERTHAN: &'static str = "SCMP_CMP_GT";
#[allow(dead_code)]
const OPMASKEDEQUAL: &'static str = "SCMP_CMP_MASKED_EQ";

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxSeccompArg {
#[serde(default)]
	index: u32,
#[serde(default)]
	value: u64,
#[serde(default, rename = "valueTwo")]
	value_two: u64,
#[serde(default)]
	op: LinuxSeccompOperator,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxSyscall {
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	names: Vec<String>,
#[serde(default, skip_serializing_if = "String::is_empty")]
	action: LinuxSeccompAction,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
	args: Vec<LinuxSeccompArg>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinuxIntelRdt {
#[serde(default, skip_serializing_if = "String::is_empty", rename = "l3CacheSchema")]
	l3_cache_schema: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
#[serde(default, skip_serializing_if = "String::is_empty", rename = "ociVersion")]
	version: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	id: String,
#[serde(default, skip_serializing_if = "String::is_empty")]
	status: String,
#[serde(default)]
	pid: i32,
#[serde(default, skip_serializing_if = "String::is_empty")]
	bundle: String,
#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	annotations: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
