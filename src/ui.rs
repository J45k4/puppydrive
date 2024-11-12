use wgui::*;
use crate::types::*;

pub fn td2(t: &str) -> Item {
	td(text(t)).text_align("center")
}

pub fn nodes_table(state: &State) -> Item {
	table([
		thead([
			tr([
				th(text("ID")),
				th(text("NAME")),
				th(text("TRAFFIC")),
				th(text("STATUS")),
			])
		]),
		tbody(
			state.nodes.iter().map(|node| {
				tr([
					td2(&node.id.to_string()),
					td2(&node.name),
					td2(&node.traffic.to_string()),
					td2(&node.status.to_string()),
				])
			})
		)
	])
}

pub fn peers_table(state: &State) -> Item {
	table([
		thead([
			tr([
				th(text("NAME")),
				th(text("IP")),
			])
		]),
		tbody(
			state.peers.iter().map(|peer| {
				tr([
					td2(&peer.name),
					td2(&peer.ip),
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