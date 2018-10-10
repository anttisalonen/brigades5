extern crate actix;
extern crate actix_web;
extern crate env_logger;

use std::time::{Instant, Duration};
use std::collections::HashSet;

use actix::prelude::*;
use actix_web::{
	fs, http, middleware, server, ws, App, Error, HttpRequest, HttpResponse,
};

struct SessionState {
	addr: Addr<ChatServer>,
}

struct ChatServer {
	clients: HashSet<Addr<MyWebSocket>>,
}

#[derive(Message)]
struct Connect {
	addr: Addr<MyWebSocket>
}

impl Default for ChatServer {
	fn default() -> ChatServer {
		ChatServer {
			clients: HashSet::new(),
		}
	}
}

impl Actor for ChatServer {
	type Context = Context<Self>;
}

fn main() {
	let sys = actix::System::new("websocket-example");
	::std::env::set_var("RUST_LOG", "actix_web=info");
	env_logger::init();
	let addr = "0.0.0.0:8080";
	println!("Starting server at {}", addr);
	let chatserver = Arbiter::start(|_| ChatServer::default());
	server::new(
		move || {
			let state = SessionState {
				addr: chatserver.clone(),
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
		ctx.state().addr.do_send(Connect {
			addr: ctx.address(),
		});
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
struct ChatMessage {
	msg: String,
}

impl Handler<ChatMessage> for MyWebSocket {
	type Result = ();

	fn handle(&mut self, msg: ChatMessage, ctx: &mut Self::Context) {
		ctx.text(msg.msg);
	}
}

/// Handler for `ws::Message`
impl StreamHandler<ws::Message, ws::ProtocolError> for MyWebSocket {
	fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
		// process websocket messages
		let note = format!("WS: {:?}", msg);
		match msg {
			ws::Message::Ping(msg) => {
				self.hb = Instant::now();
				ctx.pong(&msg);
			}
			ws::Message::Pong(_) => {
				self.hb = Instant::now();
			}
			ws::Message::Text(text) => {
				println!("{}", note);
				ctx.state().addr.do_send(ClientMessage {
					msg: text,
				});
			}
			ws::Message::Binary(bin) => ctx.binary(bin),
			ws::Message::Close(_) => {
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

				// stop actor
				ctx.stop();

				// don't try to send a ping
				return;
			}

			ctx.ping("");
		});
	}
}

