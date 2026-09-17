//! C ABI for the Android host. Each handle owns a duplicated USB descriptor.
//! The host retains UsbDeviceConnection until run returns, calls stop before
//! joining its worker, and calls free only after run has finished.
use mayhem_tcp::{config::Config, protocol::Settings, radio::Radio};
use nusb::MaybeFuture;
use std::{
    collections::VecDeque,
    os::fd::{BorrowedFd, OwnedFd},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

static LOGS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

fn log(message: &str) {
    let mut logs = LOGS.lock().unwrap_or_else(|p| p.into_inner());
    if logs.len() == 128 {
        logs.pop_front();
    }
    logs.push_back(message.chars().take(2048).collect());
}

pub struct ServerHandle {
    fd: Mutex<Option<OwnedFd>>,
    config: Config,
    stop: Arc<AtomicBool>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StartupOptions {
    frequency: u32,
    rate: u32,
    gain_tenths: i32,
    ppm: i32,
    usb_buffers: u32,
    flags: u32, // offset=1, test=2, allow antenna power=4, initial antenna power=8
}

/// # Safety
/// Both pointers must be valid. Call only before mt_run, with exclusive handle access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mt_configure(
    handle: *mut ServerHandle,
    options: *const StartupOptions,
) -> i32 {
    if handle.is_null() || options.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| -> mayhem_tcp::Result<()> {
        let options = unsafe { *options };
        let handle = unsafe { &mut *handle };
        mayhem_tcp::config::validate_usb_buffers(options.usb_buffers as usize)?;
        if options.flags & !15 != 0 || (options.flags & 8 != 0 && options.flags & 4 == 0) {
            return Err("Invalid startup flags or antenna power without opt-in".into());
        }
        let settings = Settings {
            frequency: options.frequency,
            rate: options.rate,
            gain_tenths: options.gain_tenths,
            ppm: options.ppm,
            offset_tuning: options.flags & 1 != 0,
            test_mode: options.flags & 2 != 0,
            bias_tee: options.flags & 8 != 0,
            ..handle.config.settings.clone()
        };
        settings.validate()?;
        handle.config.settings = settings;
        handle.config.usb_buffers = options.usb_buffers as usize;
        handle.config.allow_bias_tee = options.flags & 4 != 0;
        Ok(())
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            log(&format!("Startup settings rejected: {error}"));
            -1
        }
        Err(_) => {
            log("Startup configuration panicked");
            -2
        }
    }
}

