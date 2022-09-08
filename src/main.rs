use std::{
  error::Error,
  net::{SocketAddr, TcpListener},
};

mod command_info;

fn main() -> Result<(), Box<dyn Error>> {
  dotenv::dotenv()?;
  env_logger::init();
  let server_addr: SocketAddr = std::env::var("COMMAND_SERVER_ADDRESS")?.parse()?;
  let listener = TcpListener::bind(server_addr)?;
  log::info!("Server is running on {}", server_addr);
  loop {
    for client in listener.accept() {
      std::thread::spawn(move || {
        if let Err(why) = command_info::handle(client) {
          log::error!("{}", why);
        }
      });
    }
  }
}
