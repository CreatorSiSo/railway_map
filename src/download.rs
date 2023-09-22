use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
pub async fn download_file(
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
