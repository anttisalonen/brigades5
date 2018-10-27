extern crate actix;
extern crate actix_web;
extern crate env_logger;
extern crate ds;

use std::time::{Duration};

use actix::prelude::*;

use crate::websocket::*;

const WALKING_SPEED: ds::Speed = ds::Speed { speed: 1.0 };

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

pub struct GameState {
	soldiers: Vec<Soldier>,
}

impl GameState {
	pub fn new() -> GameState {
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

	pub fn available_soldiers(&self) -> Vec<ds::SoldierID> {
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
		if os {
			for ref s in &self.soldiers {
				if s.id == sid {
					let seen = self.current_percepts(&s);
					from.do_send(ServerMsg {
						msg: ds::ServerMsg::SoldierSeen(seen)
					});
					break;
				}
			}
		}
	}

	fn current_percepts(&self, s: &Soldier) -> Vec<(ds::SoldierID, ds::Position)> {
		vec![(s.id, s.pos)]
	}

	pub fn tick(&mut self, dur: Duration) -> Vec<(Addr<MyWebSocket>, ds::ServerMsg)> {
		let mut msgs: Vec<(Addr<MyWebSocket>, ds::ServerMsg)> = Vec::new();

		for s in self.soldiers.iter_mut() {
			let opt_msg = s.try_move(dur);
			match opt_msg {
				Some(m) => {
					msgs.push(m);
				}
				None    => ()
			}
		}
		msgs
	}

	pub fn client_disconnected(&mut self, addr: Addr<MyWebSocket>) {
		for ref mut s in &mut self.soldiers {
			match &s.controller {
				Some(c) => {
					if c == &addr {
						s.controller = None;
					}
				}
				None => ()
			}
		}
	}

	pub fn game_msg(&mut self, addr: &Addr<MyWebSocket>, gmsg: ds::GameMsg) -> bool {
		match gmsg {
			ds::GameMsg::Init(_v) => {
				true
			}
			ds::GameMsg::TakeControl(sid) => {
				self.handle_take_control(sid, addr);
				false
			}
			ds::GameMsg::QueryStatus => {
				let val = ds::ServerMsg::AvailableSoldiers(self.available_soldiers());
				addr.do_send(ServerMsg { msg: val });
				false
			}
			ds::GameMsg::MoveTo(sid, pos) => {
				for ref mut s in &mut self.soldiers {
					if s.id == sid {
						if let Some(c) = &s.controller {
							if c == addr {
								s.moving = Some(pos);
								break;
							}
						}
					}
				}
				false
			}
		}
	}
}


