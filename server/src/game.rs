extern crate actix;
extern crate actix_web;
extern crate env_logger;
extern crate rand;
extern crate ds;

use std::collections::HashMap;
use std::time::{Duration};

use actix::prelude::*;

use crate::websocket::*;

use ds::SoldierID;

const WALKING_SPEED: ds::Speed = ds::Speed { speed: 1.0 };

// table describing, for each soldier, which soldiers he/she sees
struct DetectionTable {
	detected: Vec<Vec<ds::SoldierID>>,
	curr_index: Vec<i32>,
}

impl DetectionTable {
	fn new() -> DetectionTable {
		DetectionTable {
			detected: vec![[ds::SoldierID(-1); ds::MAX_NUM_SOLDIERS as usize].to_vec(); 4 as usize],
			curr_index: vec![0; ds::MAX_NUM_SOLDIERS as usize],
		}
	}

	fn add(&mut self, seen: ds::SoldierID, by: ds::SoldierID) {
		let ds::SoldierID(s_index) = seen;
		let cui = self.curr_index[s_index as usize];
		if self.detected.len() <= cui as usize {
			self.detected.push(vec![ds::SoldierID(-1); ds::MAX_NUM_SOLDIERS as usize].to_vec());
		}
		self.detected[cui as usize][s_index as usize] = by;
		self.curr_index[s_index as usize] += 1;
	}
}

#[derive(PartialEq, Copy, Clone, Debug)]
struct Soldier {
	id: ds::SoldierID,
	pos: ds::Position,
	dir: ds::Direction,
	alive: bool,
	moving: Option<ds::Position>,
}

impl Soldier {
	fn new() -> Soldier {
		Soldier {
			id: SoldierID(0),
			pos: ds::Position { x: 0.0, y: 0.0 },
			dir: ds::Direction(0.0),
			alive: false,
			moving: None,
		}
	}

	fn try_move(&mut self, time: Duration) -> bool {
		match self.moving {
			Some(pos) => {
				self.move_soldier(time, pos);
				true
			}
			None => {
				false
			}
		}
	}

	fn move_soldier(&mut self, time: Duration, tgtpos: ds::Position) -> ds::SeenSoldierInfo {
		if self.pos.dist(&tgtpos) < 1.0 {
			self.moving = None;
		} else {
			let diff = self.pos.to_pos(&tgtpos).normalized();
			self.pos.add(diff, WALKING_SPEED, time);
			self.dir = ds::Direction(diff.y.atan2(diff.x));
		}
		self.construct_sensor_info()
	}

	fn construct_sensor_info(&self) -> ds::SeenSoldierInfo {
		ds::SeenSoldierInfo {
			alive: true,
			position: self.pos,
			direction: self.dir,
			side: ds::Side::Blue
		}
	}

	fn get_full_info(&self) -> ds::FullSoldierInfo {
		ds::FullSoldierInfo {
			internal: ds::InternalSoldierInfo {
				health: 100,
			},
			external: self.construct_sensor_info()
		}
	}
}

pub struct GameState {
	soldiers: Vec<Soldier>,
	soldier_controllers: Vec<Option<Addr<MyWebSocket>>>,
}

impl GameState {
	pub fn new() -> GameState {
		let mut s = vec![Soldier::new(); ds::MAX_NUM_SOLDIERS as usize];
		let mut controllers = vec![];
		for i in 0..ds::MAX_NUM_SOLDIERS {
			s[i as usize].id = SoldierID(i);
			controllers.push(None);
		}
		for i in 0..4 {
			let x: f64 = 10.0 * i as f64 + 30.0;
			let y: f64 = 10.0 * i as f64 + 30.0;
			s[i as usize].alive = true;
			s[i as usize].pos = ds::Position::new(x, y);
		}
		GameState {
			soldiers: s,
			soldier_controllers: controllers,
		}
	}

	fn is_available(&self, sid: ds::SoldierID) -> bool {
		let ds::SoldierID(i) = sid;
		self.soldiers[i as usize].alive && self.soldier_controllers[i as usize].is_none()
	}

