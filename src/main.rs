use geo::{BoundingRect, LineString};
use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Way, WayId};
use printpdf as pdf;
use printpdf::{Mm, PdfDocument};
use std::collections::HashSet;
use std::{collections::HashMap, fs::File, io::BufWriter};

#[derive(Debug)]
struct Rail {
	id: WayId,
	// TODO: Use &str or SmartString
	name: Option<String>,
	maxspeed: Option<String>,
	geometry: Vec<(f64, f64)>,
}

impl Rail {
	fn from_way(way: &Way, nodes: &HashMap<NodeId, Node>) -> Self {
		Self {
			id: way.id,
			name: way.tags.get("name").map(|name| name.as_str().into()),
			maxspeed: match way.tags.get("maxspeed") {
				Some(maxspeed) if maxspeed == "none" => None,
				Some(maxspeed) => Some(maxspeed.as_str().into()),
				None => None,
			},
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

fn main() {
	const OSM_PBF_PATH: &str = "./assets/dach-latest.osm.pbf";

	let mut reader = OsmPbfReader::new(std::fs::File::open(OSM_PBF_PATH).unwrap());

	let ways: Vec<Way> = reader
		.par_iter()
		.filter_map(|maybe_obj| {
			if let Ok(ref obj @ OsmObj::Way(ref way)) = maybe_obj {
				let tags = obj.tags();
				if tags.contains("railway", "rail") && tags.contains("usage", "main") {
					return Some(way.clone());
				}
			}
			None
		})
		.collect();

	println!("Successfully read {} ways", ways.len());

	let node_ids: HashSet<NodeId> = ways.iter().fold(HashSet::new(), |mut node_ids, way| {
		node_ids.extend(way.nodes.iter());
		node_ids
	});

	reader.rewind().unwrap();

	println!("Gathered {} node ids", node_ids.len());

	let nodes: HashMap<NodeId, Node> = reader
		.par_iter()
		.filter_map(|obj| match obj {
			Ok(OsmObj::Node(node)) if node_ids.contains(&node.id) => Some((node.id, node)),
			_ => None,
		})
		.collect();

	println!("Successfully read {} nodes", nodes.len());

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

	doc.save(&mut BufWriter::new(File::create("./rail_map.pdf").unwrap()))
		.unwrap();

	println!("Saved pdf file");
}

fn rgb(r: f64, g: f64, b: f64) -> pdf::Color {
	pdf::Color::Rgb(pdf::Rgb::new(r, g, b, None))
}