/// # Safety
/// fd must be a valid, open Android USB descriptor for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mt_create(
    fd: i32,
    port: u16,
    lan: u8,
    analog: u8,
    digital: u8,
    queue_blocks: u32,
) -> *mut ServerHandle {
    mayhem_tcp::diagnostics::set_sink(log);
    let result = catch_unwind(|| -> mayhem_tcp::Result<_> {
        if fd < 0 || port == 0 {
            return Err("Invalid USB descriptor or TCP port".into());
        }
        mayhem_tcp::config::validate_queue_blocks(queue_blocks as usize)?;
        // Duplicate rather than taking ownership of Android's descriptor.
        let fd = unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?;
        let address = if lan != 0 {
            [0, 0, 0, 0]
        } else {
            [127, 0, 0, 1]
        };
        Ok(Box::new(ServerHandle {
            fd: Mutex::new(Some(fd)),
            config: Config {
                listen: (address, port).into(),
                settings: Settings {
                    auto_gain: analog != 0,
                    digital_agc: digital != 0,
                    ..Settings::default()
                },
                serial: None,
                allow_bias_tee: false,
                sessions: 0,
                service: false,
                queue_blocks: queue_blocks as usize,
                usb_buffers: mayhem_tcp::agc::USB_TRANSFERS,
                device_index: None,
            },
            stop: Arc::new(AtomicBool::new(false)),
        }))
    });
    match result {
        Ok(Ok(handle)) => Box::into_raw(handle),
        Ok(Err(error)) => {
            log(&format!("Start failed: {error}"));
            std::ptr::null_mut()
        }
        Err(_) => {
            log("Native start panicked");
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// handle must be live; call once on a worker thread, never concurrently with free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mt_run(handle: *mut ServerHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &*handle };
    let result = catch_unwind(AssertUnwindSafe(|| -> mayhem_tcp::Result<()> {
        if handle.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let fd = handle
            .fd
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
            .ok_or("Handle already used")?;
        let device = nusb::Device::from_fd(fd).wait()?;
        // Check descriptor, claim interface and read firmware before listening.
        drop(Radio::from_device(device.clone())?);
        mayhem_tcp::server::run_with_radio(handle.config.clone(), handle.stop.clone(), move || {
            Radio::from_device(device.clone())
        })
    }));
    match result {
        Ok(Ok(())) => {
            log("Server stopped");
            0
        }
        Ok(Err(error)) => {
            log(&format!("Server error: {error}"));
            -1
        }
        Err(_) => {
            log("Native server panicked; stop and reconnect the USB device");
            -2
        }
    }
}

/// # Safety
/// handle must remain live until this call returns. May run concurrently with run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mt_stop(handle: *mut ServerHandle) {
    if let Some(handle) = unsafe { handle.as_ref() } {
        handle.stop.store(true, Ordering::Relaxed);
    }
}

/// # Safety
/// Free a handle exactly once, after run and every stop call have returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mt_free(handle: *mut ServerHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Returns one UTF-8 log entry's length, or zero if empty. No NUL terminator.
/// # Safety
/// buffer must point to capacity writable bytes. Use at least 8192 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mt_log(buffer: *mut u8, capacity: usize) -> usize {
    if buffer.is_null() {
        return 0;
    }
    let mut logs = LOGS.lock().unwrap_or_else(|p| p.into_inner());
    let Some(message) = logs.front() else {
        return 0;
    };
    if message.len() > capacity {
        return 0;
    }
    let count = message.len();
    unsafe {
        std::ptr::copy_nonoverlapping(message.as_ptr(), buffer, count);
    }
    logs.pop_front();
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, os::fd::AsRawFd};

    #[test]
    fn startup_options_validate_before_mutation() {
        assert_eq!(std::mem::size_of::<StartupOptions>(), 24);
        let file = File::open("/dev/null").unwrap();
        let handle = unsafe { mt_create(file.as_raw_fd(), 12346, 1, 1, 1, 32) };
        assert!(!handle.is_null());
        let mut options = StartupOptions {
            frequency: 101_000_000,
            rate: 225_001,
            gain_tenths: 240,
            ppm: -20,
            usb_buffers: 8,
            flags: 3,
        };
        unsafe {
            assert_eq!(mt_configure(handle, &options), 0);
            assert_eq!((*handle).config.usb_buffers, 8);
            let saved = (*handle).config.settings.clone();
            assert!(saved.offset_tuning && saved.test_mode && !saved.bias_tee);
            options.flags = 8;
            assert_eq!(mt_configure(handle, &options), -1);
            assert_eq!((*handle).config.settings, saved);
            options.flags = 12;
            assert_eq!(mt_configure(handle, &options), 0);
            assert!((*handle).config.settings.bias_tee && (*handle).config.allow_bias_tee);
            options.usb_buffers = 0;
            assert_eq!(mt_configure(handle, &options), -1);
            assert_eq!((*handle).config.usb_buffers, 8);
            mt_free(handle);
        }
    }
    #[test]
    fn invalid_descriptor_is_rejected_and_reported() {
        assert!(unsafe { mt_create(-1, 12346, 1, 1, 1, 32) }.is_null());
        let mut buffer = [0_u8; 8192];
        let mut messages = String::new();
        loop {
            let length = unsafe { mt_log(buffer.as_mut_ptr(), buffer.len()) };
            if length == 0 {
                break;
            }
            messages.push_str(&String::from_utf8_lossy(&buffer[..length]));
        }
        assert!(messages.contains("Invalid USB"));
    }

    #[test]
    fn descriptor_is_duplicated_and_stop_before_run_is_safe() {
        let file = File::open("/dev/null").unwrap();
        assert!(unsafe { mt_create(file.as_raw_fd(), 12346, 1, 1, 1, 0) }.is_null());
        let handle = unsafe { mt_create(file.as_raw_fd(), 12346, 1, 1, 1, 64) };
        assert!(!handle.is_null());
        assert_eq!(unsafe { &*handle }.config.queue_blocks, 64);
        drop(file);
        // The descriptor survives closure of the caller's original handle.
        let owned = unsafe { &*handle }.fd.lock().unwrap();
        assert!(owned.as_ref().unwrap().try_clone().is_ok());
        drop(owned);
        unsafe {
            mt_stop(handle);
            mt_stop(handle);
            assert_eq!(mt_run(handle), 0);
            mt_free(handle);
        }
    }

    #[test]
    fn invalid_usb_descriptor_fails_without_unwinding_across_ffi() {
        let file = File::open("/dev/null").unwrap();
        let handle = unsafe { mt_create(file.as_raw_fd(), 12346, 0, 0, 0, 32) };
        assert!(!handle.is_null());
        unsafe {
            assert_eq!(mt_run(handle), -1);
            mt_free(handle);
        }
    }
}
