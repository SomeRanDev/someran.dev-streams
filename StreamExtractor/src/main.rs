//! Adds every stream from the YouTube and Twitch channels that's missing from data.json and
//! names.json.
//!
//! A YouTube stream and a Twitch broadcast count as the same stream when one of them started while
//! the other was still live (see `matching`). Streams on the same platform are always separate.

mod matching;
mod store;
mod twitch;
mod youtube;

use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use ureq::Agent;

use matching::Candidate;
use store::Store;

const YOUTUBE_HANDLE: &str = "SomeRanDev2";
const TWITCH_LOGIN: &str = "somerandev";
pub const BROWSER_USER_AGENT: &str =
	"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Platform {
	YouTube,
	Twitch,
}

impl Platform {
	fn name(self) -> &'static str {
		match self {
			Platform::YouTube => "YouTube",
			Platform::Twitch => "Twitch",
		}
	}
}

#[derive(Clone)]
pub struct Video {
	pub platform: Platform,
	pub id: String,
	pub title: String,
	pub start: DateTime<Utc>,
	pub end: DateTime<Utc>,
	pub is_live: bool,
}

impl Video {
	fn url(&self) -> String {
		match self.platform {
			Platform::YouTube => format!("https://www.youtube.com/watch?v={}", self.id),
			Platform::Twitch => format!("https://www.twitch.tv/videos/{}", self.id),
		}
	}
}

/// A stream that isn't in data.json yet, on one or both platforms.
struct NewStream {
	youtube: Option<Video>,
	twitch: Option<Video>,
}

impl NewStream {
	fn videos(&self) -> impl Iterator<Item = &Video> {
		self.youtube.iter().chain(&self.twitch)
	}

	fn start(&self) -> DateTime<Utc> {
		self.videos()
			.map(|video| video.start)
			.min()
			.expect("a new stream has at least one video")
	}

	/// Prefers the Twitch title, since that's what names.json has been using.
	fn title(&self) -> &str {
		&self
			.twitch
			.as_ref()
			.or(self.youtube.as_ref())
			.expect("a new stream has at least one video")
			.title
	}
}

/// A video for a stream that's already in data.json without this platform's video.
struct MissingVideo {
	index: usize,
	video: Video,
}

fn main() -> ExitCode {
	let mut dry_run = false;
	for arg in env::args().skip(1) {
		match arg.as_str() {
			"--dry-run" => dry_run = true,
			_ => {
				eprintln!("Usage: stream-extractor [--dry-run]");
				eprintln!("Run it from the folder that has data.json and names.json.");
				eprintln!("--dry-run shows what would be added without saving anything.");
				return ExitCode::from(2);
			}
		}
	}

	match run(dry_run) {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("Error: {error:#}");
			ExitCode::FAILURE
		}
	}
}

