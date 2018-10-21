extern crate actix;
extern crate actix_web;
extern crate ds;
extern crate rmp_serde;
extern crate serde;

use std::time::{Instant, Duration};
use actix::prelude::*;
use actix_web::{
	ws, Error, HttpRequest, HttpResponse,
};

use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// do websocket handshake and start `MyWebSocket` actor
pub fn ws_index(r: &HttpRequest<Recipient<WebSocketMsg>>) -> Result<HttpResponse, Error> {
	ws::start(r, MyWebSocket::new())
}

/// websocket connection is long running connection, it easier
/// to handle with an actor
pub struct MyWebSocket {
	/// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
	/// otherwise we drop connection.
	hb: Instant,
}

#[allow(unused_must_use)]
impl Actor for MyWebSocket {
	type Context = ws::WebsocketContext<Self, Recipient<WebSocketMsg>>;

	/// Method is called on actor start. We start the heartbeat process here.
	fn started(&mut self, ctx: &mut Self::Context) {
		self.hb(ctx);
		ctx.state().do_send(WebSocketMsg::Connected(ctx.address()));
	}

	fn stopped(&mut self, ctx: &mut Self::Context) {
		ctx.state().do_send(WebSocketMsg::Disconnected(ctx.address()));
	}
}

#[allow(unused_must_use)]
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

				ctx.state().do_send(WebSocketMsg::Disconnected(ctx.address()));

				// stop actor
				ctx.stop();

				// don't try to send a ping
				return;
			}

			ctx.ping("");
		});
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

#[allow(unused_must_use)]
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
			ws::Message::Text(_) => {
				()
			}
			ws::Message::Binary(mut bin) => {
				let inp = &bin.take()[..];
				let mut de = Deserializer::new(inp);
				let msg: Result<ds::GameMsg, rmp_serde::decode::Error> = Deserialize::deserialize(&mut de);
				match msg {
					Ok(gmsg) => {
						println!("Got gmsg {:?}", gmsg);
						ctx.state().do_send(WebSocketMsg::IncomingData(ctx.address(), gmsg));
					}
					Err(e) => {
						println!("Error: {:?}\n", e);
					}
				}
			}
			ws::Message::Close(_) => {
				ctx.state().do_send(WebSocketMsg::Disconnected(ctx.address()));

				ctx.stop();
			}
		}
	}
}

#[derive(Message)]
pub enum WebSocketMsg {
	Connected(Addr<MyWebSocket>),
	Disconnected(Addr<MyWebSocket>),
	IncomingData(Addr<MyWebSocket>, ds::GameMsg)
}

#[derive(Message)]
pub struct ServerMsg {
	pub msg: ds::ServerMsg
}
