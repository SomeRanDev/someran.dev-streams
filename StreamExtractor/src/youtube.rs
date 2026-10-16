use std::thread;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use ureq::Agent;

use crate::BROWSER_USER_AGENT;

const BROWSE_URL: &str = "https://www.youtube.com/youtubei/v1/browse?prettyPrint=false";
/// Stops paging through the "Live" tab after this many pages (30 streams each) no matter what.
const MAX_PAGES: usize = 10;
const CONCURRENT_REQUESTS: usize = 6;

pub struct VideoDetails {
	pub title: String,
	/// When the video was live. `None` if it was uploaded instead of streamed.
	pub live_period: Option<(DateTime<Utc>, DateTime<Utc>)>,
	pub is_upcoming: bool,
	pub is_live: bool,
}

/// Lists the IDs of the videos on a channel's "Live" tab, newest first. Keeps loading more pages
/// until one contains a video `is_known` recognizes.
pub fn list_streams(agent: &Agent, handle: &str, is_known: impl Fn(&str) -> bool) -> Result<Vec<String>> {
	let html = get(agent, &format!("https://www.youtube.com/@{handle}/streams"))?;
	let data = embedded_json(&html, "ytInitialData = ").context("couldn't find the stream list on YouTube's page")?;
	let Some(tab) = data["contents"]["twoColumnBrowseResultsRenderer"]["tabs"]
		.as_array()
		.into_iter()
		.flatten()
		.map(|tab| &tab["tabRenderer"])
		.find(|tab| tab["selected"] == true)
	else {
		bail!("YouTube didn't show the \"Live\" tab for @{handle}");
	};

	let mut page = Page::default();
	page.collect(&tab["content"]);
	let mut videos = page.videos;
	let mut continuation = page.continuation;

	let client_version = client_version(&html);
	for _ in 1..MAX_PAGES {
		let (Some(token), Some(client_version)) = (continuation.take(), client_version) else {
			break;
		};
		if videos.iter().any(|id| is_known(id)) {
			break;
		}
		let body = json!({
			"context": { "client": { "clientName": "WEB", "clientVersion": client_version, "hl": "en", "gl": "US" } },
			"continuation": token,
		});
		let response: Value = agent
			.post(BROWSE_URL)
			.header("User-Agent", BROWSER_USER_AGENT)
			.send_json(body)
			.and_then(|mut response| response.body_mut().read_json())
			.context("couldn't load more of YouTube's stream list")?;
		let mut page = Page::default();
		page.collect(&response["onResponseReceivedActions"]);
		videos.extend(page.videos);
		continuation = page.continuation;
	}
	Ok(videos)
}

/// Looks up the details of several videos (in the same order), a few at a time.
pub fn fetch_details(agent: &Agent, ids: &[&str]) -> Result<Vec<VideoDetails>> {
	let mut details = Vec::with_capacity(ids.len());
	for chunk in ids.chunks(CONCURRENT_REQUESTS) {
		thread::scope(|scope| {
			let handles: Vec<_> = chunk
				.iter()
				.map(|&id| scope.spawn(move || video_details(agent, id)))
				.collect();
			for handle in handles {
				details.push(handle.join().expect("YouTube request thread panicked")?);
			}
			anyhow::Ok(())
		})?;
	}
	Ok(details)
}

fn video_details(agent: &Agent, id: &str) -> Result<VideoDetails> {
	let html = get(agent, &format!("https://www.youtube.com/watch?v={id}"))?;
	let player = embedded_json(&html, "ytInitialPlayerResponse = ")
		.with_context(|| format!("couldn't find the details of YouTube video {id}"))?;
	let details = &player["videoDetails"];
	let microformat = &player["microformat"]["playerMicroformatRenderer"];
	let broadcast = &microformat["liveBroadcastDetails"];

	let Some(title) = details["title"].as_str() else {
		let status = &player["playabilityStatus"];
		bail!(
			"YouTube wouldn't show video {id} ({}: {})",
			status["status"],
			status["reason"]
		);
	};
	let is_live = broadcast["isLiveNow"] == true;
	let live_period = match broadcast["startTimestamp"].as_str() {
		None => None,
		Some(start) => {
			let start =
				parse_time(start).with_context(|| format!("couldn't understand when YouTube video {id} started"))?;
			let end = match broadcast["endTimestamp"].as_str() {
				Some(end) => {
					parse_time(end).with_context(|| format!("couldn't understand when YouTube video {id} ended"))?
				}
				None if is_live => Utc::now(),
				None => {
					let length = details["lengthSeconds"].as_str().and_then(|length| length.parse().ok());
					start + TimeDelta::seconds(length.unwrap_or_default())
				}
			};
			Some((start, end))
		}
	};
	Ok(VideoDetails {
		title: title.to_owned(),
		live_period,
		is_upcoming: details["isUpcoming"] == true,
		is_live,
	})
}

#[derive(Default)]
struct Page {
	videos: Vec<String>,
	continuation: Option<String>,
}

impl Page {
	/// Gathers the videos and the "load more" token from a piece of YouTube's page data.
	fn collect(&mut self, value: &Value) {
		match value {
			Value::Array(items) => items.iter().for_each(|item| self.collect(item)),
			Value::Object(object) => {
				if let Some(lockup) = object.get("lockupViewModel") {
					if lockup["contentType"] == "LOCKUP_CONTENT_TYPE_VIDEO" {
						self.push(&lockup["contentId"]);
					}
				} else if let Some(renderer) = object.get("videoRenderer") {
					// The layout YouTube used before lockups, in case it comes back.
					self.push(&renderer["videoId"]);
				} else if let Some(renderer) = object.get("continuationItemRenderer") {
					let token = &renderer["continuationEndpoint"]["continuationCommand"]["token"];
					self.continuation = token.as_str().map(str::to_owned);
				} else {
					object.values().for_each(|item| self.collect(item));
				}
			}
			_ => {}
		}
	}

	fn push(&mut self, id: &Value) {
		if let Some(id) = id.as_str() {
			self.videos.push(id.to_owned());
		}
	}
}

fn get(agent: &Agent, url: &str) -> Result<String> {
	agent
		.get(url)
		.header("User-Agent", BROWSER_USER_AGENT)
		.header("Accept-Language", "en-US,en;q=0.9")
		// Skips the cookie consent page YouTube shows in some countries.
		.header("Cookie", "SOCS=CAI")
		.call()
		.and_then(|mut response| response.body_mut().read_to_string())
		.with_context(|| format!("couldn't load {url}"))
}

fn parse_time(time: &str) -> Result<DateTime<Utc>> {
	Ok(DateTime::parse_from_rfc3339(time)
		.with_context(|| format!("\"{time}\" isn't a valid time"))?
		.to_utc())
}

/// Parses the JSON YouTube assigns to a variable in its page, as in `var ytInitialData = {...};`.
fn embedded_json(html: &str, marker: &str) -> Option<Value> {
	let start = html.find(marker)? + marker.len();
	Value::deserialize(&mut serde_json::Deserializer::from_str(&html[start..])).ok()
}

fn client_version(html: &str) -> Option<&str> {
	let marker = "\"INNERTUBE_CLIENT_VERSION\":\"";
	let start = html.find(marker)? + marker.len();
	let length = html[start..].find('"')?;
	Some(&html[start..start + length])
}
