use crate::Video;

/// A video that might need adding to data.json. `existing` is set when the video is already in
/// data.json but its entry is missing the other platform's video.
pub struct Candidate {
	pub video: Video,
	pub existing: Option<usize>,
}

/// A YouTube candidate and a Twitch candidate (as indices into their lists) that are the same stream.
#[derive(Clone, Copy)]
pub struct Pair {
	pub youtube: usize,
	pub twitch: usize,
}

/// Pairs up YouTube and Twitch candidates that were live at the same time, meaning one of them
/// started while the other was still going. When several overlap, the ones that overlap the most
/// go together. Each candidate is used at most once, so two videos on the same platform are always
/// separate streams.
pub fn pair_by_overlap(youtube: &[Candidate], twitch: &[Candidate]) -> Vec<Pair> {
	let mut options = Vec::new();
	for (y, youtube) in youtube.iter().enumerate() {
		for (t, twitch) in twitch.iter().enumerate() {
			// Two videos that are both in data.json are in separate entries on purpose.
			if youtube.existing.is_some() && twitch.existing.is_some() {
				continue;
			}
			if let Some(overlap) = overlap_seconds(&youtube.video, &twitch.video) {
				options.push((Pair { youtube: y, twitch: t }, overlap));
			}
		}
	}
	options.sort_by_key(|&(_, overlap)| std::cmp::Reverse(overlap));

	let mut pairs: Vec<Pair> = Vec::new();
	for (option, _) in options {
		if !pairs
			.iter()
			.any(|pair| pair.youtube == option.youtube || pair.twitch == option.twitch)
		{
			pairs.push(option);
		}
	}
	pairs
}

/// How long two videos were live at the same time, or `None` if one ended before the other started.
fn overlap_seconds(a: &Video, b: &Video) -> Option<i64> {
	let overlap = (a.end.min(b.end) - a.start.max(b.start)).num_seconds();
	(overlap >= 0).then_some(overlap)
}

#[cfg(test)]
mod tests {
	use chrono::{DateTime, Utc};

	use super::*;
	use crate::Platform;

	fn candidate(platform: Platform, start: &str, end: &str, existing: Option<usize>) -> Candidate {
		let time = |time: &str| DateTime::parse_from_rfc3339(time).unwrap().with_timezone(&Utc);
		let video = Video {
			platform,
			id: String::new(),
			title: String::new(),
			start: time(start),
			end: time(end),
			is_live: false,
		};
		Candidate { video, existing }
	}

	fn youtube(start: &str, end: &str) -> Candidate {
		candidate(Platform::YouTube, start, end, None)
	}

	fn twitch(start: &str, end: &str) -> Candidate {
		candidate(Platform::Twitch, start, end, None)
	}

	fn paired(youtube: &[Candidate], twitch: &[Candidate]) -> Vec<(usize, usize)> {
		let mut pairs: Vec<_> = pair_by_overlap(youtube, twitch)
			.iter()
			.map(|pair| (pair.youtube, pair.twitch))
			.collect();
		pairs.sort();
		pairs
	}

	#[test]
	fn pairs_streams_that_started_while_the_other_was_live() {
		let youtube = [youtube("2026-10-04T20:30:00Z", "2026-10-05T00:30:00Z")];
		let twitch = [twitch("2026-10-04T20:00:00Z", "2026-10-05T00:00:00Z")];
		assert_eq!(paired(&youtube, &twitch), [(0, 0)]);
	}

	#[test]
	fn keeps_back_to_back_streams_separate() {
		let youtube = [youtube("2026-10-13T02:00:00Z", "2026-10-13T02:30:00Z")];
		let twitch = [twitch("2026-10-13T02:31:00Z", "2026-10-13T04:00:00Z")];
		assert_eq!(paired(&youtube, &twitch), []);
	}

	#[test]
	fn same_platform_videos_never_share_a_partner() {
		// Two YouTube streams during one Twitch stream: the Twitch one goes with the bigger overlap.
		let youtube = [
			youtube("2026-10-13T02:00:00Z", "2026-10-13T02:20:00Z"),
			youtube("2026-10-13T02:25:00Z", "2026-10-13T04:00:00Z"),
		];
		let twitch = [twitch("2026-10-13T02:00:00Z", "2026-10-13T04:00:00Z")];
		assert_eq!(paired(&youtube, &twitch), [(1, 0)]);
	}

	#[test]
	fn never_pairs_two_videos_already_in_data_json() {
		let youtube = [candidate(
			Platform::YouTube,
			"2026-10-04T20:00:00Z",
			"2026-10-05T00:00:00Z",
			Some(5),
		)];
		let twitch = [candidate(
			Platform::Twitch,
			"2026-10-04T20:00:00Z",
			"2026-10-05T00:00:00Z",
			Some(6),
		)];
		assert_eq!(paired(&youtube, &twitch), []);
	}
}
