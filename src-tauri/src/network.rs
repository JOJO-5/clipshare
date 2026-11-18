use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::protocol::*;
use serde::Serialize;

const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(not(test))]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(not(test))]
const WRITE_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
const WRITE_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(not(test))]
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(not(test))]
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(350);
const MESSAGE_ACK_TIMEOUT: Duration = Duration::from_secs(5);
use crate::config::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ConnectionEndpoint {
    Server { port: u16 },
    Client { address: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionSnapshot {
    pub status: ConnectionStatus,
    pub endpoint: Option<ConnectionEndpoint>,
    pub peer: Option<IpAddr>,
    pub reason: &'static str,
    #[serde(skip)]
    session_id: u64,
}

type ReceiveHandler = Arc<dyn Fn(Vec<u8>, u8) -> Result<(), String> + Send + Sync + 'static>;
type StatusHandler = Arc<dyn Fn(ConnectionStatus, &str) + Send + Sync + 'static>;
type AckWaiter = Arc<(Mutex<Option<bool>>, Condvar)>;

pub trait ReceiveOutcome {
    fn into_receive_result(self) -> Result<(), String>;
}

impl ReceiveOutcome for () {
    fn into_receive_result(self) -> Result<(), String> {
        Ok(())
    }
}

impl ReceiveOutcome for Result<(), String> {
    fn into_receive_result(self) -> Result<(), String> {
        self
    }
}

#[derive(Debug, Clone, Copy)]
enum ConnectionEnd {
    Stopped,
    HeartbeatTimeout,
    PeerClosed,
    ProtocolError,
    ReceiveHandlerPanicked,
}

impl ConnectionEnd {
    fn reason(self) -> &'static str {
        match self {
            Self::Stopped => "session stopped",
            Self::HeartbeatTimeout => "heartbeat timeout; retrying",
            Self::PeerClosed => "peer closed connection; retrying",
            Self::ProtocolError => "network protocol error; retrying",
            Self::ReceiveHandlerPanicked => "receive handler failed; retrying",
        }
    }
}

#[derive(Clone)]
struct ClientTarget {
    ip: String,
    port: u16,
    on_receive: ReceiveHandler,
}

pub struct NetworkManager {
    status: Arc<Mutex<ConnectionSnapshot>>,
    sender: Arc<Mutex<Option<TcpStream>>>,
    receiver: Arc<Mutex<Option<TcpStream>>>,
    config: Arc<Mutex<AppConfig>>,
    session: Arc<AtomicU64>,
    next_sequence: AtomicU16,
    client_target: Arc<Mutex<Option<ClientTarget>>>,
    status_handler: Arc<Mutex<Option<StatusHandler>>>,
    ack_waiters: Arc<Mutex<HashMap<u16, AckWaiter>>>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(ConnectionSnapshot {
                status: ConnectionStatus::Disconnected,
                endpoint: None,
                peer: None,
                reason: "idle",
                session_id: 0,
            })),
            sender: Arc::new(Mutex::new(None)),
            receiver: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(AppConfig::load())),
            session: Arc::new(AtomicU64::new(0)),
            next_sequence: AtomicU16::new(1),
            client_target: Arc::new(Mutex::new(None)),
            status_handler: Arc::new(Mutex::new(None)),
            ack_waiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_status(&self) -> ConnectionStatus {
        self.status.lock().unwrap().status
    }

    pub fn connection_snapshot(&self) -> ConnectionSnapshot {
        self.status.lock().unwrap().clone()
    }

    pub fn set_status_handler<F>(&self, handler: F)
    where
        F: Fn(ConnectionStatus, &str) + Send + Sync + 'static,
    {
        *self.status_handler.lock().unwrap() = Some(Arc::new(handler));
    }

    fn transition_status(
        status: &Mutex<ConnectionSnapshot>,
        status_handler: &Arc<Mutex<Option<StatusHandler>>>,
        session_id: u64,
        next: ConnectionStatus,
        reason: &'static str,
        peer: Option<IpAddr>,
    ) {
        let changed = match status.lock() {
            Ok(mut current) if current.session_id == session_id => {
                // Starting from an already idle session should not publish a disconnect event.
                let changed = current.status != next || current.peer != peer
                    || (current.reason != reason && reason != "manual disconnect");
                current.status = next;
                current.reason = reason;
                current.peer = peer;
                changed
            }
            _ => false,
        };
        if !changed {
            return;
        }

        let handler = status_handler
            .lock()
            .ok()
            .and_then(|handler| handler.clone());
        if let Some(handler) = handler {
            let _ = catch_unwind(AssertUnwindSafe(|| handler(next, reason)));
        }
    }

    fn replace_session(&self, endpoint: Option<ConnectionEndpoint>) -> u64 {
        let mut current = self.status.lock().unwrap();
        let session_id = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        current.session_id = session_id;
        current.endpoint = endpoint;
        session_id
    }

    fn close_streams(&self) {
        if let Ok(mut s) = self.sender.lock() {
            if let Some(stream) = s.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        if let Ok(mut r) = self.receiver.lock() {
            if let Some(stream) = r.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }

    fn stop_current_session(&self) {
        let session_id = self.replace_session(None);
        self.fail_ack_waiters();
        self.close_streams();
        Self::transition_status(
            &self.status,
            &self.status_handler,
            session_id,
            ConnectionStatus::Disconnected,
            "manual disconnect",
            None,
        );
    }

    pub fn disconnect(&self) {
        *self.client_target.lock().unwrap() = None;
        self.stop_current_session();
    }

    fn fail_ack_waiters(&self) {
        Self::fail_ack_waiters_for(&self.ack_waiters);
    }

    fn fail_ack_waiters_for(ack_waiters: &Arc<Mutex<HashMap<u16, AckWaiter>>>) {
        let waiters = ack_waiters
            .lock()
            .map(|mut waiters| {
                waiters
                    .drain()
                    .map(|(_, waiter)| waiter)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for waiter in waiters {
            let (completed, wake) = &*waiter;
            if let Ok(mut completed) = completed.lock() {
                *completed = Some(false);
                wake.notify_all();
            }
        }
    }

    fn status_after_connection_loss(&self) -> ConnectionStatus {
        if self.status.lock().unwrap().endpoint.is_some() {
            ConnectionStatus::Connecting
        } else {
            ConnectionStatus::Disconnected
        }
    }

    fn receive_loop(
        mut receiver: TcpStream,
        session: Arc<AtomicU64>,
        session_id: u64,
        on_receive: ReceiveHandler,
        sender: Arc<Mutex<Option<TcpStream>>>,
        ack_waiters: Arc<Mutex<HashMap<u16, AckWaiter>>>,
    ) -> ConnectionEnd {
        let mut processed_sequences = HashSet::new();
        let mut processed_sequence_order = VecDeque::new();
        loop {
            if session.load(Ordering::SeqCst) != session_id {
                return ConnectionEnd::Stopped;
            }

            let mut header_buf = [0u8; HEADER_SIZE];
            let header_result = receiver.read_exact(&mut header_buf).and_then(|()| {
                let mut cursor = std::io::Cursor::new(&header_buf);
                MessageHeader::from_reader(&mut cursor)
            });

            match header_result {
                Ok(h) => {
                    if h.msg_type == TYPE_HEARTBEAT {
                        continue;
                    }
                    if h.msg_type == TYPE_ACK || h.msg_type == TYPE_NACK {
                        let acknowledged = h.msg_type == TYPE_ACK;
                        if let Ok(mut waiters) = ack_waiters.lock() {
                            if let Some(waiter) = waiters.remove(&h.sequence) {
                                let (completed, wake) = &*waiter;
                                if let Ok(mut completed) = completed.lock() {
                                    *completed = Some(acknowledged);
                                    wake.notify_all();
                                }
                            }
                        }
                        continue;
                    }
                    if h.data_len as usize > MAX_MESSAGE_SIZE {
                        return ConnectionEnd::ProtocolError;
                    }
                    let mut data = vec![0u8; h.data_len as usize];
                    if let Err(error) = receiver.read_exact(&mut data) {
                        return Self::connection_end_from_io(&error);
                    }

                    if h.sequence != 0 && processed_sequences.contains(&h.sequence) {
                        if Self::write_sender_control_frame(&sender, TYPE_ACK, h.sequence).is_err()
                        {
                            return ConnectionEnd::PeerClosed;
                        }
                        continue;
                    }

                    match catch_unwind(AssertUnwindSafe(|| on_receive(data, h.msg_type))) {
                        Ok(Ok(())) => {
                            if h.sequence != 0 {
                                Self::remember_processed_sequence(
                                    &mut processed_sequences,
                                    &mut processed_sequence_order,
                                    h.sequence,
                                );
                            }
                            if Self::write_sender_control_frame(&sender, TYPE_ACK, h.sequence)
                                .is_err()
                            {
                                return ConnectionEnd::PeerClosed;
                            }
                        }
                        Ok(Err(_)) => {
                            if Self::write_sender_control_frame(&sender, TYPE_NACK, h.sequence)
                                .is_err()
                            {
                                return ConnectionEnd::PeerClosed;
                            }
                        }
                        Err(_) => return ConnectionEnd::ReceiveHandlerPanicked,
                    }
                }
                Err(error) => return Self::connection_end_from_io(&error),
            }
        }
    }

    fn remember_processed_sequence(
        processed_sequences: &mut HashSet<u16>,
        sequence_order: &mut VecDeque<u16>,
        sequence: u16,
    ) {
        if !processed_sequences.insert(sequence) {
            return;
        }
        sequence_order.push_back(sequence);
        const MAX_PROCESSED_SEQUENCES: usize = 4096;
        while sequence_order.len() > MAX_PROCESSED_SEQUENCES {
            if let Some(expired) = sequence_order.pop_front() {
                processed_sequences.remove(&expired);
            }
        }
    }

    fn connection_end_from_io(error: &std::io::Error) -> ConnectionEnd {
        match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                ConnectionEnd::HeartbeatTimeout
            }
            std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::BrokenPipe => ConnectionEnd::PeerClosed,
            _ => ConnectionEnd::ProtocolError,
        }
    }

    fn write_control_frame(
        stream: &mut TcpStream,
        msg_type: u8,
        sequence: u16,
    ) -> std::io::Result<()> {
        stream.write_all(&MessageHeader::new(msg_type, 0, sequence).to_bytes())?;
        stream.flush()
    }

    fn write_sender_control_frame(
        sender: &Arc<Mutex<Option<TcpStream>>>,
        msg_type: u8,
        sequence: u16,
    ) -> std::io::Result<()> {
        let mut sender = sender
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Sender unavailable"))?;
        let stream = sender.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "Not connected")
        })?;
        Self::write_control_frame(stream, msg_type, sequence)
    }

    fn configure_connected_stream(stream: &TcpStream) -> std::io::Result<()> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HEARTBEAT_TIMEOUT))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))
    }

    fn connect_with_timeout(ip: &str, port: u16) -> std::io::Result<TcpStream> {
        let addresses = (ip, port).to_socket_addrs()?;
        let mut last_error = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                "No target address resolved",
            )
        }))
    }

    fn spawn_heartbeat(
        sender: Arc<Mutex<Option<TcpStream>>>,
        status: Arc<Mutex<ConnectionSnapshot>>,
        status_handler: Arc<Mutex<Option<StatusHandler>>>,
        session: Arc<AtomicU64>,
        session_id: u64,
        stop: Arc<AtomicBool>,
    ) {
        thread::spawn(move || loop {
            thread::sleep(HEARTBEAT_INTERVAL);
            if stop.load(Ordering::SeqCst) || session.load(Ordering::SeqCst) != session_id {
                return;
            }

            let result = sender
                .lock()
                .map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Network sender is unavailable")
                })
                .and_then(|mut sender| {
                    sender
                        .as_mut()
                        .ok_or_else(|| {
                            std::io::Error::new(std::io::ErrorKind::NotConnected, "Not connected")
                        })
                        .and_then(|stream| Self::write_control_frame(stream, TYPE_HEARTBEAT, 0))
                });
            if result.is_err() {
                if !stop.load(Ordering::SeqCst) && session.load(Ordering::SeqCst) == session_id {
                    if let Ok(mut sender) = sender.lock() {
                        if let Some(stream) = sender.take() {
                            let _ = stream.shutdown(Shutdown::Both);
                        }
                    }
                    Self::transition_status(
                        &status,
                        &status_handler,
                        session_id,
                        ConnectionStatus::Connecting,
                        "heartbeat send failed; retrying",
                        None,
                    );
                }
                return;
            }
        });
    }

    fn wait_for_protocol_ack(stream: &mut TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let mut header_buf = [0u8; HEADER_SIZE];
        stream.read_exact(&mut header_buf)?;
        let mut cursor = std::io::Cursor::new(&header_buf);
        let header = MessageHeader::from_reader(&mut cursor)?;
        if header.msg_type != TYPE_ACK || header.data_len != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ClipShare protocol acknowledgement was not received",
            ));
        }
        stream.set_read_timeout(Some(HEARTBEAT_TIMEOUT))
    }

    pub fn start_server<F, R>(&self, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) -> R + Send + Sync + 'static,
        R: ReceiveOutcome,
    {
        self.disconnect();
        let session_id = self.replace_session(Some(ConnectionEndpoint::Server { port }));
        let on_receive: ReceiveHandler =
            Arc::new(move |data, msg_type| on_receive(data, msg_type).into_receive_result());

        let addr = format!("0.0.0.0:{}", port);
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(error) => {
                Self::transition_status(
                    &self.status,
                    &self.status_handler,
                    session_id,
                    ConnectionStatus::Disconnected,
                    "server bind failed",
                    None,
                );
                return Err(error.to_string());
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            Self::transition_status(
                &self.status,
                &self.status_handler,
                session_id,
                ConnectionStatus::Disconnected,
                "server listener setup failed",
                None,
            );
            return Err(error.to_string());
        }
        Self::transition_status(
            &self.status,
            &self.status_handler,
            session_id,
            ConnectionStatus::Connecting,
            "server listening",
            None,
        );

        let sender = self.sender.clone();
        let receiver = self.receiver.clone();
        let status = self.status.clone();
        let status_handler = self.status_handler.clone();
        let session = self.session.clone();
        let ack_waiters = self.ack_waiters.clone();

        thread::spawn(move || loop {
            if session.load(Ordering::SeqCst) != session_id {
                return;
            }

            match listener.accept() {
                Ok((mut stream, peer)) => {
                    if Self::configure_connected_stream(&stream).is_err() {
                        continue;
                    }
                    if Self::write_control_frame(&mut stream, TYPE_ACK, 0).is_err() {
                        continue;
                    }
                    let send_stream = match stream.try_clone() {
                        Ok(stream) => stream,
                        Err(_) => return,
                    };
                    let close_stream = match stream.try_clone() {
                        Ok(stream) => stream,
                        Err(_) => return,
                    };
                    if session.load(Ordering::SeqCst) != session_id {
                        return;
                    }
                    *sender.lock().unwrap() = Some(send_stream);
                    *receiver.lock().unwrap() = Some(close_stream);
                    Self::transition_status(
                        &status,
                        &status_handler,
                        session_id,
                        ConnectionStatus::Connected,
                        "peer accepted and acknowledged",
                        Some(peer.ip()),
                    );
                    let heartbeat_stop = Arc::new(AtomicBool::new(false));
                    Self::spawn_heartbeat(
                        sender.clone(),
                        status.clone(),
                        status_handler.clone(),
                        session.clone(),
                        session_id,
                        heartbeat_stop.clone(),
                    );
                    let connection_end = Self::receive_loop(
                        stream,
                        session.clone(),
                        session_id,
                        on_receive.clone(),
                        sender.clone(),
                        ack_waiters.clone(),
                    );
                    Self::fail_ack_waiters_for(&ack_waiters);
                    heartbeat_stop.store(true, Ordering::SeqCst);
                    if session.load(Ordering::SeqCst) != session_id {
                        return;
                    }
                    sender.lock().unwrap().take();
                    receiver.lock().unwrap().take();
                    Self::transition_status(
                        &status,
                        &status_handler,
                        session_id,
                        ConnectionStatus::Connecting,
                        connection_end.reason(),
                        None,
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {
                    if session.load(Ordering::SeqCst) == session_id {
                        Self::transition_status(
                            &status,
                            &status_handler,
                            session_id,
                            ConnectionStatus::Disconnected,
                            "server listener failed",
                            None,
                        );
                    }
                    return;
                }
            }
        });

        Ok(())
    }

    pub fn connect_to_server<F, R>(&self, ip: &str, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) -> R + Send + Sync + 'static,
        R: ReceiveOutcome,
    {
        self.disconnect();
        let session_id = self.replace_session(Some(ConnectionEndpoint::Client {
            address: format!("{ip}:{port}"),
        }));
        Self::transition_status(
            &self.status,
            &self.status_handler,
            session_id,
            ConnectionStatus::Connecting,
            "connecting to peer",
            None,
        );
        let target = ClientTarget {
            ip: ip.to_string(),
            port,
            on_receive: Arc::new(move |data, msg_type| {
                on_receive(data, msg_type).into_receive_result()
            }),
        };
        *self.client_target.lock().unwrap() = Some(target.clone());

        let sender = self.sender.clone();
        let receiver = self.receiver.clone();
        let status = self.status.clone();
        let status_handler = self.status_handler.clone();
        let session = self.session.clone();
        let client_target = self.client_target.clone();
        let ack_waiters = self.ack_waiters.clone();

        thread::spawn(move || {
            let mut retry_delay = Duration::from_millis(250);
            loop {
                if session.load(Ordering::SeqCst) != session_id {
                    return;
                }

                match Self::connect_with_timeout(&target.ip, target.port) {
                    Ok(mut stream) => {
                        if Self::configure_connected_stream(&stream).is_err() {
                            continue;
                        }
                        if Self::wait_for_protocol_ack(&mut stream).is_err() {
                            let _ = stream.shutdown(Shutdown::Both);
                            Self::transition_status(
                                &status,
                                &status_handler,
                                session_id,
                                ConnectionStatus::Connecting,
                                "protocol acknowledgement failed; retrying",
                                None,
                            );
                            continue;
                        }
                        let send_stream = match stream.try_clone() {
                            Ok(stream) => stream,
                            Err(_) => continue,
                        };
                        let close_stream = match stream.try_clone() {
                            Ok(stream) => stream,
                            Err(_) => continue,
                        };
                        if session.load(Ordering::SeqCst) != session_id {
                            return;
                        }
                        let peer = stream.peer_addr().ok().map(|address| address.ip());
                        *sender.lock().unwrap() = Some(send_stream);
                        *receiver.lock().unwrap() = Some(close_stream);
                        Self::transition_status(
                            &status,
                            &status_handler,
                            session_id,
                            ConnectionStatus::Connected,
                            "protocol ack received",
                            peer,
                        );
                        retry_delay = Duration::from_millis(250);
                        let heartbeat_stop = Arc::new(AtomicBool::new(false));
                        Self::spawn_heartbeat(
                            sender.clone(),
                            status.clone(),
                            status_handler.clone(),
                            session.clone(),
                            session_id,
                            heartbeat_stop.clone(),
                        );

                        let connection_end = Self::receive_loop(
                            stream,
                            session.clone(),
                            session_id,
                            target.on_receive.clone(),
                            sender.clone(),
                            ack_waiters.clone(),
                        );
                        Self::fail_ack_waiters_for(&ack_waiters);
                        heartbeat_stop.store(true, Ordering::SeqCst);
                        if session.load(Ordering::SeqCst) != session_id {
                            return;
                        }
                        sender.lock().unwrap().take();
                        receiver.lock().unwrap().take();
                        Self::transition_status(
                            &status,
                            &status_handler,
                            session_id,
                            ConnectionStatus::Connecting,
                            connection_end.reason(),
                            None,
                        );
                    }
                    Err(_) => {
                        Self::transition_status(
                            &status,
                            &status_handler,
                            session_id,
                            ConnectionStatus::Connecting,
                            "connect failed; retrying",
                            None,
                        );
                    }
                }

                let mut waited = Duration::ZERO;
                while waited < retry_delay {
                    if session.load(Ordering::SeqCst) != session_id {
                        return;
                    }
                    let step = (retry_delay - waited).min(Duration::from_millis(100));
                    thread::sleep(step);
                    waited += step;
                }
                retry_delay = (retry_delay * 2).min(Duration::from_secs(5));

                if client_target.lock().unwrap().is_none() {
                    return;
                }
            }
        });

        Ok(())
    }

    fn send_frame(&self, msg_type: u8, data: &[u8], sequence: u16) -> Result<(), String> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err("Message exceeds the 100 MB transfer limit".to_string());
        }
        let session_id = self.session.load(Ordering::SeqCst);
        let result = {
            let mut sender = self.sender.lock().unwrap();
            if let Some(ref mut stream) = *sender {
                let header = MessageHeader::new(msg_type, data.len() as u32, sequence);
                stream
                    .write_all(&header.to_bytes())
                    .and_then(|_| stream.write_all(data))
                    .and_then(|_| stream.flush())
                    .map_err(|error| error.to_string())
            } else {
                Err("Not connected".to_string())
            }
        };

        if result.is_err() && self.session.load(Ordering::SeqCst) == session_id {
            self.close_streams();
            self.fail_ack_waiters();
            Self::transition_status(
                &self.status,
                &self.status_handler,
                session_id,
                self.status_after_connection_loss(),
                "send failed; retrying",
                None,
            );
        }
        result
    }

    pub fn send(&self, msg_type: u8, data: &[u8], sequence: u16) -> Result<(), String> {
        self.send_frame(msg_type, data, sequence)
    }

    fn next_message_sequence(&self) -> u16 {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        if sequence == 0 {
            1
        } else {
            sequence
        }
    }

    fn send_with_ack(&self, msg_type: u8, data: &[u8]) -> Result<(), String> {
        let session_id = self.session.load(Ordering::SeqCst);
        let sequence = self.next_message_sequence();
        let waiter: AckWaiter = Arc::new((Mutex::new(None), Condvar::new()));
        self.ack_waiters
            .lock()
            .map_err(|_| "Network acknowledgement state is unavailable".to_string())?
            .insert(sequence, waiter.clone());

        if let Err(error) = self.send_frame(msg_type, data, sequence) {
            if let Ok(mut waiters) = self.ack_waiters.lock() {
                waiters.remove(&sequence);
            }
            return Err(error);
        }

        let (completed, wake) = &*waiter;
        let completion = completed
            .lock()
            .ok()
            .and_then(|completed| {
                wake.wait_timeout_while(completed, MESSAGE_ACK_TIMEOUT, |done| done.is_none())
                    .ok()
                    .map(|(completed, _)| *completed)
            })
            .unwrap_or(None);
        if let Ok(mut waiters) = self.ack_waiters.lock() {
            waiters.remove(&sequence);
        }
        if completion == Some(true) {
            return Ok(());
        }

        let (reason, error) = if completion == Some(false) {
            ("message rejected; retrying", "Peer rejected the message")
        } else {
            (
                "message acknowledgement timeout; retrying",
                "Peer did not acknowledge the message",
            )
        };
        if self.session.load(Ordering::SeqCst) == session_id {
            self.close_streams();
            self.fail_ack_waiters();
            Self::transition_status(
                &self.status,
                &self.status_handler,
                session_id,
                self.status_after_connection_loss(),
                reason,
                None,
            );
        }
        Err(error.to_string())
    }

    pub fn send_text(&self, text: &str) -> Result<(), String> {
        self.send_with_ack(TYPE_TEXT, text.as_bytes())
    }

    pub fn send_wechat(&self, message: &WeChatMessage) -> Result<(), String> {
        let payload = encode_wechat(message)?;
        self.send_with_ack(TYPE_WECHAT, &payload)
    }

    pub fn send_image(&self, width: usize, height: usize, data: &[u8]) -> Result<(), String> {
        let payload = encode_image(width, height, data)?;
        self.send_with_ack(TYPE_IMAGE, &payload)
    }

    pub fn send_file_data(&self, filename: &str, contents: &[u8]) -> Result<(), String> {
        let payload = encode_file(filename, contents)?;
        self.send_with_ack(TYPE_FILE, &payload)
    }

    pub fn send_file(
        &self,
        filename: &str,
        file_size: u64,
        chunks: impl Iterator<Item = Vec<u8>>,
    ) -> Result<(), String> {
        let capacity = usize::try_from(file_size)
            .map_err(|_| "File is too large for this platform".to_string())?;
        if capacity > MAX_MESSAGE_SIZE {
            return Err("Message exceeds the 100 MB transfer limit".to_string());
        }
        let mut contents = Vec::with_capacity(capacity);
        for chunk in chunks {
            contents.extend_from_slice(&chunk);
            if contents.len() > MAX_MESSAGE_SIZE {
                return Err("Message exceeds the 100 MB transfer limit".to_string());
            }
        }
        if contents.len() as u64 != file_size {
            return Err("File contents do not match the declared size".to_string());
        }
        self.send_file_data(filename, &contents)
    }
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn available_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn wait_for_connected(managers: &[&NetworkManager]) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while managers
            .iter()
            .any(|manager| manager.get_status() != ConnectionStatus::Connected)
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(managers
            .iter()
            .all(|manager| manager.get_status() == ConnectionStatus::Connected));
    }

    #[test]
    fn connection_snapshot_reports_listener_peer_and_disconnect() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        server.start_server(port, |_, _| {}).unwrap();
        let waiting = server.connection_snapshot();
        assert_eq!(waiting.status, ConnectionStatus::Connecting);
        assert_eq!(waiting.endpoint, Some(ConnectionEndpoint::Server { port }));
        assert_eq!(waiting.reason, "server listening");
        assert_eq!(waiting.peer, None);

        client.connect_to_server("127.0.0.1", port, |_, _| {}).unwrap();
        wait_for_connected(&[&server, &client]);
        for manager in [&server, &client] {
            assert_eq!(manager.connection_snapshot().peer, Some("127.0.0.1".parse().unwrap()));
        }
        client.disconnect();
        server.disconnect();
        let disconnected = server.connection_snapshot();
        assert_eq!(disconnected.status, ConnectionStatus::Disconnected);
        assert_eq!(disconnected.endpoint, None);
        assert_eq!(disconnected.peer, None);
    }

    #[test]
    fn outdated_worker_does_not_overwrite_connection_snapshot() {
        let manager = NetworkManager::new();
        let previous = manager.replace_session(Some(ConnectionEndpoint::Server { port: 9527 }));
        manager.disconnect();
        NetworkManager::transition_status(
            &manager.status,
            &manager.status_handler,
            previous,
            ConnectionStatus::Connected,
            "peer accepted and acknowledged",
            Some("127.0.0.1".parse().unwrap()),
        );
        assert_eq!(manager.connection_snapshot().status, ConnectionStatus::Disconnected);
        assert_eq!(manager.connection_snapshot().peer, None);
    }

    #[test]
    fn connected_peers_deliver_text_to_the_remote_callback() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let (received_tx, received_rx) = mpsc::channel();

        server
            .start_server(port, move |data, message_type| {
                received_tx.send((data, message_type)).unwrap();
            })
            .unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while server.get_status() != ConnectionStatus::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        client.send_text("network delivery check").unwrap();

        assert_eq!(
            received_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (b"network delivery check".to_vec(), TYPE_TEXT),
        );
        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn connected_peers_deliver_wechat_messages_to_the_remote_callback() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let (received_tx, received_rx) = mpsc::channel();

        server
            .start_server(port, move |data, message_type| {
                received_tx.send((data, message_type)).unwrap();
            })
            .unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while server.get_status() != ConnectionStatus::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        let message = WeChatMessage {
            id: "network-msg".to_string(),
            sender: "张三".to_string(),
            preview: "预览".to_string(),
            content: "完整消息".to_string(),
            timestamp: 1_700_000_000,
            unread_count: 1,
        };
        client.send_wechat(&message).unwrap();

        let (data, message_type) = received_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(message_type, TYPE_WECHAT);
        assert_eq!(decode_wechat(&data).unwrap(), message);

        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn wechat_send_waits_for_remote_processing_ack() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let (received_tx, received_rx) = mpsc::channel();

        server
            .start_server(port, move |_, message_type| {
                if message_type == TYPE_WECHAT {
                    received_tx.send(()).unwrap();
                    thread::sleep(Duration::from_millis(100));
                }
            })
            .unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();

        wait_for_connected(&[&server, &client]);
        let message = WeChatMessage {
            id: "ack-check".to_string(),
            sender: "Alice".to_string(),
            preview: "hello".to_string(),
            content: "hello".to_string(),
            timestamp: 1,
            unread_count: 1,
        };
        let started = Instant::now();
        client.send_wechat(&message).unwrap();

        assert!(received_rx.recv_timeout(Duration::from_secs(1)).is_ok());
        assert!(started.elapsed() >= Duration::from_millis(80));
        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn clipboard_send_reports_remote_processing_failure() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();

        server
            .start_server(port, |_, _| Err("clipboard write failed".to_string()))
            .unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&server, &client]);

        let error = client.send_text("rejected clipboard").unwrap_err();
        assert!(error.contains("rejected"));

        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn duplicate_sequence_is_processed_once_but_acknowledged_again() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let processed = Arc::new(AtomicUsize::new(0));
        let processed_by_server = Arc::clone(&processed);

        server
            .start_server(port, move |_, _| {
                processed_by_server.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&server, &client]);

        client.send(TYPE_TEXT, b"duplicate", 41).unwrap();
        client.send(TYPE_TEXT, b"duplicate", 41).unwrap();
        thread::sleep(Duration::from_millis(150));

        assert_eq!(processed.load(Ordering::SeqCst), 1);
        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn text_image_and_file_clipboard_payloads_all_wait_for_ack() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let processed = Arc::new(AtomicUsize::new(0));
        let processed_by_server = Arc::clone(&processed);

        server
            .start_server(port, move |_, _| {
                processed_by_server.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&server, &client]);

        client.send_text("ack text").unwrap();
        client.send_image(1, 1, &[255, 0, 0, 255]).unwrap();
        client.send_file_data("ack.txt", b"ack file").unwrap();

        assert_eq!(processed.load(Ordering::SeqCst), 3);
        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn client_started_before_server_retries_until_the_server_is_available() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let (received_tx, received_rx) = mpsc::channel();

        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        assert_eq!(client.get_status(), ConnectionStatus::Connecting);
        thread::sleep(Duration::from_millis(150));

        server
            .start_server(port, move |data, message_type| {
                received_tx.send((data, message_type)).unwrap();
            })
            .unwrap();
        wait_for_connected(&[&server, &client]);

        client.send_text("client started first").unwrap();
        assert_eq!(
            received_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (b"client started first".to_vec(), TYPE_TEXT),
        );
        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn server_accepts_a_new_client_after_the_previous_connection_drops() {
        let port = available_port();
        let server = NetworkManager::new();
        let first_client = NetworkManager::new();
        let second_client = NetworkManager::new();
        let (received_tx, received_rx) = mpsc::channel();

        server
            .start_server(port, move |data, message_type| {
                received_tx.send((data, message_type)).unwrap();
            })
            .unwrap();
        first_client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&server, &first_client]);
        first_client.disconnect();

        let deadline = Instant::now() + Duration::from_secs(2);
        while server.get_status() == ConnectionStatus::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        second_client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&server, &second_client]);
        second_client.send_text("replacement client").unwrap();

        assert_eq!(
            received_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (b"replacement client".to_vec(), TYPE_TEXT),
        );
        first_client.disconnect();
        second_client.disconnect();
        server.disconnect();
    }

    #[test]
    fn client_waits_for_protocol_ack_before_reporting_connected() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });
        let client = NetworkManager::new();

        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        thread::sleep(Duration::from_millis(150));

        assert_ne!(client.get_status(), ConnectionStatus::Connected);
        client.disconnect();
        peer.join().unwrap();
    }

    #[test]
    fn receive_callback_panic_does_not_leave_connected_status_stale() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        server.start_server(port, |_, _| {}).unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| -> Result<(), String> {
                panic!("callback failed")
            })
            .unwrap();
        wait_for_connected(&[&server, &client]);

        server.send(TYPE_TEXT, b"trigger callback", 0).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while client.get_status() == ConnectionStatus::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        assert_ne!(client.get_status(), ConnectionStatus::Connected);
        client.disconnect();
        server.disconnect();
    }

    #[test]
    fn connected_client_sends_periodic_heartbeats() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (heartbeat_tx, heartbeat_rx) = mpsc::channel();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            NetworkManager::write_control_frame(&mut stream, TYPE_ACK, 0).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header_buf = [0u8; HEADER_SIZE];
            stream.read_exact(&mut header_buf).unwrap();
            let mut cursor = std::io::Cursor::new(&header_buf);
            let header = MessageHeader::from_reader(&mut cursor).unwrap();
            heartbeat_tx.send(header.msg_type).unwrap();
        });
        let client = NetworkManager::new();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();

        assert_eq!(
            heartbeat_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            TYPE_HEARTBEAT
        );
        client.disconnect();
        peer.join().unwrap();
    }

    #[test]
    fn client_reconnects_when_an_acknowledged_peer_goes_silent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            NetworkManager::write_control_frame(&mut stream, TYPE_ACK, 0).unwrap();
            thread::sleep(Duration::from_secs(2));
        });
        let client = NetworkManager::new();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&client]);

        let deadline = Instant::now() + Duration::from_secs(1);
        while client.get_status() == ConnectionStatus::Connected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(client.get_status(), ConnectionStatus::Connecting);
        client.disconnect();
        peer.join().unwrap();
    }

    #[test]
    fn client_reconnects_after_the_active_socket_is_dropped() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let peer = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            NetworkManager::write_control_frame(&mut first, TYPE_ACK, 0).unwrap();
            accepted_tx.send(1).unwrap();
            thread::sleep(Duration::from_millis(100));
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            NetworkManager::write_control_frame(&mut second, TYPE_ACK, 0).unwrap();
            accepted_tx.send(2).unwrap();
            thread::sleep(Duration::from_millis(250));
        });
        let client = NetworkManager::new();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();

        assert_eq!(accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
        assert_eq!(accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
        wait_for_connected(&[&client]);

        client.disconnect();
        peer.join().unwrap();
    }

    #[test]
    fn status_handler_reports_connection_transitions() {
        let port = available_port();
        let server = NetworkManager::new();
        let client = NetworkManager::new();
        let (status_tx, status_rx) = mpsc::channel();
        client.set_status_handler(move |status, reason| {
            status_tx.send((status, reason.to_string())).unwrap();
        });
        server.start_server(port, |_, _| {}).unwrap();
        client
            .connect_to_server("127.0.0.1", port, |_, _| {})
            .unwrap();
        wait_for_connected(&[&server, &client]);

        let transitions = (0..2)
            .filter_map(|_| status_rx.recv_timeout(Duration::from_secs(1)).ok())
            .collect::<Vec<_>>();
        assert!(transitions
            .iter()
            .any(|(status, _)| *status == ConnectionStatus::Connecting));
        assert!(transitions.iter().any(|(status, reason)| {
            *status == ConnectionStatus::Connected && reason.contains("ack")
        }));

        client.disconnect();
        server.disconnect();
    }
}
