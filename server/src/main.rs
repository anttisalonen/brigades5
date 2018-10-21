extern crate actix;
extern crate actix_web;
extern crate env_logger;
extern crate ds;
extern crate rmp_serde;
extern crate serde;

use std::time::{Instant, Duration};
use std::collections::HashSet;

use actix::prelude::*;
use actix_web::{
	fs, http, middleware, server, ws, App, Error, HttpRequest, HttpResponse,
};

use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};

mod osm;

struct Soldier {
	id: ds::SoldierID,
	pos: ds::Position,
	alive: bool,
	controller: Option<Addr<MyWebSocket>>,
	moving: Option<ds::Position>,
}

impl Soldier {
	fn new(sid: ds::SoldierID) -> Soldier {
		Soldier {
			id: sid,
			pos: ds::Position { x: 0.0, y: 0.0 },
			alive: false,
			controller: None,
			moving: None,
		}
	}

	fn is_available(&self) -> bool {
		self.alive && self.controller.is_none()
	}

	fn try_move(&mut self, time: Duration) -> Option<(Addr<MyWebSocket>, ds::ServerMsg)> {
		match self.moving {
			Some(pos) => {
				match &self.controller {
					Some(c) => {
						Some((c.to_owned(), self.move_soldier(time, pos)))
					}
					None => None
				}

			}
			None => None
		}
	}

	fn move_soldier(&mut self, time: Duration, tgtpos: ds::Position) -> ds::ServerMsg {
		if self.pos.dist(&tgtpos) < 1.0 {
			self.moving = None;
		} else {
			self.pos.add(self.pos.to_pos(&tgtpos).normalized(), WALKING_SPEED, time);
		}
		ds::ServerMsg::YourPosition(self.id, self.pos)
	}
}

struct GameState {
	soldiers: Vec<Soldier>,
}

impl GameState {
	fn new() -> GameState {
		let mut v: Vec<Soldier> = Vec::new();
		for i in 1..4 {
			let mut pl = Soldier::new(ds::SoldierID(i));
			if i == 1 {
				pl.alive = true;
			}
			v.push(pl);
		}
		GameState {
			soldiers: v
		}
	}

	fn available_soldiers(&self) -> Vec<ds::SoldierID> {
		self.soldiers.iter()
			.filter(|p| p.is_available())
			.map(|p| p.id)
			.collect()
	}

	fn get_soldier_mut(&mut self, sid: ds::SoldierID) -> Option<&mut Soldier> {
		for s in &mut self.soldiers {
			if s.id == sid {
				return Some(&mut *s);
			}
		}
		None
	}

	fn update_soldier<F>(&mut self, sid: ds::SoldierID, action: F)
		where F: Fn(&mut Soldier) {
			for s in &mut self.soldiers {
				if s.id == sid {
					action(&mut *s);
					return;
				}
			}
		}

	fn try_take_control(&mut self, sid: ds::SoldierID, from: &Addr<MyWebSocket>) -> bool {
		match self.get_soldier_mut(sid) {
			Some(s) => {
				if s.is_available() {
					s.controller = Some(from.to_owned());
					from.do_send(ServerMsg {
						msg: ds::ServerMsg::YouNowHaveControl(sid)
					});
					true
				} else {
					false
				}
			}
			None => false
		}
	}

	fn handle_take_control(&mut self, sid: ds::SoldierID, from: &Addr<MyWebSocket>) {
		let os = self.try_take_control(sid, &from);
		match os {
			true => {
				for ref s in &self.soldiers {
					if s.id == sid {
						let c = s.controller.as_ref().unwrap();
						let seen = self.current_percepts(&s);
						from.do_send(ServerMsg {
							msg: ds::ServerMsg::SoldierSeen(seen)
						});
						break;
					}
				}
			}
			false => ()
		}
	}

	fn current_percepts(&self, s: &Soldier) -> Vec<(ds::SoldierID, ds::Position)> {
		vec![]
	}
}

struct SessionState {
	server: Addr<ChatServer>,
}

struct ChatServer {
	clients: HashSet<Addr<MyWebSocket>>,
	game: GameState,
}

