use super::*;
use std::{ffi::OsString, time::Duration};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

define_windows_service!(service_entry, service_main);

pub fn dispatch() -> Result<()> {
    service_dispatcher::start("mayhem_tcp", service_entry)?;
    Ok(())
}
fn service_main(_args: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("Service failed: {error}");
    }
}
fn run_service() -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = shutdown.clone();
    let handle = service_control_handler::register("mayhem_tcp", move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            signal.store(true, Ordering::Relaxed);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    let status = |state, code| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(code),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    handle.set_service_status(status(ServiceState::Running, 0))?;
    let result = Config::parse()
        .and_then(|config| server::run(config.ok_or("Missing service configuration")?, shutdown));
    handle.set_service_status(status(
        ServiceState::Stopped,
        if result.is_ok() { 0 } else { 1 },
    ))?;
    result
}
