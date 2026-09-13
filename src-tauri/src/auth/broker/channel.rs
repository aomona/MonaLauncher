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
    operations: &dyn Operations,
    deadline: Option<Instant>,
) -> io::Result<()> {
    while !bytes.is_empty() {
        // The socket's 250 ms read timeout also bounds idle lease revocation. Returning drops
        // the operation owner and its chat keys even when the game never sends another request.
        if !active.load(Ordering::Acquire) || !operations.valid() {
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
        read_exact_until(
            &mut stream,
            &mut header[..1],
            &active,
            operations.as_ref(),
            None,
        )?;
        let deadline = Some(Instant::now() + Duration::from_secs(5));
        read_exact_until(
            &mut stream,
            &mut header[1..],
            &active,
            operations.as_ref(),
            deadline,
        )?;
        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > protocol::MAX_REQUEST {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut bytes = vec![0; length];
        read_exact_until(
            &mut stream,
            &mut bytes,
            &active,
            operations.as_ref(),
            deadline,
        )?;
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
        let revoked = !active.load(Ordering::Acquire) || !operations.valid();
        let result = if revoked {
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
        if revoked {
            return Ok(());
        }
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
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).unwrap(), 0);
    }

    #[test]
    fn expired_chat_handle_does_not_revoke_the_account_channel() {
        struct ExpiredKey;
        impl Operations for ExpiredKey {
            fn valid(&self) -> bool {
                true
            }
            fn execute(&mut self, _: &Command) -> Result<serde_json::Value, BrokerError> {
                Err(BrokerError::Revoked)
            }
        }
        let broker = PreparedBroker::new(Box::new(ExpiredKey), true).unwrap();
        let mut peer = broker.test_peer();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        request(&mut peer, json!({"id":1,"command":{"type":"hello"}}));
        assert_eq!(
            request(
                &mut peer,
                json!({"id":2,"command":{"type":"sign","key_id":"expired","message":""}})
            )["error"],
            "revoked"
        );
        assert_eq!(
            request(&mut peer, json!({"id":3,"command":{"type":"hello"}}))["result"]["protocol"],
            1
        );
    }

    #[test]
    fn revocation_drops_key_owner_when_idle_or_during_execution() {
        struct KeyOwner {
            valid: Arc<AtomicBool>,
            dropped: Arc<AtomicBool>,
            revoke_in_execute: bool,
        }
        impl Operations for KeyOwner {
            fn valid(&self) -> bool {
                self.valid.load(Ordering::Acquire)
            }
            fn execute(&mut self, _: &Command) -> Result<serde_json::Value, BrokerError> {
                assert!(self.revoke_in_execute);
                self.valid.store(false, Ordering::Release);
                Ok(json!({ "must_not_return": true }))
            }
        }
        impl Drop for KeyOwner {
            fn drop(&mut self) {
                self.dropped.store(true, Ordering::Release);
            }
        }
        for during_execution in [false, true] {
            let valid = Arc::new(AtomicBool::new(true));
            let dropped = Arc::new(AtomicBool::new(false));
            let broker = PreparedBroker::new(
                Box::new(KeyOwner {
                    valid: valid.clone(),
                    dropped: dropped.clone(),
                    revoke_in_execute: during_execution,
                }),
                true,
            )
            .unwrap();
            let mut peer = broker.test_peer();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            request(&mut peer, json!({"id":1,"command":{"type":"hello"}}));
            if during_execution {
                assert_eq!(
                    request(&mut peer, json!({"id":2,"command":{"type":"certificate"}})),
                    json!({"id":2,"error":"revoked"})
                );
            } else {
                valid.store(false, Ordering::Release);
            }
            assert_eq!(peer.read(&mut [0]).unwrap(), 0);
            assert!(dropped.load(Ordering::Acquire));
        }
    }
}
