import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

let index = parseInt(prompt("Index: ").trim());
if(index <= 0 || isNaN(index)) {
	Deno.exit(1);
}

let youtubeId = prompt("YouTube ID: ").trim();
if(!youtubeId || youtubeId.length <= 0) {
	Deno.exit(1);
}

let twitchId = prompt("Twitch ID: ").trim();
if(!twitchId || twitchId.length <= 0) {
	Deno.exit(1);
}

let title = prompt("Stream title: ").trim();
if(!title || title.length <= 0) {
	Deno.exit(1);
}

const now = new Date();
const epochSeconds = Math.floor(
	Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - 1) / 1000
);


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
