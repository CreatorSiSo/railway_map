use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Way, WayId};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek};
use std::{collections::HashMap, io::BufWriter};

mod gen_pdf;
use gen_pdf::generate_pdf;

#[derive(Debug)]
pub(crate) struct Rail {
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

const CHUNK_NAMES: &[&str] = &["slovenia-latest", "bremen-latest", "sachsen-latest"];

fn main() {
	// generate_cache();
	for name in CHUNK_NAMES {
		let bytes = std::fs::read(format!("./cache/{name}.bin")).unwrap();
		let chunk: Chunk = bincode::deserialize(&bytes).unwrap();
		generate_pdf(chunk.nodes, chunk.ways)
			.save(&mut BufWriter::new(
				File::create(format!("./{name}.pdf")).unwrap(),
			))
			.unwrap();

		println!("Saved pdf file");
	}
}

fn generate_cache() {
	for name in CHUNK_NAMES {
		let mut reader = OsmPbfReader::new(File::open(format!("./assets/{name}.osm.pbf")).unwrap());
		let chunk = Chunk::from_pbf(name.to_string(), &mut reader);

		std::fs::write(
			format!("./cache/{}.bin", chunk.name),
			bincode::serialize(&chunk).unwrap(),
		)
		.unwrap();
	}
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Chunk {
	name: String,
	ways: Vec<Way>,
	nodes: HashMap<NodeId, Node>,
}

impl Chunk {
	fn from_pbf<R: Read + Seek>(name: String, reader: &mut OsmPbfReader<R>) -> Self {
		let ways: Vec<Way> = reader
			.par_iter()
			.filter_map(|maybe_obj| {
				if let Ok(ref obj @ OsmObj::Way(ref way)) = maybe_obj {
					if obj.tags().contains("railway", "rail") {
						return Some(way.clone());
					}
				}
				None
			})
			.collect();

		println!("Successfully read {} ways", ways.len());

		let required_node_ids =
			ways.iter()
				.fold(HashSet::<NodeId>::new(), |mut required_nodes, way| {
					required_nodes.extend(way.nodes.iter());
					required_nodes
				});

		reader.rewind().unwrap();

		println!("Gathered {} required node ids", required_node_ids.len());

		let nodes: HashMap<NodeId, Node> = reader
			.par_iter()
			.filter_map(|obj| match obj {
				Ok(OsmObj::Node(node)) if required_node_ids.contains(&node.id) => {
					Some((node.id, node))
				}
				_ => None,
			})
			.collect();

		println!("Successfully read {} nodes", nodes.len());

		Chunk { name, ways, nodes }
	}
}
