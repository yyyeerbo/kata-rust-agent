use futures::*;
use grpcio::{EnvBuilder, Server, ServerBuilder};
use grpcio::{RpcStatus, RpcStatusCode};
use std::sync::{Arc, Mutex};

use lazy_static;
use libcontainer::cgroups::fs::Manager as FsManager;
use libcontainer::container::{BaseContainer, LinuxContainer};
use libcontainer::cgroups::Manager as CgroupManager;
use libcontainer::process::Process;
use libcontainer::specconv::CreateOpts;
use libcontainer::errors::*;
use protocols::empty::Empty;
use protocols::agent::{WriteStreamResponse, ReadStreamResponse, GuestDetailsResponse, AgentDetails, WaitProcessResponse, ListProcessesResponse};
use protocols::health::{HealthCheckResponse_ServingStatus, HealthCheckResponse};
use protobuf::{RepeatedField, SingularPtrField};
use protocols::oci::{self, Spec, Linux, LinuxNamespace};

use std::collections::HashMap;

use nix::unistd::{self, Pid};
use nix::sys::stat;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use libcontainer::process::ProcessOperations;

use crate::mount::{add_storages, STORAGEHANDLERLIST};
use crate::sandbox::Sandbox;
use crate::version::{AGENT_VERSION, API_VERSION};
use crate::netlink::{RtnlHandle, NETLINK_ROUTE};
use crate::namespace::{NSTYPEIPC, NSTYPEUTS, NSTYPEPID};
use crate::device::rescan_pci_bus;

use std::fs;
use libc::{self, pid_t, TIOCSWINSZ, winsize, c_ushort};
use std::os::unix::io::RawFd;
use std::process::{Command, Stdio};
use serde_json;
use std::thread;
use std::sync::mpsc;
use std::time::Duration;

use std::fs::File;
use std::io::{BufRead, BufReader};

const SYSFS_MEMORY_BLOCK_SIZE_PATH: &'static str = "/sys/devices/system/memory/block_size_bytes";
const SYSFS_MEMORY_HOTPLUG_PROBE_PATH: &'static str = "/sys/devices/system/memory/probe";
const CONTAINER_BASE: &'static str = "/run/agent";


#[derive(Clone, Default)]
struct agentService {
    sandbox: Arc<Mutex<Sandbox>>,
    test: u32,
}

impl protocols::agent_grpc::AgentService for agentService {
    fn create_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::CreateContainerRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let cid = req.container_id.clone();
        let eid = req.exec_id.clone();

        let mut oci_spec = req.OCI.clone();
        let mut oci = oci_spec.as_mut().unwrap();

        let sandbox = self.sandbox.clone();
        let mut s = sandbox.lock().unwrap();

        info!("receive createcontainer {}\n", &cid);

		// re-scan PCI bus
		// looking for hidden devices
		match rescan_pci_bus() {
			Ok(_) => (),
			Err(e) => {
				let f = sink
					.fail(RpcStatus::new(
						RpcStatusCode::Internal,
						Some("Could not rescan PCI bus".to_string()),
					))
					.map_err(move |e| error!("fail to reply {:?}", req));
				ctx.spawn(f);
				return;
			}
		};

        update_container_namespaces(&s, oci);

        let opts = CreateOpts {
            cgroup_name: "".to_string(),
            use_systemd_cgroup: false,
            no_pivot_root: false,
            no_new_keyring: false,
            spec: Some(oci.clone()),
            rootless_euid: false,
            rootless_cgroup: false,
        };

        let mut ctr: LinuxContainer = match LinuxContainer::new(cid.as_str(), CONTAINER_BASE, opts) {
            Ok(v) => v,
            Err(_) => {
                info!("create contianer failed!\n");
                let f = sink
                    .fail(RpcStatus::new(
                        RpcStatusCode::Internal,
                        Some(format!("fail to create container {}", cid)),
                    ))
                    .map_err(move |e| error!("fail to reply {:?}", req));
                ctx.spawn(f);
                return;
            }
        };

        let p = if oci.Process.is_some() {
            let tp = match Process::new(oci.get_Process(), eid.as_str(), Vec::new(), true) {
                Ok(v) => v,
                Err(_) => {
                    info!("fail to create process!\n");
                    let f = sink
                        .fail(RpcStatus::new(
                            RpcStatusCode::Internal,
                            Some("fail to create process".to_string()),
                        ))
                        .map_err(|e| error!("process create fail"));
                    ctx.spawn(f);
                    return;
                }
            };
            tp
        } else {
            info!("no process configurations!\n");
            let f = sink
                .fail(RpcStatus::new(
                    RpcStatusCode::Internal,
                    Some("fail to create process".to_string()),
                ))
                .map_err(|e| error!("process create fail"));
            ctx.spawn(f);
            return;
        };

