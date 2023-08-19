use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Way, WayId};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;
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

#[derive(argh::FromArgs)]
/// Extract data on railways from OSM data
struct Args {
	#[argh(subcommand)]
	nested: Commands,
}

#[derive(argh::FromArgs)]
#[argh(subcommand)]
enum Commands {
	Filter(FilterCommand),
	Pdf(PdfCommand),
}

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "filter")]
/// Load and filter data from .osm.pbf files
struct FilterCommand {
	#[argh(switch, short = 'f')]
	/// ignore and rebuild the entire cache
	force: bool,
}

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "pdf")]
/// Generate .pdf files based on the filtered data
struct PdfCommand {}

fn main() {
	let args: Args = argh::from_env();

	match args.nested {
		Commands::Filter(FilterCommand { force }) => {
			filter(force, CHUNK_NAMES)
				.iter()
				.for_each(|result| match result {
					Ok(cached_chunk) => println!("{:?}", cached_chunk.0),
					Err(err) => println!("{err}"),
				})
		}
		Commands::Pdf(_) => {
			gen(CHUNK_NAMES);
		}
	};
}

fn filter(force: bool, chunk_names: &[&str]) -> Vec<anyhow::Result<CachedChunk>> {
	chunk_names
		.par_iter()
		.map(|name| {
			let path = PathBuf::from(format!("./cache/{name}.bin"));
			if force || !path.exists() {
				let mut reader = OsmPbfReader::new(File::open(format!("./assets/{name}.osm.pbf"))?);
				let chunk = Chunk::from_pbf(name.to_string(), &mut reader);
				std::fs::write(&path, bincode::serialize(&chunk)?)?;
			}
			Ok(CachedChunk(path))
		})
		.collect()
}

fn gen(chunk_names: &[&str]) {
	filter(false, chunk_names)
		.into_par_iter()
		.map(|cached_chunk| {
			let chunk: Chunk = cached_chunk?.try_into()?;
			generate_pdf(chunk.nodes, chunk.ways).save(&mut BufWriter::new(File::create(
				format!("./{}.pdf", chunk.name),
			)?))?;
			Ok("Saved pdf file")
		})
		.for_each(|result: anyhow::Result<&str>| println!("{result:?}"));
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

struct CachedChunk(PathBuf);

impl TryFrom<CachedChunk> for Chunk {
	type Error = anyhow::Error;

	fn try_from(CachedChunk(path): CachedChunk) -> Result<Self, Self::Error> {
		let bytes = std::fs::read(path)?;
		Ok(bincode::deserialize(&bytes)?)
	}
}