	pub fn available_soldiers(&self) -> Vec<ds::SoldierID> {
		self.soldiers.iter()
			.filter(|p| self.is_available(p.id))
			.map(|p| p.id)
			.collect()
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
		if self.is_available(sid) {
			let SoldierID(i) = sid;
			self.soldier_controllers[i as usize] = Some(from.to_owned());
			from.do_send(ServerMsg {
				msg: ds::ServerMsg::YouNowHaveControl(sid, self.soldiers[i as usize].get_full_info())
			});
			true
		} else {
			false
		}
	}

	fn handle_take_control(&mut self, sid: ds::SoldierID, from: &Addr<MyWebSocket>) {
		let _os = self.try_take_control(sid, &from);
		/*
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
		*/
	}

	fn current_percepts(&self, s: &Soldier) -> Vec<(ds::SoldierID, ds::Position)> {
		vec![(s.id, s.pos)]
	}

	fn move_all(&mut self, dur: Duration) -> Vec<ds::SoldierID> {
		let mut ret = vec![];
		for s in self.soldiers.iter_mut() {
			s.try_move(dur);
			ret.push(s.id);
		}
		ret
	}

	fn find_updates(&self, updated: Vec<ds::SoldierID>) -> DetectionTable {
		let mut dt: DetectionTable = DetectionTable::new();
		for sid in updated.into_iter() {
			let recps = self.detected_by(&sid);
			for recp in recps.into_iter() {
				dt.add(sid, recp);
			}
		}
		dt
	}

	fn construct_messages(&self, det_table: &DetectionTable) -> HashMap<Addr<MyWebSocket>, ds::ServerMsg> {
		let mut msgs: HashMap<Addr<MyWebSocket>, ds::ServerMsg> = HashMap::new();

		for j in 0..det_table.detected.len() {
			for i in 0..ds::MAX_NUM_SOLDIERS {
				if det_table.curr_index[i as usize] <= j as i32 {
					break;
				}
				let SoldierID(det) = det_table.detected[j as usize][i as usize];
				assert!(det != -1);
				let cont = self.soldier_controllers[i as usize].to_owned();
				match cont {
					Some(c) => {
						add_to_servermsg(&mut msgs, c,
								 SoldierID(i), 
								 SoldierID(det),
								 self.soldiers[det as usize].construct_sensor_info());
					}
					None => ()
				}
			}
		}
		msgs
	}

	pub fn tick(&mut self, dur: Duration) -> HashMap<Addr<MyWebSocket>, ds::ServerMsg> {
		let updated = self.move_all(dur);
		let det_table = self.find_updates(updated);
		self.construct_messages(&det_table)
	}

	fn detected_by(&self, _sold: &ds::SoldierID) -> Vec<SoldierID> {
		(0..ds::MAX_NUM_SOLDIERS).map(|i| ds::SoldierID(i)).collect()
	}

	pub fn client_disconnected(&mut self, addr: Addr<MyWebSocket>) {
		for i in 0..ds::MAX_NUM_SOLDIERS {
			match &self.soldier_controllers[i as usize] {
				Some(c) => {
					if c == &addr {
						self.soldier_controllers[i as usize] = None;
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
				let SoldierID(i) = sid;
				let cont = &self.soldier_controllers[i as usize];
				match cont {
					Some(c) => {
						if c == addr {
							self.soldiers[i as usize].moving = Some(pos);
						}
					}
					None => ()
				}
				false
			}
		}
	}
}

fn add_to_servermsg(map: &mut HashMap<Addr<MyWebSocket>, ds::ServerMsg>,
		    recp: Addr<MyWebSocket>, seer: ds::SoldierID,
		    seen: ds::SoldierID,
		    info: ds::SeenSoldierInfo) {
	match map.get_mut(&recp) {
		Some(ds::ServerMsg::SensorInfo(upd)) => {
			upd.entry(seer).or_insert(ds::SensorUpdate::new()).add(seen, info);
		}
		Some(_) => {
			assert!(false)
		}
		None    => {
			let mut su = ds::SensorUpdate::new();
			su.add(seen, info);
			let mut hm = HashMap::new();
			hm.insert(seer, su);
			map.insert(recp, ds::ServerMsg::SensorInfo(hm));
		}
	}
}