const UPDATE_INTERVAL: Duration = Duration::from_millis(100);
const WALKING_SPEED: ds::Speed = ds::Speed { speed: 1.0 };

impl Handler<UpdateMessage> for ChatServer {
	type Result = ();

	fn handle(&mut self, _msg: UpdateMessage, _ctx: &mut Context<Self>) -> Self::Result {
		let mut msgs: Vec<(Addr<MyWebSocket>, ds::ServerMsg)> = Vec::new();

		for s in self.game.soldiers.iter_mut() {
			let opt_msg = s.try_move(UPDATE_INTERVAL);
			match opt_msg {
				Some(m) => {
					msgs.push(m);
				}
				None    => ()
			}
		}
		for msg in msgs.into_iter() {
			msg.0.do_send(ServerMsg { msg: msg.1 });
		}
	}
}

impl ChatServer {
	fn update(&self, ctx: &mut <Self as Actor>::Context) {
		ctx.run_interval(UPDATE_INTERVAL, |_act, ct| {
			ct.address().do_send(UpdateMessage);
		});
	}
}

#[derive(Message)]
struct Connect {
	addr: Addr<MyWebSocket>
}

#[derive(Message)]
struct ClientDisconnected {
	client: Addr<MyWebSocket>
}

#[derive(Message)]
struct UpdateMessage;

impl Default for ChatServer {
	fn default() -> ChatServer {
		ChatServer {
			clients: HashSet::new(),
			game: GameState::new(),
		}
	}
}

impl Actor for ChatServer {
	type Context = Context<Self>;

	fn started(&mut self, ctx: &mut Self::Context) {
		self.update(ctx);
	}
}

fn main() {
	let sys = actix::System::new("websocket-example");
	osm::run_osm();
	::std::env::set_var("RUST_LOG", "actix_web=info");
	env_logger::init();
	let addr = "0.0.0.0:8080";
	println!("Starting server at {}", addr);
	let chatserver = Arbiter::start(|_| ChatServer::default());
	server::new(
		move || {
			let state = SessionState {
				server: chatserver.clone(),
			};

		App::with_state(state)
		.middleware(middleware::Logger::default())
		.resource("/ws/", |r| r.method(http::Method::GET).f(ws_index))
		.handler(
			"/",
			fs::StaticFiles::new("target/deploy").unwrap().index_file("index.html"))}
		)
		.bind(addr).unwrap()
		.start();
	let _ = sys.run();
}

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// do websocket handshake and start `MyWebSocket` actor
fn ws_index(r: &HttpRequest<SessionState>) -> Result<HttpResponse, Error> {
	ws::start(r, MyWebSocket::new())
}

/// websocket connection is long running connection, it easier
/// to handle with an actor
struct MyWebSocket {
	/// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
	/// otherwise we drop connection.
	hb: Instant,
}

impl Actor for MyWebSocket {
	type Context = ws::WebsocketContext<Self, SessionState>;

	/// Method is called on actor start. We start the heartbeat process here.
	fn started(&mut self, ctx: &mut Self::Context) {
		self.hb(ctx);
		ctx.state().server.do_send(Connect {
			addr: ctx.address(),
		});
	}

	fn stopped(&mut self, ctx: &mut Self::Context) {
		ctx.state().server.do_send(ClientDisconnected {
			client: ctx.address(),
		});
	}
}

impl Handler<ClientDisconnected> for ChatServer {
	type Result = ();

	fn handle(&mut self, msg: ClientDisconnected, _: &mut Context<Self>) -> Self::Result {
		self.clients.remove(&msg.client);
		println!("client disconnected");
		for ref mut s in &mut self.game.soldiers {
			match &s.controller {
				Some(c) => {
					if c == &msg.client {
						s.controller = None;
					}
				}
				None => ()
			}
		}
	}
}

impl Handler<Connect> for ChatServer {
	type Result = ();

	fn handle(&mut self, msg: Connect, _: &mut Context<Self>) -> Self::Result {
		self.clients.insert(msg.addr);
		()
	}
}

