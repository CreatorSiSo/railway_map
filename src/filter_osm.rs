use crate::{download_file, Config};
use indicatif::MultiProgress;
use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Way};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Region {
	pub name: String,
	pub ways: Vec<Way>,
	pub nodes: HashMap<NodeId, Node>,
}

#[derive(Debug)]
pub struct RegionCache(PathBuf);

impl RegionCache {
	pub fn restore(self) -> anyhow::Result<Region> {
		let bytes = std::fs::read(self.0)?;
		Ok(bincode::deserialize(&bytes)?)
	}
}

pub fn filter(
	multi_progress: MultiProgress,
	region: String,
	force: bool,
	Config {
		server_url, suffix, ..
	}: &Config,
) -> anyhow::Result<RegionCache> {
	let osm_pbf_path = PathBuf::from(format!("./assets/{region}{suffix}.osm.pbf"));

	download_file(
		multi_progress,
		&format!("{server_url}/europe/{region}{suffix}.osm.pbf"),
		&format!("{server_url}/europe/{region}{suffix}.osm.pbf.md5"),
		&osm_pbf_path,
	)?;

	// TODO progressbars for filtering process
	let cache_path = PathBuf::from(format!("./cache/{region}.bin"));
	if force || !cache_path.exists() {
		let mut reader = OsmPbfReader::new(std::fs::File::open(osm_pbf_path)?);

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

		// println!("Successfully read {} ways", ways.len());

		let required_node_ids =
			ways.iter()
				.fold(HashSet::<NodeId>::new(), |mut required_nodes, way| {
					required_nodes.extend(way.nodes.iter());
					required_nodes
				});

		reader.rewind().unwrap();

		// println!("Gathered {} required node ids", required_node_ids.len());

		let nodes: HashMap<NodeId, Node> = reader
			.par_iter()
			.filter_map(|obj| match obj {
				Ok(OsmObj::Node(node)) if required_node_ids.contains(&node.id) => {
					Some((node.id, node))
				}
				_ => None,
			})
			.collect();

		// println!("Successfully read {} nodes", nodes.len());

		let chunk = Region {
			name: region,
			ways,
			nodes,
		};

		std::fs::write(&cache_path, &bincode::serialize(&chunk)?)?;
		// println!("Saved {cache_path:?}");
	}

	Ok(RegionCache(cache_path))
}
