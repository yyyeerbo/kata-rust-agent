use libcontainer::cgroups::Stats as CgroupStats;
use libcontainer::intelrdt::Stats as RdtStats;


pub struct NetworkInterface {
	name: String,
	rx_bytes: u64,
	rx_packets: u64,
	rx_errors: u64,
	rx_dropped: u64,
	tx_bytes: u64,
	tx_packets: u64,
	tx_errors: u64,
	tx_dropped: u64,
}

pub struct Stats {
	interfaces: Vec<NetworkInterface>,
	cgroup_stats: Option<CgroupStats>,
	intel_rdt_stats: Option<RdtStats>,
}
