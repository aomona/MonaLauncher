use super::{
    protocol::{self, BrokerError, Command, Request},
    service::Operations,
};
use serde_json::json;
use std::{
    io::{self, Read, Write},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::Command as ProcessCommand,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const CHILD_FD: i32 = 3;

pub struct PreparedBroker {
    child: UnixStream,
    guard: BrokerGuard,
}

pub struct BrokerGuard {
    active: Arc<AtomicBool>,
    shutdown: UnixStream,
    worker: Option<JoinHandle<()>>,
}

impl Drop for BrokerGuard {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        let _ = self.shutdown.shutdown(Shutdown::Both);
        // An in-flight HTTPS request is bounded by its own 30s deadline. It may finish on its
        // worker, but the active/generation check prevents any reply after revocation.
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            let _ = self.worker.take().expect("worker checked").join();
        }
    }
}

impl PreparedBroker {
    pub fn new(operations: Box<dyn Operations>, network: bool) -> io::Result<Self> {
        let (parent, child) = UnixStream::pair()?;
        parent.set_read_timeout(Some(Duration::from_millis(250)))?;
        parent.set_write_timeout(Some(Duration::from_secs(3)))?;
        let shutdown = parent.try_clone()?;
        let shutdown_on_exit = parent.try_clone()?;
        let active = Arc::new(AtomicBool::new(true));
        let running = Arc::clone(&active);
        let worker = std::thread::Builder::new()
            .name("minecraft-auth-broker".into())
            .spawn(move || {
                let _ = serve(parent, operations, network, running);
                let _ = shutdown_on_exit.shutdown(Shutdown::Both);
            })?;
        Ok(Self {
            child,
            guard: BrokerGuard {
                active,
                shutdown,
                worker: Some(worker),
            },
        })
    }

    pub fn configure(&self, command: &mut ProcessCommand) -> io::Result<()> {
        let child = self.child.try_clone()?;
        // SAFETY: dup2 and fcntl are async-signal-safe. The captured owned socket keeps its
        // source FD alive until exec; only one specific child descriptor becomes inheritable.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(child.as_raw_fd(), CHILD_FD) == -1
                    || libc::fcntl(CHILD_FD, libc::F_SETFD, 0) == -1
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }

    pub fn into_guard(self) -> BrokerGuard {
        self.guard
    }

    #[cfg(test)]
    fn test_peer(&self) -> UnixStream {
        self.child.try_clone().unwrap()
    }
}

