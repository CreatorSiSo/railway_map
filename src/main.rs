use indicatif::{MultiProgress, ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

mod download;
pub(crate) use download::download_file;

mod filter_osm;
use filter_osm::filter;

mod gen_pdf;
use gen_pdf::generate_pdf;

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
pub struct Config {
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
		Commands::Filter(FilterCommand { force, .. }) => cmd_filter(region_names, &config, force),
		Commands::Pdf(_) => cmd_gen(region_names, &config),
	};
}

fn cmd_filter(region_names: &[String], config: &Config, force: bool) {
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
				config,
			)
		})
		.progress_with(overall_progress.clone())
		.for_each(|maybe_cache| {
			maybe_cache.unwrap();
		});
}

fn cmd_gen(region_names: &Vec<String>, config: &Config) {
	region_names
		.par_iter()
		.map(|region_name| {
			let cache = filter(todo!(), region_name.clone(), false, config)?;
			let region = cache.restore()?;
			let bytes = generate_pdf(region.nodes, region.ways).save_to_bytes()?;
			std::fs::write(format!("./{}.pdf", region.name), &bytes)?;
			Ok(())
		})
		.for_each(|maybe_cache: anyhow::Result<()>| println!("{maybe_cache:?}"));
}
