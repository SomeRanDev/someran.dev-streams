const DATA = "https://raw.githubusercontent.com/SomeRanDev/someran.dev-streams/refs/heads/main/data.json";
const NAMES = "https://raw.githubusercontent.com/SomeRanDev/someran.dev-streams/refs/heads/main/names.json";

function main() {
	// Get requested stream index.
	let streamIndex = null;
	if(location.search.match(/\?(\-*\d+)/i)) {
		streamIndex = parseInt(RegExp.$1);
		if(isNaN(streamIndex) || typeof streamIndex !== "number") {
			streamIndex = -1;
		}
	}

	if(streamIndex === null) {
		getDataAndNames(listStreams);
	} else if(streamIndex === 0) {
		setDocumentText("Stream #0 has yet to exist, or maybe it has always existed? Come back later. :)");
	} else if(streamIndex < 0) {
		setDocumentText("⚠️⚠️NEGATIVE STREAMS SHALL NOT BE TOLERATED⚠️⚠️");
	} else {
		getData(redirectToIndex.bind(null, streamIndex));
	}
}

function getData(then) {
	fetch(DATA).then(r => r.json()).then(then);
}

function getDataAndNames(then) {
	Promise.all([fetch(DATA), fetch(NAMES)])
		.then(([data, names]) => Promise.all([data.json(), names.json()]))
		.then(then);
}

function listStreams([data, names]) {
	const combinedData = [];
	for(let i = data.length - 1; i >= 0; i--) {
		combinedData.push([i, data[i], names[i]]);
	}

	const html = combinedData.map(function([index, data, name]) {
		if(index === 0) {
			return null;
		}

		let text = "";
		let youtubeId = null;
		let twitchId = null;
		if(data === null) {
			text = "Lost Media";
		} else if(typeof data === "string") {
			youtubeId = data;
			text = name[1];
		} else if(Array.isArray(data)) {
			youtubeId = data[1];
			twitchId = data[2];
			text = name[1];
		}

		let youtubeUrl = null;
		let twitchUrl = null;
		let thumbnailUrl = "UnknownThumbnail.webp";
		if(youtubeId !== null) {
			// https://i3.ytimg.com/vi/${youtubeId}/maxresdefault.jpg
			thumbnailUrl = `https://img.youtube.com/vi/${youtubeId}/maxresdefault.jpg`;
			youtubeUrl = `https://youtube.com/watch?v=${youtubeId}`;
		}
		if(twitchId !== null) {
			twitchUrl = `https://twitch.tv/videos/${twitchId}`;
		}
		return generateStreamEntry(index, text, thumbnailUrl, youtubeUrl, twitchUrl);
	}).filter((o) => o !== null).join("<br>");

	document.body.classList.remove("message");
	document.body.classList.add("stream-list");

	setDocumentText(html);
}

function generateStreamEntry(index, title, thumbnailUrl, youtubeUrl, twitchUrl) {
	let links = [];
	if(youtubeUrl !== null) {
		links.push([youtubeUrl, "YouTube"]);
	}
	if(twitchUrl !== null) {
		links.push([twitchUrl, "Twitch"]);
	}

return `
<div class="card">
    <img
        class="thumb"
        src="${thumbnailUrl}"
        alt=""
    >

    <div class="content">
        <div class="index">#${index}</div>
        <div class="title">
            ${title}
        </div>
        <div class="links">
			${links.map(([url, name]) => `<a href="${url}">${name}</a>`).join("<br>")}
        </div>
    </div>
</div>`;
}

function redirectToIndex(streamIndex, json) {
	const data = json[streamIndex];
	if(data) {
		if(typeof data === "string") {
			window.location.href = `https://youtube.com/watch?v=${data}`;
		} else if(Array.isArray(data)) {
			if(data[1]) {
				window.location.href = `https://youtube.com/watch?v=${data[1]}`;
			} else {
				window.location.href = `https://twitch.tv/videos/${data[2]}`;
			}
		}
	} else {
		if(streamIndex >= 0 && streamIndex < json.length) {
			setDocumentText(`Stream #${streamIndex} is lost media. Sorry 😔`);
		} else {
			setDocumentText(`Stream #${streamIndex} has not happened yet!`);
		}
	}
}

function setDocumentText(text) {
	const content = document.getElementById("content");
	if(content) {
		content.innerHTML = text;
	}
}

main();
