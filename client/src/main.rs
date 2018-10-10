#[macro_use]
extern crate yew;
extern crate failure;
extern crate stdweb;

use failure::Error;

use stdweb::*;

use yew::prelude::*;
use yew::format::Json;
use yew::services::ConsoleService;
use yew::services::websocket::{WebSocketService, WebSocketStatus, WebSocketTask};

struct Model {
	console: ConsoleService,
	ws: Option<WebSocketTask>,
	wss: WebSocketService,
	link: ComponentLink<Model>,
	text: String,                    // text in our input box
	server_data: String,             // data received from the server
}

enum Msg {
	Connect,                         // connect to websocket server
	Disconnected,                    // disconnected from server
	Ignore,                          // ignore this message
	TextInput(String),               // text was input in the input box
	SendText,                        // send our text to server
	Received(Result<String, Error>), // data received from server
}

impl Component for Model {
	type Message = Msg;
	type Properties = ();

	fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
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
				let cbout = self.link.send_back(|Json(data)| Msg::Received(data));
				let cbnot = self.link.send_back(|input| {
					ConsoleService::new().log(&format!("Notification: {:?}", input));
					match input {
						WebSocketStatus::Closed | WebSocketStatus::Error => {
							Msg::Disconnected
						}
						_ => Msg::Ignore,
					}
				});
				if self.ws.is_none() {
					let url = js! {
						return "ws://" + location.host + "/ws/";
					}.into_string().unwrap();
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
				true
			}
			Msg::SendText => {
				match self.ws {
					Some(ref mut task) => {
						task.send(Json(&self.text));
						self.text = "".to_string();
						true // clear input box
					}
					None => {
						false
					}
				}
			}
			Msg::Received(Ok(s)) => {
				self.server_data.push_str(&format!("{}\n", &s));
				true
			}
			Msg::Received(Err(s)) => {
				self.server_data.push_str(&format!("Error when reading data from server: {}\n", &s.to_string()));
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
			<p><button onclick=|_| Msg::SendText,>{ "Send" }</button></p><br/>
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
