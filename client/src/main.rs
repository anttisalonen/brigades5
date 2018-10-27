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
	ctx: stdweb::web::CanvasRenderingContext2d,
	seen: HashMap<ds::SoldierID, ds::Position>,
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
		for (_, pos) in &self.seen {
			let xp = pos.x / 100.0 * canv * 0.5 + canv * 0.5;
			let yp = pos.y / 100.0 * canv * 0.5 + canv * 0.5;
			self.ctx.fill_rect(xp, yp, width, width);
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
			ctx: ct,
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
			Msg::Received(m) => {
				match m {
					ds::ServerMsg::AvailableSoldiers(s) => {
						if s.len() > 0 {
							self.link.send_self(Msg::SendGameMsg(
									ds::GameMsg::TakeControl(s[0])));
						}
						true
					}
					ds::ServerMsg::SoldierSeen(v) => {
						for (sid, pos) in v {
							self.seen.insert(sid, pos);
						}
						self.update_canvas();
						false
					}
					ds::ServerMsg::YourPosition(sid, pos) => {
						self.seen.insert(sid, pos);
						self.update_canvas();
						false
					}
					_ => {
						self.server_data.push_str(&format!("{:?}\n", &m));
						true
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
			<canvas id="viewport",></canvas><br/>
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
