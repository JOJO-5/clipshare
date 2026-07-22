use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::protocol::*;

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
use crate::config::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

type ReceiveHandler = Arc<dyn Fn(Vec<u8>, u8) + Send + Sync + 'static>;
type StatusHandler = Arc<dyn Fn(ConnectionStatus, &str) + Send + Sync + 'static>;

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
    pub status: Arc<Mutex<ConnectionStatus>>,
    sender: Arc<Mutex<Option<TcpStream>>>,
    receiver: Arc<Mutex<Option<TcpStream>>>,
    config: Arc<Mutex<AppConfig>>,
    session: Arc<AtomicU64>,
    client_target: Arc<Mutex<Option<ClientTarget>>>,
    status_handler: Arc<Mutex<Option<StatusHandler>>>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            sender: Arc::new(Mutex::new(None)),
            receiver: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(AppConfig::load())),
            session: Arc::new(AtomicU64::new(0)),
            client_target: Arc::new(Mutex::new(None)),
            status_handler: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_status(&self) -> ConnectionStatus {
        *self.status.lock().unwrap()
    }

    pub fn set_status_handler<F>(&self, handler: F)
    where
        F: Fn(ConnectionStatus, &str) + Send + Sync + 'static,
    {
        *self.status_handler.lock().unwrap() = Some(Arc::new(handler));
    }

    fn transition_status(
        status: &Arc<Mutex<ConnectionStatus>>,
        status_handler: &Arc<Mutex<Option<StatusHandler>>>,
        next: ConnectionStatus,
        reason: &str,
    ) {
        let changed = match status.lock() {
            Ok(mut current) if *current != next => {
                *current = next;
                true
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
        self.session.fetch_add(1, Ordering::SeqCst);
        self.close_streams();
        Self::transition_status(
            &self.status,
            &self.status_handler,
            ConnectionStatus::Disconnected,
            "manual disconnect",
        );
    }

    pub fn disconnect(&self) {
        *self.client_target.lock().unwrap() = None;
        self.stop_current_session();
    }

    fn status_after_connection_loss(&self) -> ConnectionStatus {
        if self.client_target.lock().unwrap().is_some() {
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
    ) -> ConnectionEnd {
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
                    if h.data_len as usize > MAX_MESSAGE_SIZE {
                        return ConnectionEnd::ProtocolError;
                    }
                    let mut data = vec![0u8; h.data_len as usize];
                    if let Err(error) = receiver.read_exact(&mut data) {
                        return Self::connection_end_from_io(&error);
                    }
                    if catch_unwind(AssertUnwindSafe(|| on_receive(data, h.msg_type))).is_err() {
                        return ConnectionEnd::ReceiveHandlerPanicked;
                    }
                }
                Err(error) => return Self::connection_end_from_io(&error),
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

    fn write_control_frame(stream: &mut TcpStream, msg_type: u8) -> std::io::Result<()> {
        stream.write_all(&MessageHeader::new(msg_type, 0, 0).to_bytes())?;
        stream.flush()
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
        status: Arc<Mutex<ConnectionStatus>>,
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
                        .and_then(|stream| Self::write_control_frame(stream, TYPE_HEARTBEAT))
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
                        ConnectionStatus::Connecting,
                        "heartbeat send failed; retrying",
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

    pub fn start_server<F>(&self, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + Sync + 'static,
    {
        self.disconnect();
        Self::transition_status(
            &self.status,
            &self.status_handler,
            ConnectionStatus::Connecting,
            "server listening",
        );
        let session_id = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let on_receive: ReceiveHandler = Arc::new(on_receive);

        let addr = format!("0.0.0.0:{}", port);
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(error) => {
                Self::transition_status(
                    &self.status,
                    &self.status_handler,
                    ConnectionStatus::Disconnected,
                    "server bind failed",
                );
                return Err(error.to_string());
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            Self::transition_status(
                &self.status,
                &self.status_handler,
                ConnectionStatus::Disconnected,
                "server listener setup failed",
            );
            return Err(error.to_string());
        }

        let sender = self.sender.clone();
        let receiver = self.receiver.clone();
        let status = self.status.clone();
        let status_handler = self.status_handler.clone();
        let session = self.session.clone();

        thread::spawn(move || loop {
            if session.load(Ordering::SeqCst) != session_id {
                return;
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    if Self::configure_connected_stream(&stream).is_err() {
                        continue;
                    }
                    if Self::write_control_frame(&mut stream, TYPE_ACK).is_err() {
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
                    *sender.lock().unwrap() = Some(send_stream);
                    *receiver.lock().unwrap() = Some(close_stream);
                    Self::transition_status(
                        &status,
                        &status_handler,
                        ConnectionStatus::Connected,
                        "peer accepted and acknowledged",
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
                    let connection_end =
                        Self::receive_loop(stream, session.clone(), session_id, on_receive.clone());
                    heartbeat_stop.store(true, Ordering::SeqCst);
                    if session.load(Ordering::SeqCst) != session_id {
                        return;
                    }
                    sender.lock().unwrap().take();
                    receiver.lock().unwrap().take();
                    Self::transition_status(
                        &status,
                        &status_handler,
                        ConnectionStatus::Connecting,
                        connection_end.reason(),
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
                            ConnectionStatus::Disconnected,
                            "server listener failed",
                        );
                    }
                    return;
                }
            }
        });

        Ok(())
    }

    pub fn connect_to_server<F>(&self, ip: &str, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + Sync + 'static,
    {
        self.disconnect();
        Self::transition_status(
            &self.status,
            &self.status_handler,
            ConnectionStatus::Connecting,
            "connecting to peer",
        );
        let session_id = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let target = ClientTarget {
            ip: ip.to_string(),
            port,
            on_receive: Arc::new(on_receive),
        };
        *self.client_target.lock().unwrap() = Some(target.clone());

        let sender = self.sender.clone();
        let receiver = self.receiver.clone();
        let status = self.status.clone();
        let status_handler = self.status_handler.clone();
        let session = self.session.clone();
        let client_target = self.client_target.clone();

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
                                ConnectionStatus::Connecting,
                                "protocol acknowledgement failed; retrying",
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
                        *sender.lock().unwrap() = Some(send_stream);
                        *receiver.lock().unwrap() = Some(close_stream);
                        Self::transition_status(
                            &status,
                            &status_handler,
                            ConnectionStatus::Connected,
                            "protocol ack received",
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
                        );
                        heartbeat_stop.store(true, Ordering::SeqCst);
                        if session.load(Ordering::SeqCst) != session_id {
                            return;
                        }
                        sender.lock().unwrap().take();
                        receiver.lock().unwrap().take();
                        Self::transition_status(
                            &status,
                            &status_handler,
                            ConnectionStatus::Connecting,
                            connection_end.reason(),
                        );
                    }
                    Err(_) => {
                        Self::transition_status(
                            &status,
                            &status_handler,
                            ConnectionStatus::Connecting,
                            "connect failed; retrying",
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

    pub fn send(&self, msg_type: u8, data: &[u8], sequence: u16) -> Result<(), String> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err("Message exceeds the 100 MB transfer limit".to_string());
        }
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

        if result.is_err() {
            self.close_streams();
            Self::transition_status(
                &self.status,
                &self.status_handler,
                self.status_after_connection_loss(),
                "send failed; retrying",
            );
        }
        result
    }

    pub fn send_text(&self, text: &str) -> Result<(), String> {
        self.send(TYPE_TEXT, text.as_bytes(), 0)
    }

    pub fn send_wechat(&self, message: &WeChatMessage) -> Result<(), String> {
        let payload = encode_wechat(message)?;
        self.send(TYPE_WECHAT, &payload, 0)
    }

    pub fn send_image(&self, width: usize, height: usize, data: &[u8]) -> Result<(), String> {
        let payload = encode_image(width, height, data)?;
        self.send(TYPE_IMAGE, &payload, 0)
    }

    pub fn send_file_data(&self, filename: &str, contents: &[u8]) -> Result<(), String> {
        let payload = encode_file(filename, contents)?;
        self.send(TYPE_FILE, &payload, 0)
    }

    pub fn send_file(
        &self,
        filename: &str,
        file_size: u64,
        chunks: impl Iterator<Item = Vec<u8>>,
    ) -> Result<(), String> {
        let metadata = FileMetadata {
            filename: filename.to_string(),
            file_size,
        };
        self.send(TYPE_FILE, &metadata.to_bytes(), 0)?;
        for (i, chunk) in chunks.enumerate() {
            self.send(TYPE_FILE, &chunk, (i + 1) as u16)?;
        }
        Ok(())
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
            .connect_to_server("127.0.0.1", port, |_, _| panic!("callback failed"))
            .unwrap();
        wait_for_connected(&[&server, &client]);

        server.send_text("trigger callback").unwrap();
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
            NetworkManager::write_control_frame(&mut stream, TYPE_ACK).unwrap();
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
            NetworkManager::write_control_frame(&mut stream, TYPE_ACK).unwrap();
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
            NetworkManager::write_control_frame(&mut first, TYPE_ACK).unwrap();
            accepted_tx.send(1).unwrap();
            thread::sleep(Duration::from_millis(100));
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            NetworkManager::write_control_frame(&mut second, TYPE_ACK).unwrap();
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
