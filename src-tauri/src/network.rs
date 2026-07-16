use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, Shutdown};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::protocol::*;
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
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            sender: Arc::new(Mutex::new(None)),
            receiver: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(AppConfig::load())),
        }
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_status(&self) -> ConnectionStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn disconnect(&self) {
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

    fn receive_loop(receiver: Arc<Mutex<Option<TcpStream>>>, status: Arc<Mutex<ConnectionStatus>>, on_receive: Box<dyn Fn(Vec<u8>, u8) + Send + 'static>) {
        loop {
            let header_result = {
                let mut r = receiver.lock().unwrap();
                match &mut *r {
                    Some(s) => {
                        let mut header_buf = [0u8; HEADER_SIZE];
                        match s.read_exact(&mut header_buf) {
                            Ok(()) => {
                                let mut c = std::io::Cursor::new(&header_buf);
                                MessageHeader::from_reader(&mut c).ok()
                            }
                            Err(_) => None,
                        }
                    }
                    None => None,
                }
            };

            match header_result {
                Some(h) => {
                    if h.msg_type == TYPE_HEARTBEAT {
                        continue;
                    }
                    let mut data = vec![0u8; h.data_len as usize];
                    let read_result = {
                        let mut r = receiver.lock().unwrap();
                        match &mut *r {
                            Some(s) => s.read_exact(&mut data).ok(),
                            None => None,
                        }
                    };
                    if read_result.is_none() {
                        *status.lock().unwrap() = ConnectionStatus::Disconnected;
                        break;
                    }
                    on_receive(data, h.msg_type);
                }
                None => {
                    *status.lock().unwrap() = ConnectionStatus::Disconnected;
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

        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;

        let sender = self.sender.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                stream.set_nonblocking(false).ok();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                *sender.lock().unwrap() = Some(stream.try_clone().unwrap());
                *status.lock().unwrap() = ConnectionStatus::Connected;
            }
        });

        let receiver2 = self.receiver.clone();
        let status2 = self.status.clone();

        thread::spawn(move || {
            Self::receive_loop(receiver2, status2, Box::new(on_receive));
        });

        Ok(())
    }

    pub fn connect_to_server<F>(&self, ip: &str, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + 'static,
    {
        self.disconnect();
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;

        let addr = format!("{}:{}", ip, port);
        let stream = TcpStream::connect(&addr).map_err(|e| e.to_string())?;
        stream.set_nonblocking(false).ok();
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));

        *self.sender.lock().unwrap() = Some(stream.try_clone().unwrap());
        *self.receiver.lock().unwrap() = Some(stream);
        *self.status.lock().unwrap() = ConnectionStatus::Connected;

        let receiver = self.receiver.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            Self::receive_loop(receiver, status, Box::new(on_receive));
        });

        Ok(())
    }

    pub fn send(&self, msg_type: u8, data: &[u8], sequence: u16) -> Result<(), String> {
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

    pub fn send_image(&self, data: &[u8]) -> Result<(), String> {
        self.send(TYPE_IMAGE, data, 0)
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
