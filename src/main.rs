use indicatif::MultiProgress;
use indicatif::ParallelProgressIterator;
use indicatif::ProgressBar;
use indicatif::ProgressIterator;
use indicatif::ProgressStyle;
use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Way, WayId};
use rayon::prelude::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

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

	#[argh(option, default = "4", short = 't')]
	/// amount of parallel tasks to use
	tasks: usize,
}

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "pdf")]
/// Generate .pdf files based on the filtered data
struct PdfCommand {}

#[derive(Clone, Debug, serde::Deserialize)]
struct Config {
	server_url: String,
	suffix: String,
	regions: HashMap<String, Vec<String>>,
}

fn main() {
	let args: Args = argh::from_env();
	let config: Config =
		toml::from_str(&std::fs::read_to_string("./config.toml").unwrap()).unwrap();
	let region_names = config.regions.get("europe").unwrap();

	match args.nested {
		Commands::Filter(FilterCommand { force, .. }) => {
			let multi_progress = MultiProgress::new();

			let overall_progress = multi_progress
				.add(ProgressBar::new(region_names.len() as u64))
				.with_style(
					ProgressStyle::default_bar()
						.template("\n[{elapsed_precise}] [{wide_bar:.yellow/orange}] {pos}/{len}")
						.unwrap(),
				)
				.with_message("Overall");
			overall_progress.enable_steady_tick(Duration::from_millis(10));

			region_names
				.par_iter()
				.map(|region_name| {
					filter(
						multi_progress.clone(),
						region_name.clone(),
						force.clone(),
						&config,
					)
				})
				.progress_with(overall_progress.clone())
				.for_each(|maybe_cache| {
					maybe_cache.unwrap();
				});
		}
		Commands::Pdf(_) => {
			region_names
				.par_iter()
				.map(|region_name| gen(region_name.clone(), config.clone()))
				.for_each(|maybe_cache| println!("{maybe_cache:?}"));
		}
	};
}

fn filter(
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

#[tokio::main]
async fn download_file(
	multi_progress: MultiProgress,
	url: &str,
	md5_url: &str,
	path: &PathBuf,
) -> anyhow::Result<()> {
	use futures_util::StreamExt;
	use tokio::io::AsyncWriteExt;

	let spinner = multi_progress.add(ProgressBar::new_spinner());
	spinner.set_style(ProgressStyle::with_template("{spinner} {msg}")?);
	spinner.enable_steady_tick(Duration::from_millis(100));

	spinner.set_message("🔍 Checking whether file exists");
	let up_to_date = path.exists() && {
		spinner.set_message("🚚 Fetching MD5 hash file");
		let text = &reqwest::get(md5_url).await?.text().await?;
		let (md5_hash, expected_file_name) = text.split_once(" ").unwrap();

		spinner.set_message("🔍 Comparing MD5 hashes");
		assert_eq!(path.file_name().unwrap(), expected_file_name.trim());
		md5_hash == format!("{:x}", md5::compute(tokio::fs::read(&path).await?))
	};

	if !up_to_date {
		spinner.set_message("🚚 Fetching metadata");
		let res = reqwest::get(url).await?;
		let len = res.content_length().unwrap_or(0);
		let mut stream = res.bytes_stream();
		let mut file = tokio::fs::File::create(&path).await?;

		spinner.set_message(format!("🚚 \"{url}\""));
		spinner.set_style(
			ProgressStyle::with_template(
				"[{elapsed_precise}] {msg:70} [{wide_bar:.cyan/blue}] {bytes:>12}/{total_bytes:12} {eta:>3}",
			)?
			.progress_chars("##-"),
		);
		spinner.set_length(len);
		spinner.disable_steady_tick();

		while let Some(maybe_chunk) = stream.next().await {
			let mut chunk = maybe_chunk?;
			spinner.set_position(spinner.position() + chunk.len() as u64);
			file.write_all_buf(&mut chunk).await?;
		}
	}

	Ok(())
}

fn gen(region_name: String, config: Config) -> anyhow::Result<()> {
	let cache = filter(todo!(), region_name, false, &config)?;
	let region: Region = cache.try_into()?;
	let bytes = generate_pdf(region.nodes, region.ways).save_to_bytes()?;
	std::fs::write(format!("./{}.pdf", region.name), &bytes)?;
	Ok(())
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Region {
	name: String,
	ways: Vec<Way>,
	nodes: HashMap<NodeId, Node>,
}

#[derive(Debug)]
struct RegionCache(PathBuf);

impl TryFrom<RegionCache> for Region {
	type Error = anyhow::Error;

	fn try_from(RegionCache(path): RegionCache) -> Result<Self, Self::Error> {
		let bytes = std::fs::read(path)?;
		Ok(bincode::deserialize(&bytes)?)
	}
}