fn run(dry_run: bool) -> Result<()> {
	let mut store = Store::load(Path::new("."))?;
	let known_youtube = store.youtube_ids();
	let known_twitch = store.twitch_ids();
	let agent: Agent = Agent::config_builder()
		.timeout_global(Some(Duration::from_secs(30)))
		.build()
		.into();

	println!("Loading YouTube streams from @{YOUTUBE_HANDLE}...");
	let youtube_ids = youtube::list_streams(&agent, YOUTUBE_HANDLE, |id| known_youtube.contains_key(id))?;
	println!("Loading Twitch past broadcasts from {TWITCH_LOGIN}...");
	let twitch_videos = twitch::list_past_broadcasts(&agent, TWITCH_LOGIN, |id| known_twitch.contains_key(id))?;

	// Videos already in data.json only matter if their entry is missing the other platform's video,
	// except the newest one, which shows when the newest stream in data.json happened.
	let newest_known_youtube = youtube_ids.iter().find(|&id| known_youtube.contains_key(id));
	let youtube_ids: Vec<&str> = youtube_ids
		.iter()
		.filter(|&id| match known_youtube.get(id) {
			None => true,
			Some(&index) => store.twitch_id(index).is_none() || Some(id) == newest_known_youtube,
		})
		.map(String::as_str)
		.collect();
	println!("Loading details for {} YouTube videos...", youtube_ids.len());
	let youtube_details = youtube::fetch_details(&agent, &youtube_ids)?;

	let mut newest_known_start: Option<DateTime<Utc>> = None;
	let mut youtube = Vec::new();
	for (id, details) in youtube_ids.into_iter().zip(youtube_details) {
		if details.is_upcoming {
			continue;
		}
		let Some((start, end)) = details.live_period else {
			println!("Ignoring https://www.youtube.com/watch?v={id} because YouTube doesn't say when it was live.");
			continue;
		};
		let existing = known_youtube.get(id).copied();
		if existing.is_some() {
			newest_known_start = newest_known_start.max(Some(start));
		}
		if existing.is_none_or(|index| store.twitch_id(index).is_none()) {
			let video = Video {
				platform: Platform::YouTube,
				id: id.to_owned(),
				title: details.title,
				start,
				end,
				is_live: details.is_live,
			};
			youtube.push(Candidate { video, existing });
		}
	}
	let mut twitch = Vec::new();
	for video in twitch_videos {
		let existing = known_twitch.get(&video.id).copied();
		if existing.is_some() {
			newest_known_start = newest_known_start.max(Some(video.start));
		}
		if existing.is_none_or(|index| store.youtube_id(index).is_none()) {
			twitch.push(Candidate { video, existing });
		}
	}

	let pairs = matching::pair_by_overlap(&youtube, &twitch);
	let mut new_streams = Vec::new();
	let mut missing_videos = Vec::new();
	for pair in &pairs {
		let (youtube, twitch) = (&youtube[pair.youtube], &twitch[pair.twitch]);
		match (youtube.existing, twitch.existing) {
			(None, None) => new_streams.push(NewStream {
				youtube: Some(youtube.video.clone()),
				twitch: Some(twitch.video.clone()),
			}),
			(Some(index), None) => missing_videos.push(MissingVideo {
				index,
				video: twitch.video.clone(),
			}),
			(None, Some(index)) => missing_videos.push(MissingVideo {
				index,
				video: youtube.video.clone(),
			}),
			(Some(_), Some(_)) => unreachable!("two videos that are both in data.json never get paired"),
		}
	}
	for (i, candidate) in youtube.iter().enumerate() {
		if candidate.existing.is_none() && !pairs.iter().any(|pair| pair.youtube == i) {
			new_streams.push(NewStream {
				youtube: Some(candidate.video.clone()),
				twitch: None,
			});
		}
	}
	for (i, candidate) in twitch.iter().enumerate() {
		if candidate.existing.is_none() && !pairs.iter().any(|pair| pair.twitch == i) {
			new_streams.push(NewStream {
				youtube: None,
				twitch: Some(candidate.video.clone()),
			});
		}
	}

	// Streams are numbered in the order they happened, so anything older than the newest stream in
	// data.json can't be added to the end.
	new_streams.sort_by_key(NewStream::start);
	let (mut new_streams, too_old): (Vec<_>, Vec<_>) = new_streams
		.into_iter()
		.partition(|stream| newest_known_start.is_none_or(|newest| stream.start() > newest));
	missing_videos.sort_by_key(|missing| missing.index);

	if !too_old.is_empty() {
		println!("\nLeaving these out because they're older than the newest stream in data.json:");
		for video in too_old.iter().flat_map(NewStream::videos) {
			println!(
				"  {}  {}  \"{}\" ({})",
				video.platform.name(),
				video.url(),
				video.title,
				stream_date(video.start)
			);
		}
	}
	if !missing_videos.is_empty() {
		println!("\nVideos for streams already in data.json:");
		for missing in &missing_videos {
			println!();
			println!(
				"  #{}  {}",
				missing.index,
				store.title(missing.index).unwrap_or("(untitled)")
			);
			let name = format!("{}:", missing.video.platform.name());
			println!("        + {name:<8} {}", missing.video.url());
		}
	}
	if !new_streams.is_empty() {
		print_new_streams(store.next_index(), &new_streams);
	}

	if new_streams.is_empty() && missing_videos.is_empty() {
		println!("\ndata.json is already up to date!");
		return Ok(());
	}
	if dry_run {
		println!("\nDry run, so nothing was saved.");
		return Ok(());
	}

	while !new_streams.is_empty() {
		println!();
		let answer =
			ask("Type the numbers of any streams to leave out (like \"139 140\"), or press Enter to keep them all: ")?;
		if answer.is_empty() {
			break;
		}
		match parse_stream_numbers(&answer, store.next_index(), new_streams.len()) {
			Ok(offsets) => {
				let mut offset = 0;
				new_streams.retain(|_| {
					offset += 1;
					!offsets.contains(&(offset - 1))
				});
				if !new_streams.is_empty() {
					print_new_streams(store.next_index(), &new_streams);
				}
			}
			Err(message) => println!("{message}"),
		}
	}
	if new_streams.is_empty() && missing_videos.is_empty() {
		println!("Nothing left to save.");
		return Ok(());
	}
	println!();
	if !ask_yes_no("Save to data.json and names.json?")? {
		println!("Nothing was saved.");
		return Ok(());
	}

	for missing in &missing_videos {
		match missing.video.platform {
			Platform::YouTube => store.set_youtube_id(missing.index, &missing.video.id),
			Platform::Twitch => store.set_twitch_id(missing.index, &missing.video.id),
		}
		store.set_date_if_missing(missing.index, &stream_date(missing.video.start));
	}
	for stream in &new_streams {
		let youtube_id = stream.youtube.as_ref().map(|video| video.id.as_str());
		let twitch_id = stream.twitch.as_ref().map(|video| video.id.as_str());
		store.add(youtube_id, twitch_id, &stream_date(stream.start()), stream.title());
	}
	store.save()?;
	println!("Done!");
	Ok(())
}

