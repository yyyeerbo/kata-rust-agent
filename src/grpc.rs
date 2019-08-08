// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

use futures::*;
use grpcio::{EnvBuilder, Server, ServerBuilder};
use grpcio::{RpcStatus, RpcStatusCode};
use std::sync::{Arc, Mutex};

use lazy_static;
use rustjail::cgroups::fs::Manager as FsManager;
use rustjail::container::{BaseContainer, LinuxContainer};
use rustjail::cgroups::Manager as CgroupManager;
use rustjail::process::Process;
use rustjail::specconv::CreateOpts;
use rustjail::errors::*;
use rustjail;
use protocols::empty::Empty;
use protocols::agent::{WriteStreamResponse, ReadStreamResponse, GuestDetailsResponse, AgentDetails, WaitProcessResponse, ListProcessesResponse};
use protocols::health::{HealthCheckResponse_ServingStatus, HealthCheckResponse};
use protobuf::{RepeatedField, SingularPtrField};
use protocols::oci::{self, Spec, Linux, LinuxNamespace};
use protocols::agent::{CopyFileRequest};

use std::collections::HashMap;

use nix::unistd::{self, Pid};
use nix::sys::stat;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use rustjail::process::ProcessOperations;

use crate::mount::{add_storages, remove_mounts, STORAGEHANDLERLIST};
use crate::sandbox::Sandbox;
use crate::version::{AGENT_VERSION, API_VERSION};
use crate::netlink::{RtnlHandle, NETLINK_ROUTE};
use crate::namespace::{NSTYPEIPC, NSTYPEUTS, NSTYPEPID};
use crate::device::rescan_pci_bus;
use crate::random;

use std::fs;
use libc::{self, pid_t, TIOCSWINSZ, winsize, c_ushort};
use std::os::unix::io::RawFd;
use std::process::{Command, Stdio};
use serde_json;
use std::thread;
use std::sync::mpsc;
use std::time::Duration;

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use nix::unistd::{Uid, Gid};
use ::oci::{Spec as OCISpec};

