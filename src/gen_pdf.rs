// use super::Rail;
use geo::{BoundingRect, LineString};
use osmpbfreader::{Node, NodeId, Way, WayId};
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

	const SCALE: f32 = 60.;
	let page_width = Mm(bounds.width() as f32 * SCALE / 1.4);
	let page_height = Mm(bounds.height() as f32 * SCALE);
	let coord_to_point = |(x, y): (f64, f64)| -> pdf::Point {
		pdf::Point::new(
			Mm((x - bounds.min().x) as f32 * SCALE / 1.4),
			Mm((y - bounds.min().y) as f32 * SCALE),
		)
	};

	let (doc, page_1, layer_1) = PdfDocument::new("Railway Map", page_width, page_height, "Base");
	let current_layer = doc.get_page(page_1).get_layer(layer_1);

	current_layer.set_outline_thickness(0.5);
	for way in ways {
		let rail = Rail::from_way(&way, &nodes);

		current_layer.set_outline_color(match rail.maxspeed {
			MaxSpeed::Single(_, speed) => {
				let relative = speed as f32 / 300.;
				rgb(
					-4. * (relative - 1.).powf(2.) + 1.,
					-4. * (relative - 0.5).powf(2.) + 1.,
					-4. * (relative).powf(2.) + 1.,
				)
			}
			_ => rgb(0.5, 0.5, 0.5),
		});

		current_layer.add_line(pdf::Line {
			points: rail
				.geometry
				.iter()
				.map(|coord| (coord_to_point(*coord), false))
				.collect(),
			is_closed: false,
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
	};
	current_layer.add_line(bounds_line);

	println!("Drew outline");

	doc
}

fn rgb(r: f32, g: f32, b: f32) -> pdf::Color {
	pdf::Color::Rgb(pdf::Rgb::new(r, g, b, None))
}

#[derive(Debug)]
pub(crate) struct Rail {
	id: WayId,
	// TODO: Use &str or SmartString
	name: Option<String>,
	maxspeed: MaxSpeed,
	geometry: Vec<(f64, f64)>,
}

impl Rail {
	fn from_way(way: &Way, nodes: &HashMap<NodeId, Node>) -> Self {
		Self {
			id: way.id,
			name: way.tags.get("name").map(|name| name.as_str().into()),
			maxspeed: parse_maxspeed(
				way.tags
					.get("maxspeed")
					.map(|string| string.as_str())
					.unwrap_or("none"),
			),
			geometry: way
				.nodes
				.iter()
				.map(|nodeid| {
					let node = nodes.get(nodeid).unwrap();
					(node.lon(), node.lat())
				})
				.collect(),
		}
	}
}

fn parse_maxspeed(string: &str) -> MaxSpeed {
	let string = string.trim();

	if string == "" || string == "none" {
		return MaxSpeed::None;
	}

	if let Ok(maxspeed) = string.parse::<u32>() {
		return MaxSpeed::Single(SpeedUnit::KilometersPerHour, maxspeed);
	}

	if string.ends_with("mph") {
		return MaxSpeed::Single(
			SpeedUnit::MilesPerHour,
			string.replace("mph", "").parse().unwrap(),
		);
	}

	panic!("could not parse speed from {string}")

	// let parts: Vec<&str> = value.split([';', ',', '|']).collect();
	// // 	(!parts.is_empty()).then_some(parts)

	// else if let Some(parts) = {
	// 	let parts: Vec<&str> = value.split([';', ',', '|']).collect();
	// 	(!parts.is_empty()).then_some(parts)
	// } {
	// 	MaxSpeed::Multiple(parts.into_iter().map(|part| parse_maxspeed(part)).collect())
	// }
}

#[derive(Debug)]
enum MaxSpeed {
	Single(SpeedUnit, u32),
	Multiple(Vec<MaxSpeed>),
	None,
}

#[derive(Debug)]
enum SpeedUnit {
	MetersPerSecond,
	KilometersPerHour,
	MilesPerHour,
	Knots,
}