fn print_new_streams(first_index: usize, new_streams: &[NewStream]) {
	println!("\nNew streams:");
	for (offset, stream) in new_streams.iter().enumerate() {
		let sources = match (&stream.youtube, &stream.twitch) {
			(Some(_), Some(_)) => "YouTube + Twitch",
			(Some(_), None) => "YouTube only",
			_ => "Twitch only",
		};
		let live = if stream.videos().any(|video| video.is_live) {
			" · LIVE NOW"
		} else {
			""
		};
		println!();
		println!(
			"  #{}  {} · {sources}{live}",
			first_index + offset,
			stream_date(stream.start())
		);
		println!("        {}", stream.title());
		for video in stream.videos() {
			let name = format!("{}:", video.platform.name());
			println!("        {name:<8} {}", video.url());
			if video.title != stream.title() {
				println!("                 (titled \"{}\" there)", video.title);
			}
		}
	}
}

/// Turns stream numbers typed by the user into offsets into the new streams list.
fn parse_stream_numbers(answer: &str, first_index: usize, count: usize) -> Result<Vec<usize>, String> {
	answer
		.split(|c: char| c.is_whitespace() || c == ',')
		.filter(|part| !part.is_empty())
		.map(|part| {
			let number: usize = part
				.trim_start_matches('#')
				.parse()
				.map_err(|_| format!("\"{part}\" isn't a number."))?;
			if number < first_index || number >= first_index + count {
				return Err(format!("#{number} isn't one of the new streams."));
			}
			Ok(number - first_index)
		})
		.collect()
}

fn ask(question: &str) -> Result<String> {
	print!("{question}");
	io::stdout().flush()?;
	let mut answer = String::new();
	if io::stdin().read_line(&mut answer)? == 0 {
		// No more input, so finish the line and go with the default answer.
		println!();
	}
	Ok(answer.trim().to_owned())
}

fn ask_yes_no(question: &str) -> Result<bool> {
	loop {
		match ask(&format!("{question} [Y/n] "))?.to_lowercase().as_str() {
			"" | "y" | "yes" => return Ok(true),
			"n" | "no" => return Ok(false),
			_ => println!("Please type y or n."),
		}
	}
}

/// The day a stream happened in local time, written like "Feb 12, 2026". This is what goes in
/// data.json too.
fn stream_date(time: DateTime<Utc>) -> String {
	time.with_timezone(&Local).format("%b %-d, %Y").to_string()
}
