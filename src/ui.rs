use wgui::*;
use crate::types::*;

pub fn td2(t: &str) -> Item {
	td(text(t)).text_align("center")
}

pub fn peers_table(state: &State) -> Item {
	table([
		thead([
			tr([
				th(text("ID")),
				th(text("NAME")),
				th(text("IP")),
			])
		]),
		tbody(
			state.peers.iter().map(|peer| {
				tr([
					td2(&peer.id),
					td2(&peer.name.clone()),
					td2(&peer.addr.clone().unwrap_or_default()),
				])
			})
		)
	])
}

pub fn navigation_bar() -> Item {
	hstack([
		hstack([
			text("Peers").cursor("pointer"),
			text("Nodes").cursor("pointer"),
			text("Files").cursor("pointer"),
			text("Virtual folders").cursor("pointer")
		]).padding(10)
			.grow(1)
			.spacing(20),
		text("Settings"),
	])
}


pub fn render_ui(state: &State) -> Item {
	let item = vstack([
		navigation_bar(),
		peers_table(state),
	]);

	item
}