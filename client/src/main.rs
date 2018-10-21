#[macro_use]
extern crate yew;
extern crate failure;
extern crate stdweb;
extern crate serde_derive;
extern crate rmp_serde;

extern crate ds;

use failure::Error;

use yew::prelude::*;
use yew::format::{Json, MsgPack};
use yew::services::ConsoleService;
use yew::services::websocket::{WebSocketService, WebSocketStatus, WebSocketTask};

use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};

struct Model {
	console: ConsoleService,
	ws: Option<WebSocketTask>,
	wss: WebSocketService,
	link: ComponentLink<Model>,
	text: String,                    // text in our input box
	server_data: String,             // data received from the server
}

enum Msg {
	Connect,                          // connect to websocket server
	Disconnected,                     // disconnected from server
	Ignore,                           // ignore this message
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

impl Component for Model {
	type Message = Msg;
	type Properties = ();

	fn create(_: Self::Properties, mut link: ComponentLink<Self>) -> Self {
		link.send_self(Msg::Connect);

		Model {
			console: ConsoleService::new(),
			ws: None,
			wss: WebSocketService::new(),
			link: link,
			text: String::new(),
			server_data: String::new(),
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
				}
				true
			}
			Msg::Disconnected => {
				self.ws = None;
				true
			}
			Msg::Ignore => {
				false
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
				self.server_data.push_str(&format!("{:?}\n", &m));
				match m {
					ds::ServerMsg::AvailableSoldiers(s) => {
						if s.len() > 0 {
							self.link.send_self(Msg::SendGameMsg(
									ds::GameMsg::TakeControl(s[0])));
						}
					}
					_ => ()
				}
				true
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
			// connect button
			<p><button onclick=|_| Msg::Connect,>{ "Connect" }</button></p><br/>
			// text showing whether we're connected or not
			<p>{ "Connected: " } { !self.ws.is_none() } </p><br/>
			// input box for sending text
			<p><input type="text", value=&self.text, oninput=|e| Msg::TextInput(e.value),></input></p><br/>
			// button for sending text
			<p><button type="button", onclick=|_| Msg::SendText,>{ "Send" }</button></p><br/>
			// text area for showing data from the server
			<p><textarea rows=8, value=&self.server_data,></textarea></p><br/>
		}
	}
}

fn main() {
	yew::initialize();
	App::<Model>::new().mount_to_body();
	yew::run_loop();
}
