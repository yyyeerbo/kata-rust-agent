use serde;
#[macro_use]
use serde_derive;
use serde_json;

use std::collections::HashMap;
#[macro_use]
use lazy_static;


type NameSpaceType = String;
type Namespaces = Vec<Namespace>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Namespace {
#[serde(default)]
	r#type: NamespaceType,
#[serde(default)]
	path: String,
}

const NEWNET: &'static str = "NEWNET";
const NEWPID: &'static str = "NEWPID";
const NEWNS: &'static str = "NEWNS";
const NEWUTS: &'static str = "NEWUTS";
const NEWUSER: &'static str = "NEWUSER";
const NEWCGROUP: &'static str = "NEWCGROUP";
const NEWIPC: &'static str = "NEWIPC";

lazy_static! {
	static ref TYPETONAME: HashMap<&'static str, &'static str> = {
		let mut m = HashMap::new();
		m.insert("pid", "pid");
		m.insert("network", "net");
		m.insert("mount", "mnt");
		m.insert("user", "user");
		m.insert("uts", "uts");
		m.insert("ipc", "ipc");
		m.insert("cgroup", "cgroup");
		m
	};
}
