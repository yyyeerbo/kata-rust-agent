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
use protocols::empty::Empty;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::mount::add_storages;
use crate::sandbox::Sandbox;

use std::fs;

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
            let tp = match Process::new(oci.get_Process(), eid.as_str(), true) {
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
    }
    fn signal_process(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::SignalProcessRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn wait_process(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::WaitProcessRequest,
        sink: ::grpcio::UnarySink<protocols::agent::WaitProcessResponse>,
    ) {
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
    }
    fn read_stdout(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReadStreamRequest,
        sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>,
    ) {
    }
    fn read_stderr(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReadStreamRequest,
        sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>,
    ) {
    }
    fn close_stdin(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::CloseStdinRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
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
    }
    fn update_routes(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::UpdateRoutesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Routes>,
    ) {
    }
    fn list_interfaces(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ListInterfacesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Interfaces>,
    ) {
    }
    fn list_routes(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ListRoutesRequest,
        sink: ::grpcio::UnarySink<protocols::agent::Routes>,
    ) {
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
    }
    fn online_cpu_mem(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::OnlineCPUMemRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn reseed_random_dev(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::ReseedRandomDevRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn get_guest_details(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::GuestDetailsRequest,
        sink: ::grpcio::UnarySink<protocols::agent::GuestDetailsResponse>,
    ) {
    }
    fn mem_hotplug_by_probe(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::MemHotplugByProbeRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn set_guest_date_time(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::SetGuestDateTimeRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
    }
    fn copy_file(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::agent::CopyFileRequest,
        sink: ::grpcio::UnarySink<protocols::empty::Empty>,
    ) {
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
    }
    fn version(
        &mut self,
        ctx: ::grpcio::RpcContext,
        req: protocols::health::CheckRequest,
        sink: ::grpcio::UnarySink<protocols::health::VersionCheckResponse>,
    ) {
        info!("version {:?}", req);
        let mut rep = protocols::health::VersionCheckResponse::new();
        rep.agent_version = "a1".to_string();
        rep.grpc_version = "g2".to_string();
        let f = sink
            .success(rep)
            .map_err(move |e| error!("failed to reply {:?}: {:?}", req, e));
        ctx.spawn(f)
    }
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