const SYSFS_MEMORY_BLOCK_SIZE_PATH: &'static str = "/sys/devices/system/memory/block_size_bytes";
const SYSFS_MEMORY_HOTPLUG_PROBE_PATH: &'static str = "/sys/devices/system/memory/probe";
pub const SYSFS_MEMORY_ONLINE_PATH: &'static str = "/sys/devices/system/memory";
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

		let sandbox;
		let mut s;

        info!("receive createcontainer {}\n", &cid);

		// re-scan PCI bus
		// looking for hidden devices


		match rescan_pci_bus().chain_err(|| "Could not rescan PCI bus") {
			Ok(_) => (),
			Err(e) => {
				let f = sink
					.fail(RpcStatus::new(
						RpcStatusCode::Internal,
						Some(e.to_string()),
					))
					.map_err(move |e| error!("fail to reply {:?}", req));
				ctx.spawn(f);
				return;
			}
		};

		// Both rootfs and volumes (invoked with --volume for instance) will
		// be processed the same way. The idea is to always mount any provided
		// storage to the specified MountPoint, so that it will match what's
		// inside oci.Mounts.
		// After all those storages have been processed, no matter the order
		// here, the agent will rely on rustjail (using the oci.Mounts
		// list) to bind mount all of them inside the container.
		match add_storages(req.storages.to_vec(), self.sandbox.clone()) {
			Ok(m) => {
				sandbox = self.sandbox.clone();
				s = sandbox.lock().unwrap();
				s.container_mounts.insert(cid.clone(), m);
			},
			Err(e) => {
				let f = sink
					.fail(RpcStatus::new(
						RpcStatusCode::Internal,
						Some(format!("failed to add storage to container: {:?}", e)),
					))
					.map_err(move |e| error!("fail to reply {:?}", req));
				ctx.spawn(f);
				return;
			}
		};

		{
        	let mut oci = oci_spec.as_mut().unwrap();
        	update_container_namespaces(&s, oci);
		}

		let oci = oci_spec.as_ref().unwrap();

		// write spec to bundle path, hooks might
		// read ocispec
		let _ = setup_bundle(oci);

        let opts = CreateOpts {
            cgroup_name: "".to_string(),
            use_systemd_cgroup: false,
            no_pivot_root: s.no_pivot_root,
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
		let mut cmounts: Vec<String> = vec![];

		if req.timeout == 0 {
			let s = Arc::clone(&self.sandbox);
			let mut sandbox = s.lock().unwrap();
			let ctr = sandbox.get_container(cid.as_str()).unwrap();

			ctr.destroy().unwrap();

			// Find the sandbox storage used by this container
			let mounts = sandbox.container_mounts.get(&cid);
			if mounts.is_some() {
				let mounts =  mounts.unwrap();

				match remove_mounts(&mounts) {
					Ok(_) => (),
					Err(e) => {
						let f = sink
							.fail(RpcStatus::new(
								RpcStatusCode::Internal,
								Some(format!("fail to umount container mounts: {:?}", e)),
							))
							.map_err(move |e| error!("get container fail {}", cid.clone()));
						ctx.spawn(f);
						return;
					}
				}

				for m in mounts.iter() {
					if sandbox.storages.get(m).is_some() {
						cmounts.push(m.to_string());
					}
				}
			}

			for m in cmounts.iter() {
				match sandbox.unset_and_remove_sandbox_storage(m) {
					Ok(_) => (),
					Err(e) => {
						let f = sink
							.fail(RpcStatus::new(
								RpcStatusCode::Internal,
								Some(format!("fail to remove container storage: {:?}", e)),
							))
							.map_err(move |e| error!("get container fail {}", cid.clone()));
						ctx.spawn(f);
						return;
					}
				}
			}

			sandbox.container_mounts.remove(cid.as_str());
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

		// Find the sandbox storage used by this container
		let mounts = sandbox.container_mounts.get(&cid);
		if mounts.is_some() {
			let mounts =  mounts.unwrap();

			match remove_mounts(&mounts) {
				Ok(_) => (),
				Err(e) => {
					let f = sink
						.fail(RpcStatus::new(
							RpcStatusCode::Internal,
							Some(format!("fail to umount container mounts: {:?}", e)),
						))
						.map_err(move |e| error!("get container fail {}", cid.clone()));
					ctx.spawn(f);
					return;
				}
			}

			for m in mounts.iter() {
				if sandbox.storages.get(m).is_some() {
					cmounts.push(m.to_string());
				}
			}
		}

		for m in cmounts.iter() {
			match sandbox.unset_and_remove_sandbox_storage(m) {
				Ok(_) => (),
				Err(e) => {
					let f = sink
						.fail(RpcStatus::new(
							RpcStatusCode::Internal,
							Some(format!("fail to remove container storage: {:?}", e)),
						))
						.map_err(move |e| error!("get container fail {}", cid.clone()));
					ctx.spawn(f);
					return;
				}
			}
		}

		sandbox.container_mounts.remove(&cid);
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
		let mut args  = req.args.clone().into_vec();
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
			if line.trim().is_empty() {
				continue;
			}

			let fields: Vec<String> = line
				.split_whitespace()
				.map(|v| v.to_string())
				.collect();

			if fields.len() < pid_index + 1 {
				warn!("corrupted output?");
				continue;
			}
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
		let cid = req.container_id.clone();
		let res = req.resources.clone();

		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let ctr = sandbox.get_container(cid.as_str()).unwrap();

		let resp = Empty::new();

		if res.is_some() {
			match ctr.set(res.unwrap()) {
				Err(e) => {
					let f = sink.fail(RpcStatus::new(
						RpcStatusCode::Internal,
						Some("internal error".to_string())))
						.map_err(|_e| error!("internal error!"));
					ctx.spawn(f);
					return;
				}

				Ok(()) => {}
			}
		}

		let f = sink.success(resp)
			.map_err(|_e| error!("update container failed!"));

		ctx.spawn(f);
    }
    fn stats_container(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::StatsContainerRequest,
        sink: ::grpcio::UnarySink<protocols::agent::StatsContainerResponse>,
    ) {
		let cid = req.container_id.clone();
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let ctr = sandbox.get_container(cid.as_str()).unwrap();

		let resp = match ctr.stats() {
			Err(e) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some("internal error!".to_string())))
					.map_err(|_e| error!("internal error!"));
				ctx.spawn(f);
				return;
			}

			Ok(r) => r,
		};

		let f = sink.success(resp)
				.map_err(|_e| error!("stats containers failed!"));
		ctx.spawn(f);
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
		let mut err = "".to_string();

		{
			let sandbox = self.sandbox.clone();
			let mut s = sandbox.lock().unwrap();

			let _ = fs::remove_dir_all(CONTAINER_BASE);
			let _ = fs::create_dir_all(CONTAINER_BASE);

			s.hostname = req.hostname.clone();
			s.running = true;

			if req.sandbox_id.len() > 0 {
				s.id = req.sandbox_id.clone();
			}

			match s.setup_shared_namespaces() {
				Ok(t) => (),
				Err(e) => err = e.to_string(),
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
		}

        match add_storages(req.storages.to_vec(), self.sandbox.clone()) {
            Ok(m) => {
				let sandbox = self.sandbox.clone();
				let mut s = sandbox.lock().unwrap();
				s.mounts = m
			},
            Err(e) => err = e.to_string(),
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
		// sleep 5 seconds for debug
		// thread::sleep(Duration::new(5, 0));
		let s = Arc::clone(&self.sandbox);
		let sandbox = s.lock().unwrap();
        let empty = protocols::empty::Empty::new();

		if let Err(e) = sandbox.online_cpu_memory(&req) {
			let f = sink.fail(RpcStatus::new(
				RpcStatusCode::Internal,
				Some("Internal error".to_string())))
				.map_err(|_e| error!("cannot online memory/cpu"));
			ctx.spawn(f);
			return;
		} 

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
		if let Err(e) = random::reseed_rng(req.data.as_slice()) {
			let f = sink.fail(RpcStatus::new(
				RpcStatusCode::Internal,
				Some("Internal error".to_string())))
				.map_err(|_e| error!("fail to reseed rng!"));
			ctx.spawn(f);
			return;
		}

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

		if let Err(e) = do_mem_hotplug_by_probe(&req.memHotplugProbeAddr) {
			let f = sink.fail(RpcStatus::new(
				RpcStatusCode::Internal,
				Some("internal error!".to_string())))
				.map_err(|_e| error!("cannont mem hotplug by probe!"));
			ctx.spawn(f);
			return;
		}

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
		if let Err(e) = do_set_guest_date_time(req.Sec, req.Usec) {
			let f = sink.fail(RpcStatus::new(
				RpcStatusCode::Internal,
				Some("internal error!".to_string())))
				.map_err(|_e| error!("cannot set guest time!"));
			ctx.spawn(f);
			return;
		}

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
		if let Err(e) = do_copy_file(&req) {
			let f = sink.fail(RpcStatus::new(
				RpcStatusCode::Internal,
				Some("Internal error!".to_string())))
				.map_err(|_e| error!("cannot copy file!"));
			ctx.spawn(f);
			return;
		}

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

fn do_mem_hotplug_by_probe(addrs: &Vec<u64>) -> Result<()> {
	for addr in addrs.iter() {
		fs::write(SYSFS_MEMORY_HOTPLUG_PROBE_PATH, format!("{:#X}", *addr))?;
	}
	Ok(())
}

fn do_set_guest_date_time(sec: i64, usec: i64) -> Result<()> {
	let tv = libc::timeval {
		tv_sec: sec,
		tv_usec: usec,
	};

	let ret = unsafe { libc::settimeofday(&tv as *const libc::timeval,
		0 as *const libc::timezone) };
	
	Errno::result(ret).map(drop)?;

	Ok(())
}

fn do_copy_file(req: &CopyFileRequest) -> Result<()> {
	let path = fs::canonicalize(req.path.as_str())?;

	if !path.starts_with(CONTAINER_BASE) {
		return Err(nix::Error::Sys(Errno::EINVAL).into());
	}

	let parent = path.parent();

	let dir = if parent.is_some() {
		parent.unwrap().to_path_buf()
	} else {
		PathBuf::from("/")
	};

	if let Err(e) = fs::create_dir_all(dir.to_str().unwrap()) {
		if e.kind() != std::io::ErrorKind::AlreadyExists {
			return Err(e.into());
		}
	}

	let ret = unsafe { libc::chmod(dir.to_str().unwrap().as_ptr() as *const libc::c_char, req.dir_mode) };

	let _ = Errno::result(ret).map(drop)?;

	let mut tmpfile = path.clone();
	tmpfile.set_extension("tmp");

	let file = OpenOptions::new()
				.write(true)
				.create(true)
				.truncate(false)
				.open(tmpfile.to_str().unwrap())?;
	file.write_all_at(req.data.as_slice(), req.offset as u64)?;

	let st = stat::stat(tmpfile.to_str().unwrap())?;

	if st.st_size != req.file_size {
		return Ok(());
	}

	let ret = unsafe { libc::chmod(tmpfile.to_str().unwrap().as_ptr() as *const libc::c_char, req.file_mode) };

	let _ = Errno::result(ret).map(drop)?;
	unistd::chown(tmpfile.to_str().unwrap(), Some(Uid::from_raw(req.uid as u32)), Some(Gid::from_raw(req.gid as u32)))?;

	fs::rename(tmpfile, path)?;

	Ok(())
}

fn setup_bundle(gspec: &Spec) -> Result<()>{
	if gspec.Root.is_none() {
		return Err(nix::Error::Sys(Errno::EINVAL).into());
	}
	let root = gspec.Root.as_ref().unwrap().Path.as_str();

	let rootfs = fs::canonicalize(root)?;
	let bundle_path = rootfs.parent().unwrap().to_str().unwrap();

	let config = format!("{}/{}", bundle_path, "config.json");

	let oci = rustjail::grpc_to_oci(gspec);
	info!("{:?}", oci.process.as_ref().unwrap().console_size.as_ref());
	let _ = oci.save(config.as_str());

	unistd::chdir(bundle_path)?;

	Ok(())
}
