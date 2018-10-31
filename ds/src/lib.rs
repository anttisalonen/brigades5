extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rmp_serde;

use std::collections::HashMap;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash, Deserialize, Serialize)]
pub struct SoldierID(pub i32);

#[derive(PartialEq, Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Position {
	pub x: f64,
	pub y: f64
}

#[derive(PartialEq)]
pub struct Speed {
	pub speed: f64
}

impl Position {
	pub fn new(a: f64, b: f64) -> Position {
		Position {
			x: a,
			y: b,
		}
	}

	pub fn dist(&self, pos: &Position) -> f64 {
		((self.x - pos.x) * (self.x - pos.x) +
		 (self.y - pos.y) * (self.y - pos.y)).sqrt()
	}

	pub fn length(&self) -> f64 {
		((self.x * self.x) + (self.y * self.y)).sqrt()
	}

	pub fn normalized(&self) -> Position {
		let len = self.length();
		Position {
			x: self.x / len,
			y: self.y / len
		}
	}

	pub fn to_pos(&self, pos: &Position) -> Position {
		Position {
			x: pos.x - self.x,
			y: pos.y - self.y
		}
	}

	pub fn add(&mut self, towards: Position, speed: Speed, dur: std::time::Duration) {
		let d = dur.as_secs() as f64 + dur.subsec_nanos() as f64 * 1e-9;
		self.x += towards.x * speed.speed * d;
		self.y += towards.y * speed.speed * d;
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub enum GameMsg {
	Init(i32),           // start new game
	TakeControl(SoldierID),
	QueryStatus,
	MoveTo(SoldierID, Position),
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub enum Side {
	Red,
	Blue,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InternalSoldierInfo {
	pub health: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FullSoldierInfo {
	pub internal: InternalSoldierInfo,
	pub external: SeenSoldierInfo,
}

#[derive(PartialEq, Debug, Copy, Clone, Deserialize, Serialize)]
pub struct Direction(pub f64);

pub const MAX_NUM_SOLDIERS: i32 = 64;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeenSoldierInfo {
	pub alive: bool,
	pub position: Position,
	pub direction: Direction,
	pub side: Side,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SensorUpdate {
	pub insense: Vec<(SoldierID, SeenSoldierInfo)>,
	pub outsense: Vec<SoldierID>,
}

impl SensorUpdate {
	pub fn new() -> SensorUpdate {
		SensorUpdate {
			insense: Vec::new(),
			outsense: Vec::new(),
		}
	}

	pub fn add(&mut self, sid: SoldierID, info: SeenSoldierInfo) {
		self.insense.push((sid, info))
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMsg {
	NewGame(Vec<SoldierID>),    // including list of available soldiers
	AvailableSoldiers(Vec<SoldierID>),
	YouNowHaveControl(SoldierID, FullSoldierInfo),
	SensorInfo(HashMap<SoldierID, SensorUpdate>),
}

