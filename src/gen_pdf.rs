use super::Rail;
use geo::{BoundingRect, LineString};
use osmpbfreader::{Node, NodeId, Way};
use printpdf as pdf;
use printpdf::{Mm, PdfDocument, PdfDocumentReference};
use std::collections::HashMap;

pub fn generate_pdf(nodes: HashMap<NodeId, Node>, ways: Vec<Way>) -> PdfDocumentReference {
	let bounds = nodes
		.iter()
		.map(|(_, node)| (node.lon(), node.lat()))
		.collect::<LineString>()
		.bounding_rect()
		.unwrap();

	println!("Calculated bounding box");

	const SCALE: f64 = 60.;
	let page_width = Mm(bounds.width() * SCALE / 1.4);
	let page_height = Mm(bounds.height() * SCALE);
	let coord_to_point = |(x, y): (f64, f64)| -> pdf::Point {
		pdf::Point::new(
			Mm((x - bounds.min().x) * SCALE / 1.4),
			Mm((y - bounds.min().y) * SCALE),
		)
	};

	let (doc, page_1, layer_1) = PdfDocument::new("Railway Map", page_width, page_height, "Base");
	let current_layer = doc.get_page(page_1).get_layer(layer_1);

	current_layer.set_outline_thickness(0.5);
	for way in ways {
		let rail = Rail::from_way(&way, &nodes);

		current_layer.set_outline_color(match rail.maxspeed {
			Some(maxspeed) => {
				let maxspeed = maxspeed.parse::<u32>().unwrap_or(50) as f64;
				let relative = maxspeed / 300.;
				rgb(
					-4. * (relative - 1.).powf(2.) + 1.,
					-4. * (relative - 0.5).powf(2.) + 1.,
					-4. * (relative).powf(2.) + 1.,
				)
			}
			_ => rgb(0.5, 0.5, 0.5),
		});

		current_layer.add_shape(pdf::Line {
			points: rail
				.geometry
				.iter()
				.map(|coord| (coord_to_point(*coord), false))
				.collect(),
			is_closed: false,
			has_fill: false,
			has_stroke: true,
			is_clipping_path: false,
		});
	}

	println!("Drew map");

	current_layer.set_outline_thickness(2.);
	current_layer.set_outline_color(rgb(1., 0., 0.));
	let bounds_line = pdf::Line {
		points: vec![
			(coord_to_point((bounds.min().x, bounds.min().y)), false),
			(coord_to_point((bounds.max().x, bounds.min().y)), false),
			(coord_to_point((bounds.max().x, bounds.max().y)), false),
			(coord_to_point((bounds.min().x, bounds.max().y)), false),
		],
		is_closed: true,
		has_fill: false,
		has_stroke: true,
		is_clipping_path: false,
	};
	current_layer.add_shape(bounds_line);

	println!("Drew outline");

	doc
}

fn rgb(r: f64, g: f64, b: f64) -> pdf::Color {
	pdf::Color::Rgb(pdf::Rgb::new(r, g, b, None))
}
