//! A connected overlapped pipe, created entirely in the trusted parent before child launch.
//! The protected DACL excludes AppContainers. The name is never an authorization secret;
//! no listener is reopened and only the connected client handle crosses the process boundary.
use std::{
    io::{self, Read, Write},
    os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            LocalFree, SetHandleInformation, ERROR_BROKEN_PIPE, ERROR_IO_PENDING,
            ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ,
            GENERIC_WRITE, HANDLE, HANDLE_FLAG_INHERIT, HLOCAL, WAIT_OBJECT_0,
        },
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
            FILE_SHARE_MODE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION,
            SECURITY_SQOS_PRESENT,
        },
        System::{
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            },
            Threading::{CreateEventW, GetCurrentProcessId, WaitForSingleObject},
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        },
    },
};

fn error(value: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error((value.code().0 as u32 & 0xffff) as i32)
}
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: called only on newly created, exclusively owned kernel handles.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}
struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW allocates with LocalAlloc.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0 .0)));
        }
    }
}
struct Endpoint {
    handle: OwnedHandle,
    read_ms: AtomicU32,
    write_ms: AtomicU32,
}
#[derive(Clone)]
pub(super) struct Stream(Arc<Endpoint>);
impl Stream {
    pub fn pair() -> io::Result<(Self, Self)> {
        let mut random = [0u8; 24];
        getrandom::fill(&mut random)
            .map_err(|_| io::Error::other("secure randomness unavailable"))?;
        let name = format!(
            r"\\.\pipe\MonaLauncher.Auth.{}",
            random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = Descriptor(PSECURITY_DESCRIPTOR::default());
        // SAFETY: constant SDDL and valid out pointer; descriptor is released by RAII.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w!("D:P(A;;GA;;;SY)(A;;GA;;;OW)"),
                1,
                &mut descriptor.0,
                None,
            )
            .map_err(error)?;
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0 .0,
            bInheritHandle: false.into(),
        };
        // SAFETY: name and descriptor stay valid through creation; first-instance and max=1
        // refuse collisions; remote clients are rejected independently of the DACL.
        let server = owned(unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                Some(&attributes),
            )
        })?;
        // SAFETY: this trusted parent connects its client before exposing any inherited handle.
        let client = owned(unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
            .map_err(error)?
        })?;
        let event = owned(unsafe_event()?)?;
        let mut operation = OVERLAPPED {
            hEvent: raw(&event),
            ..Default::default()
        };
        // SAFETY: the client is already connected. No asynchronous connect is expected or allowed.
        let connected = unsafe { ConnectNamedPipe(raw(&server), Some(&mut operation)) };
        if let Err(failure) = connected {
            if failure.code() == ERROR_IO_PENDING.to_hresult() {
                // SAFETY: cancel and drain before the stack OVERLAPPED/event can be released.
                unsafe {
                    let _ = CancelIoEx(raw(&server), Some(&operation));
                    let mut count = 0;
                    let _ = GetOverlappedResult(raw(&server), &operation, &mut count, true);
                }
                return Err(io::Error::other(
                    "authentication pipe connection was not local",
                ));
            }
            if failure.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                return Err(error(failure));
            }
        }
        let mut client_pid = 0;
        // SAFETY: connected server handle and valid output pointer. Reject any connect race.
        unsafe {
            GetNamedPipeClientProcessId(raw(&server), &mut client_pid).map_err(error)?;
            if client_pid != GetCurrentProcessId() {
                return Err(io::Error::other("authentication pipe peer mismatch"));
            }
            SetHandleInformation(raw(&client), HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT)
                .map_err(error)?;
        }
        let make = |handle| {
            Self(Arc::new(Endpoint {
                handle,
                read_ms: AtomicU32::new(250),
                write_ms: AtomicU32::new(3000),
            }))
        };
        Ok((make(server), make(client)))
    }
    pub fn borrow_handle(&self) -> BorrowedHandle<'_> {
        self.0.handle.as_handle()
    }
    #[cfg(test)]
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
    pub fn set_read_timeout(&self, duration: Option<Duration>) -> io::Result<()> {
        self.0.read_ms.store(timeout(duration)?, Ordering::Relaxed);
        Ok(())
    }
    pub fn set_write_timeout(&self, duration: Option<Duration>) -> io::Result<()> {
        self.0.write_ms.store(timeout(duration)?, Ordering::Relaxed);
        Ok(())
    }
    fn transfer(&self, read: Option<&mut [u8]>, write: Option<&[u8]>) -> io::Result<usize> {
        let event = owned(unsafe_event()?)?;
        let mut operation = OVERLAPPED {
            hEvent: raw(&event),
            ..Default::default()
        };
        let writing = write.is_some();
        // SAFETY: buffers and OVERLAPPED stay alive until success or cancellation is drained.
        let result = unsafe {
            if let Some(bytes) = write {
                WriteFile(raw(&self.0.handle), Some(bytes), None, Some(&mut operation))
            } else {
                ReadFile(raw(&self.0.handle), read, None, Some(&mut operation))
            }
        };
        if let Err(failure) = result {
            if failure.code() != ERROR_IO_PENDING.to_hresult() {
                return transfer_error(failure, writing);
            }
        }
        let ms = if writing {
            &self.0.write_ms
        } else {
            &self.0.read_ms
        };
        // SAFETY: event belongs to this live I/O; timeout always cancels and drains.
        let wait = unsafe { WaitForSingleObject(raw(&event), ms.load(Ordering::Relaxed)) };
        let mut count = 0;
        if wait != WAIT_OBJECT_0 {
            // SAFETY: cancellation may race completion; inspect the terminal result before returning
            // so bytes completed at the deadline are not lost or written a second time.
            let completed = unsafe {
                let _ = CancelIoEx(raw(&self.0.handle), Some(&operation));
                GetOverlappedResult(raw(&self.0.handle), &operation, &mut count, true)
            };
            return match completed {
                Ok(()) => Ok(count as usize),
                Err(failure) if failure.code() == ERROR_OPERATION_ABORTED.to_hresult() => {
                    Err(io::ErrorKind::TimedOut.into())
                }
                Err(failure) => transfer_error(failure, writing),
            };
        }
        // SAFETY: event signaled completion; both operation and buffer still live.
        match unsafe { GetOverlappedResult(raw(&self.0.handle), &operation, &mut count, true) } {
            Ok(()) => Ok(count as usize),
            Err(failure) => transfer_error(failure, writing),
        }
    }
}
fn unsafe_event() -> io::Result<HANDLE> {
    // SAFETY: unnamed, non-inheritable manual-reset event, initially unsignaled.
    unsafe { CreateEventW(None, true, false, PCWSTR::null()).map_err(error) }
}
fn timeout(duration: Option<Duration>) -> io::Result<u32> {
    duration
        .and_then(|d| u32::try_from(d.as_millis()).ok())
        .filter(|&ms| ms > 0 && ms < u32::MAX)
        .ok_or_else(|| io::ErrorKind::InvalidInput.into())
}
fn transfer_error(failure: windows::core::Error, writing: bool) -> io::Result<usize> {
    if failure.code() == ERROR_BROKEN_PIPE.to_hresult()
        || failure.code() == ERROR_PIPE_NOT_CONNECTED.to_hresult()
    {
        return if writing {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(0)
        };
    }
    if failure.code() == ERROR_OPERATION_ABORTED.to_hresult() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    Err(error(failure))
}
impl Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.transfer(Some(bytes), None)
    }
}
impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.transfer(None, Some(bytes))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::GetHandleInformation;
    #[test]
    fn only_child_endpoint_is_inheritable_and_stalled_io_is_bounded() {
        let (mut parent, child) = Stream::pair().unwrap();
        for (stream, inheritable) in [(&parent, false), (&child, true)] {
            let mut flags = 0;
            // SAFETY: live endpoint and valid output pointer.
            unsafe {
                GetHandleInformation(raw(&stream.0.handle), &mut flags).unwrap();
            }
            assert_eq!(flags & HANDLE_FLAG_INHERIT.0 != 0, inheritable);
        }
        parent
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let start = std::time::Instant::now();
        assert_eq!(
            parent.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(3));
        parent
            .set_write_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let start = std::time::Instant::now();
        assert!(parent.write_all(&vec![0; 512 * 1024]).is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
