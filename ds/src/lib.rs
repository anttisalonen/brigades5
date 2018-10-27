extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rmp_serde;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash, Deserialize, Serialize)]
pub struct SoldierID(pub u32);

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
	Init(u32),           // start new game
	TakeControl(SoldierID),
	QueryStatus,
	MoveTo(SoldierID, Position),
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerMsg {
	NewGame(Vec<SoldierID>),    // including list of available soldiers
	AvailableSoldiers(Vec<SoldierID>),
	YouNowHaveControl(SoldierID),
	YourPosition(SoldierID, Position),
	SoldierSeen(Vec<(SoldierID, Position)>),
}

