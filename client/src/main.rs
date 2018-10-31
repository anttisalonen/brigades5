#[macro_use]
extern crate yew;
extern crate failure;
extern crate stdweb;
extern crate serde_derive;
extern crate rmp_serde;

extern crate ds;

use std::collections::HashMap;
use stdweb::*;
use stdweb::web::INonElementParentNode;
use stdweb::unstable::TryInto;
use failure::Error;

use yew::prelude::*;
use yew::format::{Json, MsgPack};
use yew::services::ConsoleService;
use yew::services::websocket::{WebSocketService, WebSocketStatus, WebSocketTask};

use ::serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};

struct Model {
	console: ConsoleService,
	ws: Option<WebSocketTask>,
	wss: WebSocketService,
	link: ComponentLink<Model>,
	text: String,                    // text in our input box
	server_data: String,             // data received from the server
	canvas: stdweb::web::html_element::CanvasElement,
	canvas_dimensions: (f64, f64),
	canvas_scale: f64, // pixels to server units (meters)
	ctx: stdweb::web::CanvasRenderingContext2d,
	view_center: ds::Position,
	sid: Option<ds::SoldierID>,
	seen: HashMap<ds::SoldierID, ds::SeenSoldierInfo>,
}

enum Msg {
	Connect,                          // connect to websocket server
	Disconnected,                     // disconnected from server
	TextInput(String),                // text was input in the input box
	SendText,                         // send our text to server
	ReceivedText(Result<String, Error>),             // data received from server
	Received(ds::ServerMsg),          // data received from server
	ReceivedError(String),
	SendGameMsg(ds::GameMsg),
	CanvasClick(ClickEvent),
}

fn text_to_gamemsg(text: &String) -> Option<ds::GameMsg> {
	let spl: Vec<&str> = text.split(" ").collect();
	match spl[0] {
		"/init" => {
			spl.get(1)
				.and_then(|s| s.parse().ok())
				.and_then(|n| Some(ds::GameMsg::Init(n)))
		}
		"/control" => {
			spl.get(1)
				.and_then(|s| s.parse().ok())
				.and_then(|n| Some(ds::GameMsg::TakeControl(ds::SoldierID(n))))
		}
		"/query" => {
			Some(ds::GameMsg::QueryStatus)
		}
		"/move" => {
			let vc = spl.get(1..4);
			match vc {
				Some([s1, s2, s3]) => {
					let n1 = s1.parse().ok();
					let n2 = s2.parse().ok();
					let n3 = s3.parse().ok();
					n1.and_then(|n1| n2
						    .and_then(|n2| n3
							      .and_then(|n3| Some(ds::GameMsg::MoveTo(ds::SoldierID(n1),
							      ds::Position::new(n2, n3))))))
				}
				_ => None
			}

		}
		_ => None
	}
}

struct InputData {
	msg: Msg
}

impl From<yew::format::Binary> for InputData {
	fn from(data: yew::format::Binary) -> InputData {
		match data {
			Ok(d) => {
				let mut de = Deserializer::new(&d[..]);
				let msg = Deserialize::deserialize(&mut de);
				match msg {
					Ok(m)  => { InputData { msg: Msg::Received(m) } }
					Err(e) => { InputData { msg: Msg::ReceivedError(e.to_string()) } }
				}
			}
			Err(d) => {
				InputData { msg: Msg::ReceivedError(d.to_string()) }
			}
		}
	}
}

impl From<yew::format::Text> for InputData {
	fn from(data: yew::format::Text) -> InputData {
		InputData { msg: Msg::ReceivedText(data) }
	}
}

impl Model {
	fn setup_canvas(&mut self) {
		let canv: stdweb::web::html_element::CanvasElement = 
			stdweb::web::document()
			.get_element_by_id("viewport")
			.unwrap()
			.try_into()
			.unwrap();
		let ct: stdweb::web::CanvasRenderingContext2d =
			canv.get_context().unwrap();
		let client_width: u64 = js! {
			let el = document.getElementById("main");
			return el.clientWidth;
		}.try_into().unwrap();
		let client_height: u64 = js! {
			let el = document.getElementById("main");
			return el.clientHeight;
		}.try_into().unwrap();
		canv.set_width(client_width as u32);
		canv.set_height(client_height as u32);
		self.canvas = canv;
		self.ctx = ct;
		self.canvas_dimensions = (client_width as f64, client_height as f64);
		self.canvas_scale = 0.05;
	}

	fn update_canvas(&self) {
		self.ctx.set_fill_style_color("black");
		self.ctx.fill_rect(0.0, 0.0,
				   self.canvas.width().into(),
				   self.canvas.height().into());
		self.ctx.set_fill_style_color("green");
		let canv = self.canvas_dimensions.0.min(
			self.canvas_dimensions.1);
		let width = canv * 0.05;
		let edge_x = self.view_center.x - (self.canvas_dimensions.0 * self.canvas_scale) * 0.5;
		let edge_y = self.view_center.y - (self.canvas_dimensions.1 * self.canvas_scale) * 0.5;
		for (_, info) in &self.seen {
			self.draw_soldier(info, width, edge_x, edge_y);
		}
	}

	fn draw_soldier(&self, info: &ds::SeenSoldierInfo, width: f64, edge_x: f64, edge_y: f64) {
		let pos = info.position;
		let xp = (pos.x - edge_x) / self.canvas_scale;
		let yp = (pos.y - edge_y) / self.canvas_scale;
		let ds::Direction(dir) = info.direction;
		let dirx = dir.cos() * width * 0.5;
		let diry = dir.sin() * width * 0.5;
		self.ctx.begin_path();
		self.ctx.move_to(xp + dirx, yp + diry);
		self.ctx.line_to(xp - diry, yp + dirx);
		self.ctx.line_to(xp + diry, yp - dirx);
		self.ctx.fill(stdweb::web::FillRule::NonZero);
	}

