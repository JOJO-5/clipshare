use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, Shutdown};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
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

pub struct NetworkManager {
    pub status: Arc<Mutex<ConnectionStatus>>,
    sender: Arc<Mutex<Option<TcpStream>>>,
    receiver: Arc<Mutex<Option<TcpStream>>>,
    config: Arc<Mutex<AppConfig>>,
    session: Arc<AtomicU64>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            sender: Arc::new(Mutex::new(None)),
            receiver: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(AppConfig::load())),
            session: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_status(&self) -> ConnectionStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn disconnect(&self) {
        self.session.fetch_add(1, Ordering::SeqCst);
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
        *self.status.lock().unwrap() = ConnectionStatus::Disconnected;
    }

    fn receive_loop(
        mut receiver: TcpStream,
        status: Arc<Mutex<ConnectionStatus>>,
        session: Arc<AtomicU64>,
        session_id: u64,
        on_receive: Box<dyn Fn(Vec<u8>, u8) + Send + 'static>,
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
                        if session.load(Ordering::SeqCst) == session_id {
                            *status.lock().unwrap() = ConnectionStatus::Disconnected;
                        }
                        break;
                    }
                    let mut data = vec![0u8; h.data_len as usize];
                    if receiver.read_exact(&mut data).is_err() {
                        if session.load(Ordering::SeqCst) == session_id {
                            *status.lock().unwrap() = ConnectionStatus::Disconnected;
                        }
                        break;
                    }
                    on_receive(data, h.msg_type);
                }
                Err(_) => {
                    if session.load(Ordering::SeqCst) == session_id {
                        *status.lock().unwrap() = ConnectionStatus::Disconnected;
                    }
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn start_server<F>(&self, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + 'static,
    {
        self.disconnect();
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;
        let session_id = self.session.fetch_add(1, Ordering::SeqCst) + 1;

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

        thread::spawn(move || {
            loop {
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
                        Self::receive_loop(stream, status, session, session_id, Box::new(on_receive));
                        return;
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
            }
        });

        Ok(())
    }

    pub fn connect_to_server<F>(&self, ip: &str, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + 'static,
    {
        self.disconnect();
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;
        let session_id = self.session.fetch_add(1, Ordering::SeqCst) + 1;

        let addr = format!("{}:{}", ip, port);
        let stream = match TcpStream::connect(&addr) {
            Ok(stream) => stream,
            Err(error) => {
                *self.status.lock().unwrap() = ConnectionStatus::Disconnected;
                return Err(error.to_string());
            }
        };
        stream.set_nonblocking(false).ok();
        let send_stream = stream.try_clone().map_err(|e| e.to_string())?;
        let close_stream = stream.try_clone().map_err(|e| e.to_string())?;
        *self.sender.lock().unwrap() = Some(send_stream);
        *self.receiver.lock().unwrap() = Some(close_stream);
        *self.status.lock().unwrap() = ConnectionStatus::Connected;

        let status = self.status.clone();
        let session = self.session.clone();

        thread::spawn(move || {
            Self::receive_loop(stream, status, session, session_id, Box::new(on_receive));
        });

        Ok(())
    }

    pub fn send(&self, msg_type: u8, data: &[u8], sequence: u16) -> Result<(), String> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err("Message exceeds the 100 MB transfer limit".to_string());
        }
        let mut sender = self.sender.lock().unwrap();
        if let Some(ref mut stream) = *sender {
            let header = MessageHeader::new(msg_type, data.len() as u32, sequence);
            stream.write_all(&header.to_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(data).map_err(|e| e.to_string())?;
            stream.flush().map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("Not connected".to_string())
        }
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

    pub fn send_file(&self, filename: &str, file_size: u64, chunks: impl Iterator<Item = Vec<u8>>) -> Result<(), String> {
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
    fn default() -> Self { Self::new() }
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
}
