extern crate actix;
extern crate actix_web;
extern crate env_logger;
extern crate ds;

use std::time::{Duration};
use std::collections::HashSet;

use actix::prelude::*;

use crate::websocket::*;
use crate::game::*;

pub struct ChatServer {
	clients: HashSet<Addr<MyWebSocket>>,
	game: GameState,
}

#[derive(Message)]
struct UpdateMessage;

const UPDATE_INTERVAL: Duration = Duration::from_millis(100);

impl Handler<UpdateMessage> for ChatServer {
	type Result = ();

	fn handle(&mut self, _msg: UpdateMessage, _ctx: &mut Context<Self>) -> Self::Result {
		let msgs = self.game.tick(UPDATE_INTERVAL);
		for (recp, msg) in msgs {
			recp.do_send(ServerMsg { msg: msg });
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

impl Handler<WebSocketMsg> for ChatServer {
	type Result = ();

	fn handle(&mut self, msg: WebSocketMsg, _: &mut Context<Self>) -> Self::Result {
		match msg {
			WebSocketMsg::Connected(addr) => {
				self.clients.insert(addr);
			}
			WebSocketMsg::Disconnected(addr) => {
				println!("client disconnected");
				self.clients.remove(&addr);
				self.game.client_disconnected(addr);
			}
			WebSocketMsg::IncomingData(addr, gmsg) => {
				if self.game.game_msg(&addr, gmsg) {
					self.game = GameState::new();
					let val = ds::ServerMsg::NewGame(self.game.available_soldiers());
					addr.do_send(ServerMsg {
						msg: val
					});
				}
			}
		}
	}
}