#[derive(Message)]
struct ClientMessage {
	msg: String,
}

impl Handler<ClientMessage> for ChatServer {
	type Result = ();

	fn handle(&mut self, msg: ClientMessage, _: &mut Self::Context) {
		for addr in &self.clients {
			addr.do_send(ChatMessage { msg: msg.msg.to_owned() } );
		}
	}
}

#[derive(Message)]
struct GameMsg {
	from: Addr<MyWebSocket>,
	msg: ds::GameMsg
}

#[derive(Message)]
struct ServerMsg {
	msg: ds::ServerMsg
}

impl Handler<GameMsg> for ChatServer {
	type Result = ();

	fn handle(&mut self, msg: GameMsg, _ctx: &mut Self::Context) {
		match msg.msg {
			ds::GameMsg::Init(v) => {
				self.game = GameState::new();
				let val = ds::ServerMsg::NewGame(self.game.available_soldiers());
				msg.from.do_send(ServerMsg {
					msg: val
				});
			}
			ds::GameMsg::TakeControl(sid) => {
				self.game.handle_take_control(sid, &msg.from);
			}
			ds::GameMsg::QueryStatus => {
				let val = ds::ServerMsg::AvailableSoldiers(self.game.available_soldiers());
				msg.from.do_send(ServerMsg { msg: val });
			}
			ds::GameMsg::MoveTo(sid, pos) => {
				for ref mut s in &mut self.game.soldiers {
					if s.id == sid {
						if let Some(c) = &s.controller {
							if c == &msg.from {
								s.moving = Some(pos);
								break;
							}
						}
					}
				}
			}
		}
	}
}

#[derive(Message)]
struct ChatMessage {
	msg: String,
}

impl Handler<ChatMessage> for MyWebSocket {
	type Result = ();

	fn handle(&mut self, msg: ChatMessage, ctx: &mut Self::Context) {
		ctx.text(msg.msg);
	}
}

impl Handler<ServerMsg> for MyWebSocket {
	type Result = ();

	fn handle(&mut self, msg: ServerMsg, ctx: &mut Self::Context) {
		let mut buf = Vec::new();
		msg.msg.serialize(&mut Serializer::new(&mut buf)).unwrap();
		ctx.binary(buf);
	}
}

/// Handler for `ws::Message`
impl StreamHandler<ws::Message, ws::ProtocolError> for MyWebSocket {
	fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
		// process websocket messages
		match msg {
			ws::Message::Ping(msg) => {
				self.hb = Instant::now();
				ctx.pong(&msg);
			}
			ws::Message::Pong(_) => {
				self.hb = Instant::now();
			}
			ws::Message::Text(text) => {
				ctx.state().server.do_send(ClientMessage {
					msg: text,
				});
			}
			ws::Message::Binary(mut bin) => {
				let inp = &bin.take()[..];
				let mut de = Deserializer::new(inp);
				let msg: Result<ds::GameMsg, rmp_serde::decode::Error> = Deserialize::deserialize(&mut de);
				match msg {
					Ok(gmsg) => {
						println!("Got gmsg {:?}", gmsg);
						ctx.state().server.do_send(GameMsg {
							from: ctx.address(),
							msg: gmsg,
						});
					}
					Err(e) => {
						println!("Error: {:?}\n", e);
					}
				}
			}
			ws::Message::Close(_) => {
				ctx.state().server.do_send(ClientDisconnected {
					client: ctx.address(),
				});

				ctx.stop();
			}
		}
	}
}

impl MyWebSocket {
	fn new() -> Self {
		Self { hb: Instant::now() }
	}

	/// helper method that sends ping to client every second.
	///
	/// also this method checks heartbeats from client
	fn hb(&self, ctx: &mut <Self as Actor>::Context) {
		ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
			// check client heartbeats
			if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
				// heartbeat timed out
				println!("Websocket Client heartbeat failed, disconnecting!");

				ctx.state().server.do_send(ClientDisconnected {
					client: ctx.address(),
				});

				// stop actor
				ctx.stop();

				// don't try to send a ping
				return;
			}

			ctx.ping("");
		});
	}
}

