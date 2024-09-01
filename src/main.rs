use std::collections::HashSet;

use wgui::{gui::{button, hstack, table, tbody, td, text, text_input, th, thead, tr, vstack, Item}, types::ClientEvent, Wgui};

struct State {

}

impl State {
	pub fn new() -> State {
		State {}
	}
}

fn td2(t: &str) -> Item {
	td(text(t)).text_align("center")
}

fn nodes_table(state: &State) -> Item {
	table([
		thead([
			tr([
				th(text("ID")),
				th(text("NAME")),
				th(text("TRAFFIC")),
				th(text("STATUS")),
			])
		]),
		tbody([
			tr([
				td2("1"),
				td2("Node 1"),
				td2("0.0"),
				td2("Online"),
			]),
			tr([
				td2("2"),
				td2("Node 2"),
				td2("0.0"),
				td2("Online"),
			]),
			tr([
				td2("3"),
				td2("Node 3"),
				td2("0.0"),
				td2("Online"),
			]),
		])
	])
}

fn navigation_bar() -> Item {
	hstack([
		hstack([
			text("Nodes").cursor("pointer"),
			text("Files").cursor("pointer"),
			text("Virtual folders").cursor("pointer")
		]).padding(10)
			.grow(1)
			.spacing(20),
		text("Settings"),
	])
}

struct App {
	wgui: Wgui,
	clients: HashSet<usize>,
	state: State,
}

impl App {
	pub fn new() -> App {
		App {
			wgui: Wgui::new("0.0.0.0:8832".parse().unwrap()),
			clients: HashSet::new(),
			state: State::new(),
		}
	}

	async fn render_ui(&mut self) {
		let item = vstack([
			navigation_bar(),
			nodes_table(&self.state),
		]);

		for client_id in &self.clients {
			self.wgui.render(*client_id, item.clone()).await;
		}
	}

	async fn handle_event(&mut self, event: ClientEvent) {
		match event {
			ClientEvent::Disconnected { id } => { self.clients.remove(&id); },
			ClientEvent::Connected { id } => { self.clients.insert(id); },
			_ => {}
		};

		self.render_ui().await;
	}

	async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.wgui.next() => {
					match event {
						Some(e) => {
							println!("Event: {:?}", e);
							self.handle_event(e).await;
						},
						None => {
							println!("No event");
							break;
						},
					}
				}
			}
		}
	}
}

#[tokio::main]
async fn main() {
	App::new().run().await;
}
