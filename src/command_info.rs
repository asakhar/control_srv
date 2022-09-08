use std::{
  convert::TryInto,
  error::Error,
  fmt::Display,
  io::{Read, Write},
  net::{SocketAddr, TcpStream},
  sync::{mpsc, Mutex},
};

#[derive(Debug, Clone)]
enum CommandInfo {
  Next,
  Prev,
  Pause,
  Vol(f64),
}

impl TryFrom<[u8; 16]> for CommandInfo {
  type Error = ClientIdentError;
  fn try_from(buf: [u8; 16]) -> Result<Self, ClientIdentError> {
    Ok(match buf {
      [1, 0, 0, 0, 0, 0, 0, 0, ..] => Self::Next,
      [2, 0, 0, 0, 0, 0, 0, 0, ..] => Self::Prev,
      [3, 0, 0, 0, 0, 0, 0, 0, ..] => Self::Pause,
      [4, 0, 0, 0, 0, 0, 0, 0, ..] => Self::Vol(f64::from_le_bytes(buf[8..16].try_into().unwrap())),
      _ => return Err(ClientIdentError::InvalidByte),
    })
  }
}

impl From<CommandInfo> for [u8; 16] {
  fn from(command: CommandInfo) -> Self {
    let mut buffer = [0u8; 16];
    buffer[0] = match command {
      CommandInfo::Next => 1,
      CommandInfo::Prev => 2,
      CommandInfo::Pause => 3,
      CommandInfo::Vol(n) => {
        buffer[8..16].clone_from_slice(&n.to_le_bytes());
        4
      }
    };
    buffer
  }
}

#[derive(Debug, Clone, Copy)]
enum ClientType {
  Client,
  Host,
}

#[derive(Debug, Clone, Copy)]
enum ClientIdentError {
  InvalidByte,
  AlreadyExists,
  HostNotConnected,
  FailedToWrite,
}

impl Display for ClientIdentError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      ClientIdentError::InvalidByte => "Invalid byte received for client type",
      ClientIdentError::AlreadyExists => "Host with different address already exists",
      ClientIdentError::HostNotConnected => "Host is not connected yet",
      ClientIdentError::FailedToWrite => "Failed to write to host",
    })
  }
}

impl Error for ClientIdentError {}

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

static HOST_CHANNEL: Mutex<Option<mpsc::Sender<CommandInfo>>> = Mutex::new(None);

pub fn handle(client: (TcpStream, SocketAddr)) -> Result<(), Box<dyn Error>> {
  let (mut stream, addr) = client;
  log::info!("Client {} tries to connect", addr);
  let mut buffer = [0u8; 1];
  stream.read_exact(&mut buffer)?;
  let requested_type: ClientType = buffer[0].try_into()?;
  log::info!("Client pretended to be {requested_type:?}");
  match requested_type {
    ClientType::Host => {
      if HOST_CHANNEL.lock().expect("Mutex got poisoned").is_some() {
        stream.write_all(&[0u8; 1])?;
        Err(ClientIdentError::AlreadyExists)?;
      }
      stream.write_all(&[1u8; 1])?;
      let res = handle_host(stream);
      *HOST_CHANNEL.lock().expect("Mutex got poisoned") = None;
      if res.is_err() {
        return res;
      }
    }
    ClientType::Client => match *HOST_CHANNEL.lock().expect("Mutex got poisoned") {
      Some(ref sender) => {
        stream.write_all(&[1u8; 1])?;
        log::info!("Ready written to client");
        handle_client(stream, sender.clone())?;
      }
      None => {
        stream.write_all(&[0u8; 1])?;
        Err(ClientIdentError::HostNotConnected)?;
      }
    },
  }
  Ok(())
}

fn handle_host(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
  let (sender, receiver) = mpsc::channel();
  *HOST_CHANNEL.lock().expect("Mutex got poisoned") = Some(sender);
  let mut buf = [0u8; 1];

  while stream.read_exact(&mut buf).is_ok() && buf[0] != 2 {
    match buf[0] {
      0 => continue,
      1 => {}
      _ => return Err(Box::new(ClientIdentError::InvalidByte)),
    }

    let command: [u8; 16] = match receiver.try_recv() {
      Ok(command) => command.into(),
      Err(mpsc::TryRecvError::Empty) => {
        stream.write_all(&[0])?;
        continue;
      }
      _ => unreachable!("Sender disconnected"),
    };
    stream.write_all(&[1])?;
    if stream.write_all(&command).is_err() {
      return Err(Box::new(ClientIdentError::FailedToWrite));
    }
  }
  stream.write_all(&[2])?;
  Ok(())
}

fn handle_client(
  mut stream: TcpStream,
  sender: mpsc::Sender<CommandInfo>,
) -> Result<(), Box<dyn Error>> {
  let mut buffer = [0u8; 16];
  stream.read_exact(&mut buffer)?;
  let command = CommandInfo::try_from(buffer)?;
  log::info!("Command read from client: {command:?}");
  sender.send(command)?;
  Ok(())
}
