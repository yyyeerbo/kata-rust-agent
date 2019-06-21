use nix::unistd::Pid;
use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::time::SystemTime;
//use oci::Spec;

#[derive(Debug)]
pub enum Status {
    // Created is the status that denotes the container exists but has not been run yet.
    Created,
    // Running is the status that denotes the container exists and is running.
    Running,
    // Pausing is the status that denotes the container exists, it is in the process of being paused.
    Pausing,
    // Paused is the status that denotes the container exists, but all its processes are paused.
    Paused,
    // Stopped is the status that denotes the container does not have a created or running process.
    Stopped,
}

#[derive(Debug)]
pub struct Process {
    id: String,
    pid: Pid,
    stdin: RawFd,
    stdout: RawFd,
    stderr: RawFd,
    console_sock: RawFd,
    term_master: RawFd,
    stdin_closed: bool,
}

#[derive(Debug)]
pub struct Container {
    pub id: String,
    root_fs: String,
    //    spec:   Spec,
    init_process: Process,
    console_master_fd: RawFd,
    use_tty: bool,
    uid_map_path: String,
    gid_map_path: String,
    state: Status,
    created: SystemTime,
    processes: HashMap<String, Process>,
}

impl Status {
    fn to_string(&self) -> String {
        match *self {
            Status::Created => "Created".to_string(),
            Status::Running => "Running".to_string(),
            Status::Pausing => "Pausing".to_string(),
            Status::Paused => "Paused".to_string(),
            Status::Stopped => "Stopped".to_string(),
        }
    }
}

impl Container {
    pub fn get_id(self) -> String {
        self.id
    }
}
