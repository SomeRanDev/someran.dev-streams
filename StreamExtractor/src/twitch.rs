use anyhow::{Context, Result, bail};
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::{Value, json};
use ureq::Agent;

use crate::{BROWSER_USER_AGENT, Platform, Video};

const GQL_URL: &str = "https://gql.twitch.tv/gql";
/// The public client ID twitch.tv itself uses, which works without logging in.
const CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";
const QUERY: &str = "query($login: String!, $after: Cursor) {
	user(login: $login) {
		videos(first: 100, after: $after, type: ARCHIVE, sort: TIME) {
			edges { cursor node { id title createdAt lengthSeconds status } }
			pageInfo { hasNextPage }
		}
	}
}";
/// Stops paging through past broadcasts after this many pages (100 each) no matter what.
const MAX_PAGES: usize = 10;

/// Lists a channel's past broadcasts, newest first. Keeps loading more pages until one contains a
/// broadcast `is_known` recognizes.
pub fn list_past_broadcasts(agent: &Agent, login: &str, is_known: impl Fn(&str) -> bool) -> Result<Vec<Video>> {
	let mut videos = Vec::new();
	let mut after: Option<String> = None;
	for _ in 0..MAX_PAGES {
		let response: Value = agent
			.post(GQL_URL)
			.header("User-Agent", BROWSER_USER_AGENT)
			.header("Client-ID", CLIENT_ID)
			.send_json(json!({ "query": QUERY, "variables": { "login": login, "after": after } }))
			.and_then(|mut response| response.body_mut().read_json())
			.context("couldn't load Twitch's past broadcasts")?;
		if let Some(errors) = response.get("errors") {
			bail!("Twitch couldn't list past broadcasts: {errors}");
		}
		let user = &response["data"]["user"];
		if user.is_null() {
			bail!("couldn't find the Twitch channel \"{login}\"");
		}

		let edges = user["videos"]["edges"]
			.as_array()
			.context("unexpected response from Twitch")?;
		for node in edges.iter().map(|edge| &edge["node"]) {
			let (Some(id), Some(created_at)) = (node["id"].as_str(), node["createdAt"].as_str()) else {
				continue;
			};
			let start = DateTime::parse_from_rfc3339(created_at)
				.with_context(|| format!("couldn't understand when Twitch video {id} started ({created_at})"))?
				.to_utc();
			let is_live = node["status"] == "RECORDING";
			let end = if is_live {
				Utc::now()
			} else {
				start + TimeDelta::seconds(node["lengthSeconds"].as_i64().unwrap_or_default())
			};
			videos.push(Video {
				platform: Platform::Twitch,
				id: id.to_owned(),
				title: node["title"].as_str().unwrap_or_default().to_owned(),
				start,
				end,
				is_live,
			});
		}

		after = edges.last().and_then(|edge| edge["cursor"].as_str()).map(str::to_owned);
		let has_next_page = user["videos"]["pageInfo"]["hasNextPage"] == true;
		if !has_next_page || after.is_none() || videos.iter().any(|video| is_known(&video.id)) {
			break;
		}
	}
	Ok(videos)
}