	fn canvas_event(&mut self, ev: ClickEvent) {
		let xo = ev.offset_x();
		let yo = ev.offset_y();
		let xp = (xo - self.canvas_dimensions.0 * 0.5) * self.canvas_scale + self.view_center.x;
		let yp = (yo - self.canvas_dimensions.1 * 0.5) * self.canvas_scale + self.view_center.y;
		self.console.log(&format!("xo: {}, yo: {}", xo, yo));
		self.console.log(&format!("xp: {}, yp: {}", xp, yp));
		if let Some(sid) = self.sid {
			if let Some(ref mut task) = self.ws {
				let msg = ds::GameMsg::MoveTo(sid, ds::Position::new(xp, yp));
				task.send_binary(MsgPack(&msg));
			}
		}
	}
}

impl Component for Model {
	type Message = Msg;
	type Properties = ();

	fn create(_: Self::Properties, mut link: ComponentLink<Self>) -> Self {
		link.send_self(Msg::Connect);
		let canv: stdweb::web::html_element::CanvasElement = 
			stdweb::web::document()
			.create_element("canvas").unwrap().try_into().unwrap();
		let ct = canv.get_context().unwrap();

		Model {
			console: ConsoleService::new(),
			ws: None,
			wss: WebSocketService::new(),
			link: link,
			text: String::new(),
			server_data: String::new(),
			canvas: canv,
			canvas_dimensions: (1.0, 1.0),
			canvas_scale: 1.0,
			ctx: ct,
			view_center: ds::Position::new(0.0, 0.0),
			sid: None,
			seen: HashMap::new(),
		}
	}

	fn update(&mut self, msg: Self::Message) -> ShouldRender {
		match msg {
			Msg::Connect => {
				self.console.log("Connecting");
				let cbout = self.link.send_back(|data: InputData| data.msg);
				let cbnot = self.link.send_back(|input| {
					ConsoleService::new().log(&format!("Notification: {:?}", input));
					match input {
						WebSocketStatus::Closed | WebSocketStatus::Error => {
							Msg::Disconnected
						}
						WebSocketStatus::Opened => {
							Msg::SendGameMsg(ds::GameMsg::QueryStatus)
						}
					}
				});
				if self.ws.is_none() {
					let url = format!("ws://{}/ws/",
							  stdweb::web::document().location().unwrap().host().unwrap());
					let task = self.wss.connect(&url, cbout, cbnot.into());
					self.ws = Some(task);
					self.setup_canvas();
				}
				true
			}
			Msg::Disconnected => {
				self.ws = None;
				true
			}
			Msg::TextInput(e) => {
				self.text = e; // note input box value
				if self.text.len() > 0 {
					if self.text.chars().last().unwrap() == '\n' {
						self.link.send_self(Msg::SendText);
					}
				}
				true
			}
			Msg::SendText => {
				match self.ws {
					Some(ref mut task) => {
						match text_to_gamemsg(&self.text) {
							Some(msg) => {
								task.send_binary(MsgPack(&msg));
							}
							None => {
								task.send(Json(&self.text));
							}
						}
						self.text = "".to_string();
						true // clear input box
					}
					None => {
						false
					}
				}
			}
			Msg::ReceivedText(m) => {
				self.server_data.push_str(&format!("{:?}\n", &m));
				true
			}
			Msg::CanvasClick(e) => {
				self.canvas_event(e);
				false
			}
			Msg::Received(m) => {
				match m {
					ds::ServerMsg::NewGame(_) => {
						self.sid = None;
						self.seen = HashMap::new();
						true
					}
					ds::ServerMsg::AvailableSoldiers(s) => {
						if s.len() > 0 {
							self.link.send_self(Msg::SendGameMsg(
									ds::GameMsg::TakeControl(s[0])));
						}
						true
					}
					ds::ServerMsg::SensorInfo(v) => {
						if let Some(mysid) = self.sid {
							let inf = v.get(&mysid);
							match inf {
								Some(upd) => {
									for (sid, ins) in upd.insense.iter() {
										self.seen.insert(sid.to_owned(), ins.to_owned());
									}
									for out in upd.outsense.iter() {
										self.seen.remove(&out);
									}
								}
								None => ()
							}
							self.update_canvas();
						}
						false
					}
					ds::ServerMsg::YouNowHaveControl(sid, info) => {
						self.sid = Some(sid);
						self.view_center = info.external.position.clone();
						self.seen.insert(sid, info.external);
						false
					}
				}
			}
			Msg::ReceivedError(e) => {
				self.server_data.push_str(&format!("Error when reading data from server: {}\n", e));
				true
			},
			Msg::SendGameMsg(msg) => {
				match self.ws {
					Some(ref mut w) => {
						w.send_binary(MsgPack(&msg));
					}
					None => ()
				}
				true
			}
		}
	}
}

impl Renderable<Model> for Model {
	fn view(&self) -> Html<Self> {
		html! {
			<div id="main",>
			<canvas id="viewport", onclick=|e| Msg::CanvasClick(e),></canvas><br/>
			// text showing whether we're connected or not
			<p>{ "Connected: " } { !self.ws.is_none() } </p><br/>
			// input box for sending text
			<p><input type="text", value=&self.text, oninput=|e| Msg::TextInput(e.value),></input></p><br/>
			// button for sending text
			<p><button type="button", onclick=|_| Msg::SendText,>{ "Send" }</button></p><br/>
			// text area for showing data from the server
			<p><textarea rows=8, value=&self.server_data,></textarea></p><br/>
			</div>
		}
	}
}

fn main() {
	yew::initialize();
	App::<Model>::new().mount_to_body();
	yew::run_loop();
}
