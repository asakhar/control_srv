use std::{
  collections::HashMap,
  convert::TryInto,
  error::Error,
  fmt::Display,
  io::{Read, Write},
  net::{IpAddr, SocketAddr, TcpStream},
  sync::{mpsc, Mutex},
};

use crate::utils::ReadSizedExt;

#[derive(Debug, Clone, Copy)]
enum ClientType {
  Client,
  Host,
}
impl TryFrom<u8> for ClientType {
  type Error = ClientIdentError;
  fn try_from(buf: u8) -> Result<Self, ClientIdentError> {
    Ok(match buf {
      1 => Self::Host,
      2 => Self::Client,
      _ => return Err(ClientIdentError::InvalidByte),
    })
  }
}

#[derive(Debug, Clone, Copy)]
enum ClientIdentError {
  InvalidByte,
  HostNotConnected,
  FailedToWrite,
}

impl Display for ClientIdentError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      ClientIdentError::InvalidByte => "Invalid byte received for client type",
      ClientIdentError::HostNotConnected => "Host is not connected yet",
      ClientIdentError::FailedToWrite => "Failed to write to host",
    })
  }
}

impl Error for ClientIdentError {}

#[derive(Debug)]
pub struct DataBlock {
  #[allow(dead_code)]
  client: IpAddr,
  message: Vec<u8>,
}

#[derive(Debug)]
pub struct HostChannelInner {
  sender: Mutex<mpsc::Sender<DataBlock>>,
  receiver: Mutex<mpsc::Receiver<DataBlock>>,
}

#[derive(Debug, Clone, Copy)]
pub struct HostChannel {
  inner: *const HostChannelInner,
}

unsafe impl Send for HostChannel {}

impl HostChannel {
  pub fn new() -> Self {
    let (sender, receiver) = mpsc::channel();
    let receiver = Mutex::new(receiver);
    let sender = Mutex::new(sender);
    Self {
      inner: Box::into_raw(Box::new(HostChannelInner { sender, receiver })),
    }
  }

  pub fn deref(&self) -> &'static HostChannelInner {
    unsafe { &*self.inner }
  }
}

pub fn handle(
  client: (TcpStream, SocketAddr),
  hosts: &Mutex<HashMap<String, HostChannel>>,
) -> Result<(), Box<dyn Error>> {
  let (mut stream, addr) = client;
  log::info!("Client {} tries to connect", addr);
  let mut buffer = [0u8; 1];
  stream.read_exact(&mut buffer)?;
  let requested_type: ClientType = buffer[0].try_into()?;
  log::info!("Client pretended to be {requested_type:?}");
  match requested_type {
    ClientType::Host => {
      let host_name = String::from_utf8(stream.read_sized()?)?;
      eprintln!("Pretended hostname: {host_name:?}");

      let mut hosts_lock = hosts.lock().expect("Got poisoned");

      let host_channel = hosts_lock
        .entry(host_name)
        .or_insert_with(HostChannel::new)
        .deref();

      drop(hosts_lock);
      stream.write_all(&[1u8; 1])?;
      let res = handle_host(stream, &host_channel);
      log::info!("Closed connection to host: {}", client.1);
      res
    }
    ClientType::Client => {
      let host_name = String::from_utf8(stream.read_sized()?)?;
      eprintln!("Requested hostname: {host_name:?}");
      let hosts_lock = hosts.lock().expect("Got poisoned");
      if let Some(host_channel) = hosts_lock.get(&host_name) {
        let host_channel = host_channel.deref();
        drop(hosts_lock);
        stream.write_all(&[1u8; 1])?;
        handle_client(stream, client.1.ip(), &*host_channel)
      } else {
        stream.write_all(&[0u8; 1])?;
        Err(Box::new(ClientIdentError::HostNotConnected))
      }
    }
  }
}

fn handle_host(
  mut stream: TcpStream,
  host_channel: &HostChannelInner,
) -> Result<(), Box<dyn Error>> {
  let mut buf = [0u8; 1];
  let receiver = match host_channel.receiver.try_lock() {
    Ok(recv) => recv,
    Err(std::sync::TryLockError::WouldBlock) => {
      return Err(Box::new(
        std::sync::TryLockError::<mpsc::Receiver<DataBlock>>::WouldBlock,
      ))
    }
    _ => unreachable!("Got poisoned"),
  };
  'outer: while stream.read_exact(&mut buf).is_ok() && buf[0] != 2 {
    match buf[0] {
      0 => continue,
      1 => {}
      _ => return Err(Box::new(ClientIdentError::InvalidByte)),
    }

    let fetch_start = std::time::SystemTime::now();
    let data_block = loop {
      let res = receiver.try_recv();
      match res {
        Ok(db) => break db,
        Err(mpsc::TryRecvError::Empty) => {
          if std::time::SystemTime::now()
            .duration_since(fetch_start)
            .expect("fetch_start was later then now()")
            .as_millis()
            > 100
          {
            stream.write_all(&[0])?;
            continue 'outer;
          }
        }
        _ => unreachable!("Sender disconnected"),
      }
    };
    log::info!("Data block is being sent to host: {data_block:?}");
    stream.write_all(&[data_block.message.len() as u8])?;
    if stream.write_all(&data_block.message).is_err() {
      return Err(Box::new(ClientIdentError::FailedToWrite));
    }
  }
  stream.write_all(&[2])?;
  Ok(())
}

fn handle_client(
  mut stream: TcpStream,
  client: IpAddr,
  host_channel: &HostChannelInner,
) -> Result<(), Box<dyn Error>> {
  let message = stream.read_sized()?;
  let sender = host_channel.sender.lock().expect("Got poisoned").clone();
  log::info!("Message read from client({client}): {message:?}");
  sender.send(DataBlock { message, client })?;
  Ok(())
}
