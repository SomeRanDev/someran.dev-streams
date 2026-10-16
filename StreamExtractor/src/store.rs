use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

const DATA_FILE: &str = "data.json";
const NAMES_FILE: &str = "names.json";

/// data.json and names.json, where entry N of each file describes stream #N.
///
/// data.json entries are `null` (lost media), a bare YouTube ID (older streams), or
/// `[index, youtubeId, twitchId, date]` where the date looks like "Feb 12, 2026". names.json
/// entries are `[index, title]`.
pub struct Store {
	dir: PathBuf,
	data: Vec<Value>,
	names: Vec<Value>,
}

impl Store {
	pub fn load(dir: &Path) -> Result<Self> {
		let data = read_array(&dir.join(DATA_FILE))?;
		let names = read_array(&dir.join(NAMES_FILE))?;
		if data.len() != names.len() {
			bail!(
				"{DATA_FILE} has {} entries but {NAMES_FILE} has {}, so they need fixing by hand first",
				data.len(),
				names.len()
			);
		}
		Ok(Self {
			dir: dir.to_owned(),
			data,
			names,
		})
	}

	/// The number the next added stream will get.
	pub fn next_index(&self) -> usize {
		self.data.len()
	}

	pub fn youtube_id(&self, index: usize) -> Option<&str> {
		match &self.data[index] {
			Value::String(id) => Some(id),
			Value::Array(entry) => entry.get(1)?.as_str(),
			_ => None,
		}
	}

	pub fn twitch_id(&self, index: usize) -> Option<&str> {
		self.data[index].as_array()?.get(2)?.as_str()
	}

	pub fn title(&self, index: usize) -> Option<&str> {
		self.names[index].as_array()?.get(1)?.as_str()
	}

	/// Every YouTube ID in data.json, mapped to its stream number.
	pub fn youtube_ids(&self) -> HashMap<String, usize> {
		(0..self.data.len())
			.filter_map(|index| Some((self.youtube_id(index)?.to_owned(), index)))
			.collect()
	}

	/// Every Twitch ID in data.json, mapped to its stream number.
	pub fn twitch_ids(&self) -> HashMap<String, usize> {
		(0..self.data.len())
			.filter_map(|index| Some((self.twitch_id(index)?.to_owned(), index)))
			.collect()
	}

	pub fn add(&mut self, youtube_id: Option<&str>, twitch_id: Option<&str>, date: &str, title: &str) {
		let index = self.data.len();
		self.data.push(json!([index, youtube_id, twitch_id, date]));
		self.names.push(json!([index, title]));
	}

	pub fn set_youtube_id(&mut self, index: usize, id: &str) {
		self.entry_mut(index)[1] = id.into();
	}

	pub fn set_twitch_id(&mut self, index: usize, id: &str) {
		self.entry_mut(index)[2] = id.into();
	}

	pub fn set_date_if_missing(&mut self, index: usize, date: &str) {
		let entry = self.entry_mut(index);
		if entry[3].is_null() {
			entry[3] = date.into();
		}
	}

	pub fn save(&self) -> Result<()> {
		write_array(&self.dir.join(DATA_FILE), &self.data)?;
		write_array(&self.dir.join(NAMES_FILE), &self.names)
	}

	fn entry_mut(&mut self, index: usize) -> &mut Vec<Value> {
		let entry = &mut self.data[index];
		if !entry.is_array() {
			// Older entries are a bare YouTube ID, so upgrade them to the full form.
			let youtube_id = entry.take();
			*entry = json!([index, youtube_id, null, null]);
		}
		let entry = entry.as_array_mut().expect("entry was just made an array");
		entry.resize(4, Value::Null);
		entry
	}
}

fn read_array(path: &Path) -> Result<Vec<Value>> {
	let text = fs::read_to_string(path).with_context(|| {
		format!(
			"couldn't read {} (run this from the folder that has data.json and names.json)",
			path.display()
		)
	})?;
	serde_json::from_str(&text).with_context(|| format!("{} isn't a valid JSON array", path.display()))
}

/// Writes the array the same way add_stream.ts did: one entry per line, with a blank line after
/// every tenth entry.
fn write_array(path: &Path, items: &[Value]) -> Result<()> {
	let mut lines = vec!["[".to_owned()];
	for (i, item) in items.iter().enumerate() {
		let is_last = i == items.len() - 1;
		let mut line = format!("\t{}", item.to_string().replacen(',', ", ", 1));
		if !is_last {
			line.push(',');
			if i % 10 == 0 {
				line.push('\n');
			}
		}
		lines.push(line);
	}
	lines.push("]".to_owned());
	fs::write(path, lines.join("\n")).with_context(|| format!("couldn't write {}", path.display()))
}
