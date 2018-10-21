extern crate actix;
extern crate actix_web;
extern crate env_logger;
extern crate ds;
extern crate rmp_serde;
extern crate serde;

use actix::prelude::*;
use actix_web::{
	fs, http, middleware, server, App,
};

mod websocket;
mod osm;
mod game;
mod serv;

use crate::websocket::*;
use crate::serv::*;

fn main() {
	let sys = actix::System::new("websocket-example");
	osm::run_osm();
	::std::env::set_var("RUST_LOG", "actix_web=info");
	env_logger::init();
	let addr = "0.0.0.0:8080";
	println!("Starting server at {}", addr);
	let chatserver = Arbiter::start(|_| ChatServer::default());
	server::new(
		move || {
			let state = chatserver.clone().recipient();

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


