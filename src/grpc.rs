use std::sync::Arc;

use grpcio::{ServerBuilder, EnvBuilder, Server};

#[derive(Clone)]
struct agentService;

impl protocols::agent_grpc::AgentService for agentService {
    fn create_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::CreateContainerRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn start_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::StartContainerRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn remove_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::RemoveContainerRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn exec_process(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ExecProcessRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn signal_process(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::SignalProcessRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn wait_process(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::WaitProcessRequest, sink: ::grpcio::UnarySink<protocols::agent::WaitProcessResponse>) {}
    fn list_processes(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ListProcessesRequest, sink: ::grpcio::UnarySink<protocols::agent::ListProcessesResponse>) {}
    fn update_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::UpdateContainerRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn stats_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::StatsContainerRequest, sink: ::grpcio::UnarySink<protocols::agent::StatsContainerResponse>) {}
    fn pause_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::PauseContainerRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn resume_container(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ResumeContainerRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn write_stdin(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::WriteStreamRequest, sink: ::grpcio::UnarySink<protocols::agent::WriteStreamResponse>) {}
    fn read_stdout(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ReadStreamRequest, sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>) {}
    fn read_stderr(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ReadStreamRequest, sink: ::grpcio::UnarySink<protocols::agent::ReadStreamResponse>) {}
    fn close_stdin(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::CloseStdinRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn tty_win_resize(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::TtyWinResizeRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn update_interface(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::UpdateInterfaceRequest, sink: ::grpcio::UnarySink<protocols::types::Interface>) {}
    fn update_routes(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::UpdateRoutesRequest, sink: ::grpcio::UnarySink<protocols::agent::Routes>) {}
    fn list_interfaces(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ListInterfacesRequest, sink: ::grpcio::UnarySink<protocols::agent::Interfaces>) {}
    fn list_routes(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ListRoutesRequest, sink: ::grpcio::UnarySink<protocols::agent::Routes>) {}
    fn start_tracing(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::StartTracingRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn stop_tracing(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::StopTracingRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn create_sandbox(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::CreateSandboxRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn destroy_sandbox(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::DestroySandboxRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn online_cpu_mem(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::OnlineCPUMemRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn reseed_random_dev(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::ReseedRandomDevRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn get_guest_details(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::GuestDetailsRequest, sink: ::grpcio::UnarySink<protocols::agent::GuestDetailsResponse>) {}
    fn mem_hotplug_by_probe(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::MemHotplugByProbeRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn set_guest_date_time(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::SetGuestDateTimeRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
    fn copy_file(&mut self, ctx: ::grpcio::RpcContext, req: protocols::agent::CopyFileRequest, sink: ::grpcio::UnarySink<protocols::empty::Empty>) {}
}

pub fn start<S: Into<String>>(host: S, port: u16) -> Server {
    let env = Arc::new(EnvBuilder::new().build());
    let service = protocols::agent_grpc::create_agent_service(agentService);
    let mut server = ServerBuilder::new(env)
        .register_service(service)
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
