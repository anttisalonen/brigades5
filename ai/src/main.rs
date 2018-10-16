extern crate ws;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rmp_serde;

extern crate ds;

use ws::{connect, CloseCode};

use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};

enum State {
	Init,
	Done
}

struct Client {
	out: ws::Sender,
	state: State,
	sid: Option<ds::SoldierID>,
}

impl Client {
	fn send(&mut self, msg: ds::GameMsg) -> ws::Result<()> {
		let mut buf = Vec::new();
		msg.serialize(&mut Serializer::new(&mut buf)).unwrap();
		self.out.send(buf)
	}
}

impl ws::Handler for Client {
	fn on_open(&mut self, _: ws::Handshake) -> ws::Result<()> {
		// self.send(ds::GameMsg::Init(1))
		self.send(ds::GameMsg::QueryStatus)
	}

	fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
		println!("Got message: {}", msg);
		match msg {
			ws::Message::Text(t) => {
				self.out.close(CloseCode::Normal)
			}
			ws::Message::Binary(b) => {
				let inp = &b[..];
				let mut de = Deserializer::new(inp);
				let msg = Deserialize::deserialize(&mut de);
				println!("Got msg {:?}", msg);
				match msg {
					Ok(ds::ServerMsg::NewGame(soldiers)) => {
						if soldiers.len() > 0 {
							self.send(ds::GameMsg::TakeControl(soldiers[0]))
						} else {
							self.out.close(CloseCode::Normal)
						}
					}
					Ok(ds::ServerMsg::AvailableSoldiers(soldiers)) => {
						if soldiers.len() > 0 {
							self.send(ds::GameMsg::TakeControl(soldiers[0]))
						} else {
							self.out.close(CloseCode::Normal)
						}
					}
					Ok(ds::ServerMsg::YouNowHaveControl(sid)) => {
						self.sid = Some(sid);
						self.send(ds::GameMsg::MoveTo(self.sid.unwrap(),
									      ds::Position {
										      x: 5.0,
										      y: 0.0
									      }))
					}
					Ok(ds::ServerMsg::YourPosition(sid, pos)) => {
						if pos.dist(&ds::Position {
							x: 5.0,
							y: 0.0
						}) < 1.0 {
							self.out.close(CloseCode::Normal)
						} else {
							std::result::Result::Ok(())
						}
					}
					Err(e) => {
						println!("Error: {:?}\n", e);
						self.out.close(CloseCode::Normal)
					}
				}
			}
		}
	}
}

fn main() {
	connect("ws://127.0.0.1:8080/ws/", |out| Client {
		out: out,
		state: State::Init,
		sid: None,
	}).unwrap()
}
