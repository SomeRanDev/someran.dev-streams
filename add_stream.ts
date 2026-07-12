import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs";

// ---

const HEADERS = { headers: { "User-Agent": "Mozilla/5.0" } };

// ---

const existingData = JSON.parse(await Deno.readTextFile("./data.json"));

// ---

const defaultIndex = existingData.at(-1)[0] + 1;
const latestYouTubeStream = await getLatestYoutubeLiveId("SomeRanDev2");
const latestTwitchStream = await getLatestTwitchVodId("SomeRanDev");

let index = parseInt(prompt(`Index (${defaultIndex}): `).trim());
if(index <= 0 || isNaN(index)) {
	index = defaultIndex;
}

let youtubeId = prompt(`YouTube ID (https://www.youtube.com/watch?v=${latestYouTubeStream.videoId}): `).trim();
if(!youtubeId || youtubeId.length <= 0) {
	youtubeId = latestYouTubeStream.videoId;
}

let twitchId = prompt(`Twitch ID (https://www.twitch.tv/videos/${latestTwitchStream}): `).trim();
if(!twitchId || twitchId.length <= 0) {
	twitchId = latestTwitchStream;
}

let title = prompt(`Stream title (${latestYouTubeStream.title}): `).trim();
if(!title || title.length <= 0) {
	title = latestYouTubeStream.title;
}

const now = new Date();
const epochSeconds = Math.floor(
	Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - 1) / 1000
);

// ---

async function getLatestYoutubeLiveId(channelHandle: string): Promise<string | null> {
	const url = `https://www.youtube.com/@${channelHandle}/streams`;
	const response = await fetch(url, HEADERS);
	const html = await response.text();
	const videoIdMatch = html.match(/"videoId":"([^"]+)"/);
	const titleMatch = html.match(/"lockupMetadataViewModel":\{"title":\{"content":"([^"]+)"/);
	return {
		videoId: videoIdMatch?.[1] ?? null,
		title: titleMatch?.[1] ?? null
	};
}

async function getLatestTwitchVodId(channel: string): Promise<string | null> {
	const response = await fetch(`https://www.twitch.tv/${channel}/videos?filter=archives&sort=time`, HEADERS);
	const html = await response.text();
	const ids = Array.from(html.matchAll(/https?:\/\/(?:www\.)?twitch\.tv\/videos\/(\d+)/g)).map((m) => m[1]);
	if(ids.length > 0) { return ids[0]; }
	const relativeIds = Array.from(html.matchAll(/\/videos\/(\d+)/g)).map((m) => m[1]);
	return relativeIds[0] ?? null;
}

// ---

const thisJsFilePath = fileURLToPath(import.meta.url);
const thisJsDirectory = dirname(thisJsFilePath);
async function pushToJson(jsonPath: string, data: any) {
	const fullJsonPath = join(thisJsDirectory, jsonPath);
	const array = JSON.parse(await Deno.readTextFile(fullJsonPath)) as unknown[];
	array.push(data);

	const json = ["["];
	array.forEach((item, i) => {
		const line = `\t${JSON.stringify(item).replace(",", ", ")}`;
		const isLast = i === array.length - 1;
		const comma = isLast ? "" : ",";
		const extraNewline = ((i % 10 !== 0) || isLast) ? "" : "\n";
		json.push(line + comma + extraNewline);
	});
	json.push("]");

	await Deno.writeTextFile(fullJsonPath, json.join("\n"));
}

pushToJson("data.json", [index, youtubeId, twitchId, epochSeconds.toString()]);
pushToJson("names.json", [index, title]);

console.log("Done!");
