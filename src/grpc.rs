use futures::*;
use grpcio::{EnvBuilder, Server, ServerBuilder};
use grpcio::{RpcStatus, RpcStatusCode};
use std::sync::Arc;

use lazy_static;
use libcontainer::cgroups::fs::Manager as FsManager;
use libcontainer::container::{BaseContainer, LinuxContainer};
use libcontainer::cgroups::Manager as CgroupManager;
use libcontainer::process::Process;
use libcontainer::specconv::CreateOpts;
use libcontainer::errors::*;
use protocols::empty::Empty;
use protocols::agent::{WriteStreamResponse, ReadStreamResponse, GuestDetailsResponse, AgentDetails, WaitProcessResponse};
use protocols::health::{HealthCheckResponse_ServingStatus, HealthCheckResponse};
use protobuf::{RepeatedField, SingularPtrField};

use std::collections::HashMap;
use std::sync::Mutex;

use nix::unistd::{self, Pid};
use nix::sys::stat;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use libcontainer::process::ProcessOperations;

use crate::mount::{add_storages, STORAGEHANDLERLIST};
use crate::sandbox::Sandbox;
use crate::version::{AGENT_VERSION, API_VERSION};

use std::fs;
use libc::pid_t;
use std::os::unix::io::RawFd;

const SYSFS_MEMORY_BLOCK_SIZE_PATH: &'static str = "/sys/devices/system/memory/block_size_bytes";
const SYSFS_MEMORY_HOTPLUG_PROBE_PATH: &'static str = "/sys/devices/system/memory/probe";


#[derive(Clone, Default)]
struct agentService {
    sandbox: Arc<Mutex<Sandbox>>,
    test: u32,
}

lazy_static! {
    static ref CTRS: Mutex<HashMap<String, LinuxContainer<FsManager>>> = {
        let mut m = HashMap::new();
        Mutex::new(m)
    };
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
        let oci = req.OCI.as_ref().unwrap();

        let sandbox = self.sandbox.clone();
        let mut s = sandbox.lock().unwrap();

        info!("receive createcontainer {}\n", &cid);

        let _ = fs::remove_dir_all("/run/agent");

        lazy_static::initialize(&CTRS);

        let opts = CreateOpts {
            cgroup_name: "".to_string(),
            use_systemd_cgroup: false,
            no_pivot_root: false,
            no_new_keyring: false,
            spec: Some(oci.clone()),
            rootless_euid: false,
            rootless_cgroup: false,
        };

        let _ = fs::create_dir_all("/run/agent");

        let mut ctr: LinuxContainer<FsManager> = match LinuxContainer::new(cid.as_str(), "/run/agent", opts) {
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

        let mut ctr: &mut LinuxContainer<FsManager> = match s.get_container(cid.as_str()) {
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

		let _ = ctr.run(p);

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
		let eid = req.container_id.clone();
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

		let _ = p.signal(Signal::from_c_int(req.signal as i32).unwrap());

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
		pid = p.pid;

		match p.wait() {
			Ok(st) => {
				match st {
					WaitStatus::Exited(_, c) => resp.status = c,
					WaitStatus::Signaled(_, sig, _) => resp.status = sig as i32,
					_ => {
						info!("wrong status");
						let f = sink.fail(RpcStatus::new(
						RpcStatusCode::InvalidArgument,
						Some(String::from("wait error"))))
						.map_err(|_e| error!("wait error"));
						ctx.spawn(f);
						return;
					}
				}
			}
			
			Err(e) => {
				info!("wait process failed with: {}",
					e.as_errno().unwrap().desc());
				match e {
					nix::Error::Sys(Errno::ECHILD) => {
						// this definitely not right
						// the exit status is probably not
						// right. we should use subreaper
						// and thread to gather process exit 
						// status. register as subreaper,
						// get SIGCHILD and then wait to get
						// the correct status
						info!("already exited");
						resp.status = 0;
					}

					_ => {
						let f = sink.fail(RpcStatus::new(
							RpcStatusCode::InvalidArgument,
							Some(String::from("wait error"))))
						.map_err(|_e| error!("wait error"));
						ctx.spawn(f);
						return;
					}
				}
			}
		}
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
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();
		// info!("read stdout for {}/{}", cid.clone(), eid.clone());
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let vector = match read_stream(&mut sandbox, cid.as_str(),
			eid.as_str(), req.len as usize, true) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("read stream error!"))))
				.map_err(move |_e| error!(
				"read stream fail for container {} process {}",
				cid.clone(), eid.clone()));

				ctx.spawn(f);
				return;
			}
		};

		let mut resp = ReadStreamResponse::new();
		resp.set_data(vector);

		let f = sink.success(resp)
			.map_err(move |_e| error!("read error for container {} process {}", cid.clone(), eid.clone()));

		ctx.spawn(f);
    }
    fn read_stderr(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReadStreamRequest,
        sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>,
    ) {
		let cid = req.container_id.clone();
		let eid = req.exec_id.clone();
		// info!("read stderr for {}/{}", cid.clone(), eid.clone());
		let s = Arc::clone(&self.sandbox);
		let mut sandbox = s.lock().unwrap();

		let vector = match read_stream(&mut sandbox, cid.as_str(),
			eid.as_str(), req.len as usize, false) {
			Ok(v) => v,
			Err(_) => {
				let f = sink.fail(RpcStatus::new(
					RpcStatusCode::Internal,
					Some(String::from("read stream error!"))))
				.map_err(move |_e| error!(
				"read stream fail for container {} process {}",
				cid.clone(), eid.clone()));

				ctx.spawn(f);
				return;
			}
		};

		let mut resp = ReadStreamResponse::new();
		resp.set_data(vector);

		let f = sink.success(resp)
			.map_err(move |_e| error!("read error for container {} process {}", cid.clone(), eid.clone()));

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
        info!("tty_win_resize {:?} self.test={}", req, self.test);
        self.test = 1;
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
        let interface = protocols::types::Interface::new();
        let f = sink
            .success(interface)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
    fn update_routes(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::UpdateRoutesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Routes>,
    ) {
        let routes = protocols::agent::Routes::new();
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
        let interface = protocols::agent::Interfaces::new();
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
        let routes = protocols::agent::Routes::new();
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

fn read_stream(sandbox: &mut Sandbox, cid: &str, eid: &str, l: usize, stdout: bool) -> Result<Vec<u8>> {
	let p = match find_process(sandbox, cid, eid, false) {
		Ok(v) => v,
		Err(_) => return Err(ErrorKind::ErrorCode(
		"Invalid Argument!".to_string()).into()),
	};

	let fd = if p.term_master.is_some() {
		p.term_master.unwrap()
	} else {
		if stdout {
			p.parent_stdout.unwrap()
		} else {
			p.parent_stderr.unwrap()
		}
	};

	let mut v: Vec<u8> = Vec::with_capacity(l);
	unsafe { v.set_len(l); }

	match unistd::read(fd, v.as_mut_slice()) {
		Ok(len) => {
			v.resize(len, 0);
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

pub fn start<S: Into<String>>(sandbox: Sandbox, host: S, port: u16) -> Server {
    let env = Arc::new(EnvBuilder::new().build());
    let worker = agentService {
        sandbox: Arc::new(Mutex::new(sandbox)),
        test: 1,
    };
    let service = protocols::agent_grpc::create_agent_service(worker);
    let hservice = protocols::health_grpc::create_health(healthService);
    let mut server = ServerBuilder::new(env)
        .register_service(service)
        .register_service(hservice)
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
