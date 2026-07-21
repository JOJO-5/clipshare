use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::protocol::*;

const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
use crate::config::AppConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

type ReceiveHandler = Arc<dyn Fn(Vec<u8>, u8) + Send + Sync + 'static>;

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
        }
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_status(&self) -> ConnectionStatus {
        self.status.lock().unwrap().clone()
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
        *self.status.lock().unwrap() = ConnectionStatus::Disconnected;
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
    ) {
        loop {
            if session.load(Ordering::SeqCst) != session_id {
                break;
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
                        break;
                    }
                    let mut data = vec![0u8; h.data_len as usize];
                    if receiver.read_exact(&mut data).is_err() {
                        break;
                    }
                    on_receive(data, h.msg_type);
                }
                Err(_) => break,
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn start_server<F>(&self, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + Sync + 'static,
    {
        self.disconnect();
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;
        let session_id = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let on_receive: ReceiveHandler = Arc::new(on_receive);

        let addr = format!("0.0.0.0:{}", port);
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(error) => {
                *self.status.lock().unwrap() = ConnectionStatus::Disconnected;
                return Err(error.to_string());
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            *self.status.lock().unwrap() = ConnectionStatus::Disconnected;
            return Err(error.to_string());
        }

        let sender = self.sender.clone();
        let receiver = self.receiver.clone();
        let status = self.status.clone();
        let session = self.session.clone();

        thread::spawn(move || loop {
            if session.load(Ordering::SeqCst) != session_id {
                return;
            }

            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).ok();
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
                    *status.lock().unwrap() = ConnectionStatus::Connected;
                    Self::receive_loop(stream, session.clone(), session_id, on_receive.clone());
                    if session.load(Ordering::SeqCst) != session_id {
                        return;
                    }
                    sender.lock().unwrap().take();
                    receiver.lock().unwrap().take();
                    *status.lock().unwrap() = ConnectionStatus::Connecting;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {
                    if session.load(Ordering::SeqCst) == session_id {
                        *status.lock().unwrap() = ConnectionStatus::Disconnected;
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
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;
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
        let session = self.session.clone();
        let client_target = self.client_target.clone();

        thread::spawn(move || {
            let mut retry_delay = Duration::from_millis(250);
            loop {
                if session.load(Ordering::SeqCst) != session_id {
                    return;
                }

                let addr = format!("{}:{}", target.ip, target.port);
                match TcpStream::connect(&addr) {
                    Ok(stream) => {
                        stream.set_nonblocking(false).ok();
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
                        *status.lock().unwrap() = ConnectionStatus::Connected;
                        retry_delay = Duration::from_millis(250);

                        Self::receive_loop(
                            stream,
                            session.clone(),
                            session_id,
                            target.on_receive.clone(),
                        );
                        if session.load(Ordering::SeqCst) != session_id {
                            return;
                        }
                        sender.lock().unwrap().take();
                        receiver.lock().unwrap().take();
                        *status.lock().unwrap() = ConnectionStatus::Connecting;
                    }
                    Err(_) => {
                        *status.lock().unwrap() = ConnectionStatus::Connecting;
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
            *self.status.lock().unwrap() = self.status_after_connection_loss();
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
}
