use std::{
  collections::HashMap,
  error::Error,
  net::{SocketAddr, TcpListener},
  sync::Mutex,
};

mod command_info;
mod utils;

fn main() -> Result<(), Box<dyn Error>> {
  dotenv::dotenv()?;
  env_logger::init();
  let server_addr: SocketAddr = std::env::var("COMMAND_SERVER_ADDRESS")?.parse()?;
  let listener = TcpListener::bind(server_addr)?;
  let hosts = Mutex::new(HashMap::new());
  log::info!("Server is running on {}", server_addr);
  std::thread::scope(|scope| loop {
    for client in listener.accept() {
      let hosts = &hosts;
      scope.spawn(move || {
        if let Err(why) = command_info::handle(client, hosts) {
          log::error!("{}", why);
        }
      });
    }
  });
  unreachable!()
}