fn read_exact_until(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    active: &AtomicBool,
    deadline: Option<Instant>,
) -> io::Result<()> {
    while !bytes.is_empty() {
        if !active.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if deadline.is_some_and(|end| Instant::now() >= end) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        match stream.read(bytes) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => bytes = &mut bytes[n..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn serve(
    mut stream: UnixStream,
    mut operations: Box<dyn Operations>,
    network: bool,
    active: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut previous_id = 0;
    let mut greeted = false;
    let mut window = Instant::now();
    let mut requests = 0;
    loop {
        let mut header = [0; 4];
        read_exact_until(&mut stream, &mut header[..1], &active, None)?;
        let deadline = Some(Instant::now() + Duration::from_secs(5));
        read_exact_until(&mut stream, &mut header[1..], &active, deadline)?;
        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > protocol::MAX_REQUEST {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut bytes = vec![0; length];
        read_exact_until(&mut stream, &mut bytes, &active, deadline)?;
        let request: Request =
            serde_json::from_slice(&bytes).map_err(|_| io::ErrorKind::InvalidData)?;
        if window.elapsed() >= Duration::from_secs(1) {
            window = Instant::now();
            requests = 0;
        }
        requests += 1;
        let result = if request.id <= previous_id {
            Err(BrokerError::InvalidRequest)
        } else if !active.load(Ordering::Acquire) || !operations.valid() {
            Err(BrokerError::Revoked)
        } else if requests > 16 {
            Err(BrokerError::RateLimited)
        } else if matches!(request.command, Command::Hello {}) {
            greeted = true;
            Ok(json!({ "protocol": protocol::PROTOCOL_VERSION }))
        } else if !greeted {
            Err(BrokerError::InvalidRequest)
        } else if !network {
            Err(BrokerError::NetworkDenied)
        } else {
            operations.execute(&request.command)
        };
        previous_id = previous_id.max(request.id);
        let result = if !active.load(Ordering::Acquire) || !operations.valid() {
            Err(BrokerError::Revoked)
        } else {
            result
        };
        let mut reply = protocol::response(request.id, result);
        if reply.len() > protocol::MAX_RESPONSE {
            reply = protocol::response(request.id, Err(BrokerError::InvalidResponse));
        }
        stream.write_all(&(reply.len() as u32).to_be_bytes())?;
        stream.write_all(&reply)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestOperations(Arc<AtomicBool>);
    impl Operations for TestOperations {
        fn valid(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
        fn execute(&mut self, _: &Command) -> Result<serde_json::Value, BrokerError> {
            Ok(json!({"called":true}))
        }
    }
    fn request(peer: &mut UnixStream, value: serde_json::Value) -> serde_json::Value {
        let body = serde_json::to_vec(&value).unwrap();
        peer.write_all(&(body.len() as u32).to_be_bytes()).unwrap();
        peer.write_all(&body).unwrap();
        let mut len = [0; 4];
        peer.read_exact(&mut len).unwrap();
        let mut response = vec![0; u32::from_be_bytes(len) as usize];
        peer.read_exact(&mut response).unwrap();
        serde_json::from_slice(&response).unwrap()
    }
    #[test]
    fn rejects_replays_and_rate_limits_while_channels_remain_independent() {
        let valid = Arc::new(AtomicBool::new(true));
        let first = PreparedBroker::new(Box::new(TestOperations(valid.clone())), true).unwrap();
        let second = PreparedBroker::new(Box::new(TestOperations(valid)), true).unwrap();
        let mut a = first.test_peer();
        let mut b = second.test_peer();
        for peer in [&mut a, &mut b] {
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            assert!(request(peer, json!({"id":1,"command":{"type":"hello"}}))
                .get("result")
                .is_some());
        }
        assert_eq!(
            request(
                &mut a,
                json!({"id":1,"command":{"type":"join","server_hash":"abc"}})
            )["error"],
            "invalid_request"
        );
        for id in 2..=15 {
            request(&mut a, json!({"id":id,"command":{"type":"properties"}}));
        }
        assert_eq!(
            request(&mut a, json!({"id":16,"command":{"type":"properties"}}))["error"],
            "rate_limited"
        );
        drop(first);
        assert_eq!(
            request(&mut b, json!({"id":2,"command":{"type":"properties"}}))["result"]["called"],
            true
        );
    }

    #[test]
    fn closes_oversized_or_unknown_requests_without_a_response() {
        for bytes in [
            ((protocol::MAX_REQUEST as u32 + 1).to_be_bytes()).to_vec(),
            {
                let body = br#"{"id":1,"command":{"type":"hello","url":"http://localhost"}}"#;
                let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
                bytes.extend_from_slice(body);
                bytes
            },
        ] {
            let broker = PreparedBroker::new(
                Box::new(TestOperations(Arc::new(AtomicBool::new(true)))),
                true,
            )
            .unwrap();
            let mut peer = broker.test_peer();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            peer.write_all(&bytes).unwrap();
            let mut byte = [0];
            match peer.read(&mut byte) {
                Ok(0) => {}
                Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
                result => panic!("malformed channel was not closed: {result:?}"),
            }
        }
    }

    #[test]
    fn dedicated_channel_enforces_handshake_network_and_revocation() {
        let valid = Arc::new(AtomicBool::new(true));
        let broker = PreparedBroker::new(Box::new(TestOperations(valid.clone())), false).unwrap();
        let mut peer = broker.test_peer();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        assert_eq!(
            request(&mut peer, json!({"id":1,"command":{"type":"properties"}}))["error"],
            "invalid_request"
        );
        assert_eq!(
            request(&mut peer, json!({"id":2,"command":{"type":"hello"}}))["result"]["protocol"],
            1
        );
        assert_eq!(
            request(&mut peer, json!({"id":3,"command":{"type":"properties"}}))["error"],
            "network_denied"
        );
        valid.store(false, Ordering::Release);
        assert_eq!(
            request(&mut peer, json!({"id":4,"command":{"type":"hello"}}))["error"],
            "revoked"
        );
        drop(broker);
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).unwrap(), 0);
    }
}