        if let Err(_) = ctr.start(p) {
            info!("fail to start process!\n");
            let f = sink
                .fail(RpcStatus::new(
                    RpcStatusCode::Internal,
                    Some(format!("fail to start init process {}", eid)),
                ))
                .map_err(move |e| error!("fail to start {}", eid));
            ctx.spawn(f);
            return;
        }

        s.add_container(ctr);
        info!("created container!\n");

        let resp = Empty::new();
        let f = sink
            .success(resp)
            .map_err(move |e| error!("fail to create container {}", cid));
        ctx.spawn(f);
    }

    fn start_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::StartContainerRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let cid = req.container_id.clone();

        let sandbox = self.sandbox.clone();
        let mut s = sandbox.lock().unwrap();

        let mut ctr: &mut LinuxContainer = match s.get_container(cid.as_str()) {
            Some(cr) => cr,
            None => {
                let f = sink
                    .fail(RpcStatus::new(
                        RpcStatusCode::Internal,
                        Some("fail to find container".to_string()),
                    ))
                    .map_err(move |e| error!("get container fail {}", cid.clone()));
                ctx.spawn(f);
                return;
            }
        };

        let _ = ctr.exec();

        info!("exec process!\n");

        let resp = Empty::new();
        let f = sink
            .success(resp)
            .map_err(move |e| error!("fail to create container {}", cid));
        ctx.spawn(f);
    }

    fn remove_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::RemoveContainerRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
		let cid = req.container_id.clone();
		let resp = Empty::new();

		if req.timeout == 0 {
			let s = Arc::clone(&self.sandbox);
			let mut sandbox = s.lock().unwrap();
			let ctr = sandbox.get_container(cid.as_str()).unwrap();

			ctr.destroy().unwrap();
			sandbox.containers.remove(cid.as_str());

			let f = sink.success(resp)
				.map_err(|_e| error!("cannot destroy container"));
			ctx.spawn(f);
			return;
		}

		// timeout != 0
		let s = Arc::clone(&self.sandbox);
		let cid2 = cid.clone();
		let (tx, rx) = mpsc::channel();

		let handle = thread::spawn(move || {
			let mut sandbox = s.lock().unwrap();
			let ctr = sandbox.get_container(cid2.as_str()).unwrap();

			ctr.destroy().unwrap();
			tx.send(1).unwrap();
		});

		rx.recv_timeout(Duration::from_secs(req.timeout as u64)).unwrap();
		handle.join().unwrap();

		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();
		sandbox.containers.remove(cid.as_str());

		let f = sink.success(resp)
			.map_err(|_e| error!("remove container failed"));
		ctx.spawn(f);
    }
    fn exec_process(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ExecProcessRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
		let cid = req.container_id.clone();
		let exec_id = req.exec_id.clone();

		info!("cid: {} eid: {}", cid.clone(), exec_id.clone());

		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let mut m: Vec<Option<RawFd>> = Vec::new();

		for (_, c) in &sandbox.containers {
			for (_, p) in &c.processes {
				m.push(p.term_master.clone());
				m.push(p.parent_stdin.clone());
				m.push(p.parent_stdout.clone());
				m.push(p.parent_stderr.clone());
			}
		}

		// ignore string_user, not sure what it is
		let ocip = if req.process.is_some() {
			req.process.as_ref().unwrap()
		} else {
			let f = sink.fail(RpcStatus::new(
			RpcStatusCode::InvalidArgument,
			Some(String::from("No process configuration!"))))
			.map_err(|e| error!("Invalid execprocessrequest!"));
			ctx.spawn(f);
			return;
		};

		let p = match Process::new(ocip, exec_id.as_str(), m, false) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("fail while creating process!"))))
				.map_err(|e| error!("fail to create process!"));
				ctx.spawn(f);
				return;
			}
		};

		let mut ctr = match sandbox.get_container(cid.as_str()) {
			Some(v) => v,
			None => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("no container"))))
				.map_err(move |e| error!("no container {}", cid.clone()));
				ctx.spawn(f);
				return;
			}
		};

		match ctr.run(p) {
			Err(e) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(e.to_string())))
					.map_err(move |e| error!("connt exec process {}", cid.clone()));
				ctx.spawn(f);
				return;
			},
			Ok(_) => ()
		};

		let resp = Empty::new();
		let f = sink.success(resp)
				.map_err(move |e| error!("connot exec process {}",
					exec_id.clone()));
		ctx.spawn(f);
    }
    fn signal_process(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::SignalProcessRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		info!("signal process: {}/{}", cid.clone(), eid.clone());
		let p = match find_process(&mut sandbox, cid.as_str(),
				eid.as_str(), true) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::InvalidArgument,
					Some(String::from("invalid argument"))))
				.map_err(|_e| error!("invalid argument"));
				ctx.spawn(f);
				return;
			}
		};

		let mut signal = Signal::from_c_int(req.signal as i32).unwrap();

		// For container initProcess, if it hasn't installed handler for "SIGTERM" signal,
		// it will ignore the "SIGTERM" signal sent to it, thus send it "SIGKILL" signal
		// instead of "SIGTERM" to terminate it.
		if p.init && signal == Signal::SIGTERM && !is_signal_handled(p.pid, req.signal) {
			signal = Signal::SIGKILL;
		}

		let _ = p.signal(signal);

		let resp = Empty::new();
		let f = sink.success(resp)
			.map_err(|_e| error!("cannot signal process"));
		ctx.spawn(f);
    }
    fn wait_process(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::WaitProcessRequest,
        sink: ::grpcio::UnarySink<protocols::agent::WaitProcessResponse>,
    ) {
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();
		let s = Arc::clone(&self.sandbox);
		let mut resp = WaitProcessResponse::new();
        let mut pid: pid_t = -1;
        let mut exit_pipe_r: RawFd = -1;
        let mut buf: Vec<u8> = vec![0,1];

		info!("wait process: {}/{}", cid.clone(), eid.clone());

        {
            let mut sandbox = s.lock().unwrap();

            let p = match find_process(&mut sandbox, cid.as_str(),
                                       eid.as_str(), false) {
                Ok(v) => v,
                Err(_) => {
                    let f = sink.fail(RpcStatus::new(
                        RpcStatusCode::InvalidArgument,
                        Some(String::from("invalid argument"))))
                        .map_err(|_e| error!("invalid argument"));
                    ctx.spawn(f);
                    return;
                }
            };
            if p.exit_pipe_r.is_some() {
                exit_pipe_r = p.exit_pipe_r.unwrap();
            }
            pid = p.pid;
        }

        if exit_pipe_r != -1 {
            let _ = unistd::read(exit_pipe_r, buf.as_mut_slice());
        }

		let mut sandbox = s.lock().unwrap();
		let mut ctr = sandbox.get_container(cid.as_str()).unwrap();
		// need to close all fds
		let mut p = ctr.processes.get_mut(&pid).unwrap();

		if p.parent_stdin.is_some() {
			let _ = unistd::close(p.parent_stdin.unwrap());
		}

		if p.parent_stdout.is_some() {
			let _ = unistd::close(p.parent_stdout.unwrap());
		}

		if p.parent_stderr.is_some() {
			let _ = unistd::close(p.parent_stderr.unwrap());
		}

		if p.term_master.is_some() {
			let _ = unistd::close(p.term_master.unwrap());
		}

        if p.exit_pipe_r.is_some() {
            let _ = unistd::close(p.exit_pipe_r.unwrap());
        }

		p.parent_stdin = None;
		p.parent_stdout = None;
		p.parent_stderr = None;
		p.term_master = None;

		ctr.processes.remove(&pid);

		let f = sink.success(resp)
			.map_err(|_e| error!("cannot wait process"));
		ctx.spawn(f);
    }
    fn list_processes(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ListProcessesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::ListProcessesResponse>,
    ) {
		let cid = req.container_id.clone();
		let format = req.format.clone();
		let mut args  = req.args.to_vec();
		let mut resp = ListProcessesResponse::new();

		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let ctr = sandbox.get_container(cid.as_str()).unwrap();
		let pids = ctr.processes().unwrap();

		match format.as_str() {
			"table" => {}
			"json" => {
				resp.process_list = serde_json::to_vec(&pids).unwrap();
				let f = sink.success(resp)
					.map_err(|_e| error!("cannot handle json resp"));
				ctx.spawn(f);
				return;
			}
			_ => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::InvalidArgument,
					Some(String::from("invalid format"))))
					.map_err(|_e| error!("invalid format!"));
				ctx.spawn(f);
				return;
			}
		}

		// format "table"
		if args.len() == 0 {
			// default argument
			args = vec!["-ef".to_string()];
		}

		let output = Command::new("ps")
					.args(args.as_slice())
					.stdout(Stdio::piped())
					.output()
					.expect("ps failed");

		let out: String = String::from_utf8(output.stdout).unwrap();
		let mut lines: Vec<String> = out.split('\n')
					.map(|v| v.to_string())
					.collect();

		let predicate = |v| if v == "PID" {
				return true;
		} else {
			return false;
		};

		let pid_index = lines[0].split_whitespace()
					.position(predicate).unwrap();

		let mut result = String::new();
		result.push_str(lines[0].as_str());

		lines.remove(0);
		for line in &lines {
			let fields: Vec<String> = line
				.split_whitespace()
				.map(|v| v.to_string())
				.collect();
			let pid = fields[pid_index].trim().parse::<i32>().unwrap();

			for p in &pids {
				if pid == *p {
					result.push_str(line.as_str());
				}
			}
		}

		resp.process_list = Vec::from(result);

		let f = sink.success(resp)
				.map_err(|_e| error!("list processes failed"));
		ctx.spawn(f);
    }
    fn update_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::UpdateContainerRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn stats_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::StatsContainerRequest,
        sink: ::grpcio::UnarySink<protocols::agent::StatsContainerResponse>,
    ) {
    }
    fn pause_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::PauseContainerRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn resume_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ResumeContainerRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn write_stdin(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::WriteStreamRequest,
        sink: ::grpcio::UnarySink<protocols::agent::WriteStreamResponse>,
    ) {
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();

		info!("write stdin for {}/{}", cid.clone(), eid.clone());

		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();
		let ctr = match sandbox.get_container(cid.as_str()) {
			Some(v) => v,
			None => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::InvalidArgument,
					Some(String::from("invalid cid"))))
				.map_err(move |e| error!("invalid cid {}", cid.clone()));
				ctx.spawn(f);
				return;
			}
		};

		let p = match ctr.get_process(eid.as_str()) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::InvalidArgument,
					Some(format!("invalid eid {}", eid.as_str()))))
				.map_err(move |e| error!("invalid eid {}", eid.clone()));
				ctx.spawn(f);
				return;
			}
		};

		// use ptmx io
		let fd = if p.term_master.is_some() {
			p.term_master.unwrap()
		} else {
			// use piped io
			p.parent_stdin.unwrap()
		};

		let mut l = req.data.len();
		match unistd::write(fd, req.data.as_slice()) {
			Ok(v) => {
				if v < l {
					/*
					let f = sink.fail(RpcStatus::new(
						RpcStatusCode::InvalidArgument,
						Some(format!("write error"))))
					.map_err(|_e| error!("write error"));
					ctx.spawn(f);
					return;
					*/
					info!("write {} bytes", v);
					l = v;
				}
			}
			Err(e) => {
				match e {
					nix::Error::Sys(nix::errno::Errno::EAGAIN) => l = 0,
					_ => {
					let f = sink.fail(RpcStatus::new(
						RpcStatusCode::InvalidArgument,
						Some(format!("write error"))))
					.map_err(|_e| error!("write error"));
					ctx.spawn(f);
					return;
					}
				}
			}
		}
		
		let mut resp = WriteStreamResponse::new();
		resp.set_len(l as u32);

		let f = sink.success(resp)
			.map_err(|e| error!("writestream request failed!"));

		ctx.spawn(f);
    }
    fn read_stdout(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReadStreamRequest,
        sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>,
    ) {
		let cid = req.container_id;
		let eid = req.exec_id;

        let mut fd: RawFd = -1;
		// info!("read stdout for {}/{}", cid.clone(), eid.clone());
        {
            let s = Arc::clone(&self.sandbox);
            let mut sandbox = s.lock().unwrap();

            let p = match find_process(&mut sandbox, cid.as_str(), eid.as_str(), false) {
                Ok(v) => v,
                Err(_) => {
                    let f = sink.fail(RpcStatus::new(
                        RpcStatusCode::Internal,
                        Some(String::from("invalid argument!"))))
                        .map_err(move |_e| error!(
                            "read stream failed"));
                    ctx.spawn(f);
                    return;
                }
            };

               fd = if p.term_master.is_some() {
                p.term_master.unwrap()
            } else if p.parent_stdout.is_some() {
                p.parent_stdout.unwrap()
            } else { -1 };
        }

        if fd == -1 {
            let f = sink.fail(RpcStatus::new(
                RpcStatusCode::Internal,
                Some(String::from("invalid argument!"))))
                .map_err(move |_e| error!("read stream failed"));
            ctx.spawn(f);
            return;
        }

        let vector = match read_stream(fd, cid.as_str(),
			eid.as_str(), req.len as usize) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("read stream error!"))))
				.map_err(move |_e| error!(
				"read stream failed"));

				ctx.spawn(f);
				return;
			}
		};

		let mut resp = ReadStreamResponse::new();
		resp.set_data(vector);

		let f = sink.success(resp)
			.map_err(move |_e| error!("read error for container {} process {}", cid, eid));

		ctx.spawn(f);
    }
    fn read_stderr(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReadStreamRequest,
        sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>,
    ) {
        let cid = req.container_id;
        let eid = req.exec_id;
        let mut fd: RawFd = -1;
        // info!("read stderr for {}/{}", cid.clone(), eid.clone());
        {
            let s = Arc::clone(&self.sandbox);
            let mut sandbox = s.lock().unwrap();

            let p = match find_process(&mut sandbox, cid.as_str(), eid.as_str(), false) {
                Ok(v) => v,
                Err(_) => {
                    let f = sink.fail(RpcStatus::new(
                        RpcStatusCode::Internal,
                        Some(String::from("invalid argument!"))))
                        .map_err(move |_e| error!(
                            "read stream failed"));
                    ctx.spawn(f);
                    return;
                }
            };

            fd = if p.term_master.is_some() {
                p.term_master.unwrap()
            } else if p.parent_stderr.is_some() {
                p.parent_stderr.unwrap()
            } else { -1 };
        }

        if fd == -1 {
            let f = sink.fail(RpcStatus::new(
                RpcStatusCode::Internal,
                Some(String::from("invalid argument!"))))
                .map_err(move |_e| error!(
                    "read stream failed"));
            ctx.spawn(f);
            return;
        }

		let vector = match read_stream(fd, cid.as_str(),
			eid.as_str(), req.len as usize) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("read stream error!"))))
				.map_err(move |_e| error!(
				"read stream failed"));

				ctx.spawn(f);
				return;
			}
		};

		let mut resp = ReadStreamResponse::new();
		resp.set_data(vector);

		let f = sink.success(resp)
			.map_err(move |_e| error!("read error for container {} process {}", cid, eid));

		ctx.spawn(f);
    }
    fn close_stdin(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::CloseStdinRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let p = match find_process(&mut sandbox, cid.as_str(),
				eid.as_str(), false) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::InvalidArgument,
					Some(String::from("invalid argument"))))
				.map_err(|_e| error!("invalid argument"));
				ctx.spawn(f);
				return;
			}
		};

		if p.term_master.is_some() {
			let _ = unistd::close(p.term_master.unwrap());
			p.term_master = None;
		}

		if p.parent_stdin.is_some() {
			let _ = unistd::close(p.parent_stdin.unwrap());
			p.parent_stdin = None;
		}

		let resp = Empty::new();

		let f = sink.success(resp)
			.map_err(|_e| error!("close stdin failed"));
		ctx.spawn(f);
    }

    fn tty_win_resize(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::TtyWinResizeRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();
		let p = find_process(&mut sandbox, cid.as_str(), eid.as_str(), false).unwrap();

		if p.term_master.is_none() {
			let f = sink.fail(RpcStatus::new(
				RpcStatusCode::Unavailable,
				Some("no tty".to_string())))
				.map_err(|_e| error!("tty resize"));
			ctx.spawn(f);
			return;
		}

		let fd = p.term_master.unwrap();
		unsafe {
			let win = winsize {
				ws_row: req.row as c_ushort,
				ws_col: req.column as c_ushort,
				ws_xpixel: 0,
				ws_ypixel: 0,
			};

			let err = libc::ioctl(fd, TIOCSWINSZ, &win);
			if let Err(_) = Errno::result(err).map(drop) {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some("ioctl error".to_string())))
				.map_err(|_e| error!("ioctl error!"));
				ctx.spawn(f);
				return;
			}
		}

        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn update_interface(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::UpdateInterfaceRequest,
        sink: ::grpcio::UnarySink<protocols::types::Interface>,
    ) {
        let interface = req.interface.clone();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		if sandbox.rtnl.is_none() {
			sandbox.rtnl = Some(RtnlHandle::new(NETLINK_ROUTE, 0).unwrap());
		}

		let rtnl = sandbox.rtnl.as_mut().unwrap();

		let iface = rtnl.update_interface(interface.as_ref().unwrap()).unwrap();

        let f = sink
            .success(iface)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)

    }
    fn update_routes(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::UpdateRoutesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Routes>,
    ) {
        let mut routes = protocols::agent::Routes::new();
		let rs = req.routes.clone().unwrap().Routes.into_vec();

		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		if sandbox.rtnl.is_none() {
			sandbox.rtnl = Some(RtnlHandle::new(NETLINK_ROUTE, 0).unwrap());
		}

		let rtnl = sandbox.rtnl.as_mut().unwrap();
		let v = rtnl.update_routes(rs.as_ref()).unwrap();

		routes.set_Routes(RepeatedField::from_vec(v));

        let f = sink
            .success(routes)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));

        ctx.spawn(f)
    }
    fn list_interfaces(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ListInterfacesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Interfaces>,
    ) {
        let mut interface = protocols::agent::Interfaces::new();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		if sandbox.rtnl.is_none() {
			sandbox.rtnl = Some(RtnlHandle::new(NETLINK_ROUTE, 0).unwrap());
		}

		let rtnl = sandbox.rtnl.as_mut().unwrap();
		let v = rtnl.list_interfaces().unwrap();

		interface.set_Interfaces(RepeatedField::from_vec(v));

        let f = sink
            .success(interface)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn list_routes(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ListRoutesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Routes>,
    ) {
        let mut routes = protocols::agent::Routes::new();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		if sandbox.rtnl.is_none() {
			sandbox.rtnl = Some(RtnlHandle::new(NETLINK_ROUTE, 0).unwrap());
		}

		let rtnl = sandbox.rtnl.as_mut().unwrap();

		let v = rtnl.list_routes().unwrap();

		routes.set_Routes(RepeatedField::from_vec(v));

        let f = sink
            .success(routes)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn start_tracing(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::StartTracingRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        info!("start_tracing {:?} self.test={}", req, self.test);
        self.test = 2;
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn stop_tracing(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::StopTracingRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn create_sandbox(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::CreateSandboxRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let sandbox = self.sandbox.clone();
        let mut s = sandbox.lock().unwrap();

        let mut err = "".to_string();

        let _ = fs::remove_dir_all(CONTAINER_BASE);
        let _ = fs::create_dir_all(CONTAINER_BASE);

        s.hostname = req.hostname.clone();
        s.running = true;

        if req.sandbox_id.len() > 0 {
            s.id = req.sandbox_id.clone();
        }

        match s.setup_shared_namespaces() {
            Ok(t) => (),
            Err(e) => err = e,
        }
        if err.len() != 0 {
            let rpc_status =
                grpcio::RpcStatus::new(grpcio::RpcStatusCode::FailedPrecondition, Some(err));
            let f = sink
                .fail(rpc_status)
                .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
            ctx.spawn(f);
            return;
        }

        match add_storages(req.storages.to_vec(), &mut s) {
            Ok(m) => s.mounts = m,
            Err(e) => err = e,
        };

        if err.len() != 0 {
            let rpc_status =
                grpcio::RpcStatus::new(grpcio::RpcStatusCode::FailedPrecondition, Some(err));
            let f = sink
                .fail(rpc_status)
                .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
            ctx.spawn(f);
            return;
        }

        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn destroy_sandbox(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::DestroySandboxRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();
		// destroy all containers, clean up, notify agent to exit 
		// etc.
		sandbox.destroy().unwrap();

		sandbox.sender.as_ref().unwrap().send(1).unwrap();
		sandbox.sender = None;

        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn online_cpu_mem(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::OnlineCPUMemRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn reseed_random_dev(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReseedRandomDevRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn get_guest_details(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::GuestDetailsRequest,
        sink: ::grpcio::UnarySink<protocols::agent::GuestDetailsResponse>,
    ) {
		info!("get guest details!");
		let mut resp = GuestDetailsResponse::new();
		// to get memory block size
		match get_memory_info(req.mem_block_size,
				req.mem_hotplug_probe) {
			Ok((u, v)) => {
				resp.mem_block_size_bytes = u;
				resp.support_mem_hotplug_probe = v;
			}

			Err(_) => {
				info!("fail to get memory info!");
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("internal error"))))
				.map_err(|_e| error!("cannot get memory info!"));
				ctx.spawn(f);
				return;
			}
		}

		// to get agent details
		let detail = get_agent_details();
		resp.agent_details = SingularPtrField::some(detail);

		let f = sink.success(resp)
			.map_err(|_e| error!("cannot get guest detail"));
		ctx.spawn(f);
    }
    fn mem_hotplug_by_probe(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::MemHotplugByProbeRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn set_guest_date_time(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::SetGuestDateTimeRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn copy_file(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::CopyFileRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
        let empty = protocols::empty::Empty::new();
        let f = sink
            .success(empty)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
}

#[derive(Clone)]
struct healthService;
impl protocols::health_grpc::Health for healthService {
    fn check(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::health::CheckRequest,
        sink: ::grpcio::UnarySink<protocols::health::HealthCheckResponse>,
    ) {
		let mut resp = HealthCheckResponse::new();
		resp.set_status(HealthCheckResponse_ServingStatus::SERVING);

		let f = sink.success(resp)
			.map_err(|_e| error!(
			"cannot get health status"));

		ctx.spawn(f);
    }
    fn version(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::health::CheckRequest,
        sink: ::grpcio::UnarySink<protocols::health::VersionCheckResponse>,
    ) {
        info!("version {:?}", req);
        let mut rep = protocols::health::VersionCheckResponse::new();
        rep.agent_version = AGENT_VERSION.to_string();
        rep.grpc_version = API_VERSION.to_string();
        let f = sink
            .success(rep)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
}

fn get_memory_info(block_size: bool, hotplug: bool) -> Result<(u64, bool)> {
	let mut size: u64 = 0;
	let mut plug: bool = false;
	if block_size {
		match fs::read_to_string(SYSFS_MEMORY_BLOCK_SIZE_PATH) {
			Ok(v) => {
				if v.len() == 0 {
					info!("string in empty???");
					return Err(ErrorKind::ErrorCode(
						"Invalid block size".to_string()).into());
				}

				size = v.trim().parse::<u64>()?;
			}
			Err(e) => {
				info!("memory block size error: {:?}", e.kind());
				if e.kind() != std::io::ErrorKind::NotFound {
					return Err(ErrorKind::Io(e).into());
				}
			}
		}
	}

	if hotplug {
		match stat::stat(SYSFS_MEMORY_HOTPLUG_PROBE_PATH) {
			Ok(_) => plug = true,
			Err(e) => {
				info!("hotplug memory error: {}",
					e.as_errno().unwrap().desc());
				match e {
					nix::Error::Sys(errno) => {
						match errno {
							Errno::ENOENT => plug = false,
							_ => return Err(ErrorKind::Nix(e).into()),
						}
					}
					_ => return Err(ErrorKind::Nix(e).into()),
				}
			}
		}
	}

	Ok((size, plug))
}

fn get_agent_details() -> AgentDetails {
	let mut detail = AgentDetails::new();

	detail.set_version(AGENT_VERSION.to_string());
	detail.set_supports_seccomp(false);
	detail.init_daemon = {
		unistd::getpid() == Pid::from_raw(1)
	};

	detail.device_handlers = RepeatedField::new();
	detail.storage_handlers = RepeatedField::from_vec(
							STORAGEHANDLERLIST
							.keys()
							.cloned()
							.map(|x| x.into())
							.collect());
	
	detail
}

fn read_stream(fd: RawFd, cid: &str, eid: &str, l: usize) -> Result<Vec<u8>> {
	let mut v: Vec<u8> = Vec::with_capacity(l);
	unsafe { v.set_len(l); }

	match unistd::read(fd, v.as_mut_slice()) {
		Ok(len) => {
			v.resize(len, 0);
			// Rust didn't return an EOF error when the reading peer point
			// was closed, instead it would return a 0 reading length, please
			// see https://github.com/rust-lang/rfcs/blob/master/text/0517-io-os-reform.md#errors
			if len  == 0 {
				return Err(ErrorKind::ErrorCode("read  meet eof".to_string()).into());
			}
		}
		Err(e) => {
			match e {
				nix::Error::Sys(errno) => {
					match errno {
						Errno::EAGAIN => v.resize(0, 0),
						_ => return Err(ErrorKind::Nix(
							nix::Error::Sys(errno)).into()),
					}
				}
				_ => return Err(ErrorKind::ErrorCode(
				"read error".to_string()).into()),
			}
		}
	}

	Ok(v)
}

fn find_process<'a>(sandbox: &'a mut Sandbox, cid: &'a str, eid: &'a str, init: bool) -> Result<&'a mut Process> {
	let ctr = match sandbox.get_container(cid) {
		Some(v) => v,
		None => return Err(ErrorKind::ErrorCode(
			String::from("Invalid container id")).into()),
	};

	if init && eid == "" {
		let p = match ctr.processes.get_mut(&ctr.init_process_pid) {
			Some(v) => v,
			None =>  return Err(ErrorKind::ErrorCode(
				String::from("cannot find init process!")).into()),
		};

		return Ok(p);
	}

	let p = match ctr.get_process(eid) {
		Ok(v) => v,
		Err(_) => return Err(ErrorKind::ErrorCode(
			"Invalid exec id".to_string()).into()),
	};

	Ok(p)
}

pub fn start<S: Into<String>>(sandbox: Arc<Mutex<Sandbox>>, host: S, port: u16) -> Server {
    let env = Arc::new(EnvBuilder::new()
	.cq_count(1)
        .wait_thread_count_default(5)
        .wait_thread_count_min(1)
        .wait_thread_count_max(10)
	.build());
    let worker = agentService {
        sandbox: sandbox,
        test: 1,
    };
    let service = protocols::agent_grpc::create_agent_service(worker);
    let hservice = protocols::health_grpc::create_health(healthService);
    let mut server = ServerBuilder::new(env)
        .register_service(service)
        .register_service(hservice)
		.requests_slot_per_cq(1024)
        .bind(host, port)
        .build()
        .unwrap();
    server.start();
    info!("gRPC server started");
    for &(ref host, port) in server.bind_addrs() {
        info!("listening on {}:{}", host, port);
    }

    server
}

// This function updates the container namespaces configuration based on the
// sandbox information. When the sandbox is created, it can be setup in a way
// that all containers will share some specific namespaces. This is the agent
// responsibility to create those namespaces so that they can be shared across
// several containers.
// If the sandbox has not been setup to share namespaces, then we assume all
// containers will be started in their own new namespace.
// The value of a.sandbox.sharedPidNs.path will always override the namespace
// path set by the spec, since we will always ignore it. Indeed, it makes no
// sense to rely on the namespace path provided by the host since namespaces
// are different inside the guest.
fn update_container_namespaces(sandbox: &Sandbox, spec: &mut Spec) -> Result<()> {
    let mut linux = match spec.Linux.as_mut() {
        None => return Err(ErrorKind::ErrorCode("Spec didn't container linux field".to_string()).into()),
        Some(l) => l
    };

	let mut pidNs = false;

    let mut namespaces = linux.Namespaces.as_mut_slice();
    for namespace in namespaces.iter_mut() {
		if namespace.Type == NSTYPEPID {
			pidNs = true;
			continue
		}
        if namespace.Type == NSTYPEIPC {
            namespace.Path = sandbox.shared_ipcns.path.clone();
			continue
        }
        if namespace.Type == NSTYPEUTS {
            namespace.Path = sandbox.shared_utsns.path.clone();
			continue
        }
    };

	if !pidNs && !sandbox.sandbox_pid_ns {
		let mut pid_ns = LinuxNamespace::new();
		pid_ns.set_Type(NSTYPEPID.to_string());
		linux.Namespaces.push(pid_ns);
	}

    Ok(())
}

// Check is the container process installed the
// handler for specific signal.
fn is_signal_handled(pid: pid_t, signum: u32) -> bool {
	let sig_mask: u64 = 1u64 << (signum - 1);
	let file_name = format!("/proc/{}/status", pid);

	// Open the file in read-only mode (ignoring errors).
	let file = match File::open(&file_name) {
		Ok(f) => f,
		Err(e) => {
			warn!("failed to open file {}\n", file_name);
			return false;
		}
	};

	let reader = BufReader::new(file);

	// Read the file line by line using the lines() iterator from std::io::BufRead.
	for (index, line) in reader.lines().enumerate() {
		let line = match line {
			Ok(l) => l,
			Err(e) => {
				warn!("failed to read file {}\n", file_name);
				return false;
			}
		};
		if line.starts_with("SigCgt:") {
			let mask_vec: Vec<&str> = line.split(":").collect();
			if mask_vec.len() != 2 {
				warn!("parse the SigCgt field failed\n");
				return false;
			}
			let sig_cgt_str = mask_vec[1];
			let sig_cgt_mask = match u64::from_str_radix(sig_cgt_str, 16) {
				Ok(h) => h,
				Err(e) => {
					warn!("failed to parse the str {} to hex\n", sig_cgt_str);
					return false;
				}
			};

			return (sig_cgt_mask & sig_mask) == sig_mask;
		}
	}
	false
}
