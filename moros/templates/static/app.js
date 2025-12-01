const parser = new DOMParser();
const tickDelay = 60 * 1000; // 1 minute
var tickerId = undefined;
var lastTick = Date.now();

function initApp() {
    document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === "hidden") {
            if (typeof tickerId !== "undefined") {
                clearTimeout(tickerId);
                tickerId = undefined;
            }
        } else {
            const delta = Date.now() - lastTick;
            const delay = (delta > tickDelay) ? 0 : (tickDelay - delta);
            tick(delay);
        }
    });
    tick(0);
}

function tick(delay) {
    tickerId = setTimeout(function() {
        navigator.geolocation.getCurrentPosition(function(pos) {
            const lat = pos.coords.latitude;
            const lon = pos.coords.longitude;

            if (!withinBounds(lat, lon)) {
                setError("Doesn't look like you're in The Netherlands");
                return;
            }
            const next = `/@${lat},${lon}`;
            replaceContent(next).then((error) => {
                if (typeof error !== "undefined") {
                    setError(error);
                } else {
                    lastTick = Date.now();
                    tick(tickDelay);
                }
            });

        }, setError, { maximumAge: 10000 });
    }, delay);
}

function withinBounds(lat, lon) {
    const validLat = lat >= 48.895301818847656 && lat < 55.973602294921875;
    const validLon = lon >= 0.0 && lon < 10.856452941894531;
    return validLat && validLon;
}

function setError(msg) {
    const error = document.getElementById("error");
    if (typeof msg === "string") {
        error.innerText = `Error: ${msg}`;
    } else {
        error.innerText = "Error retrieving location";
    }
}

async function replaceContent(uri) {
    let response = await fetch(uri);

    if (!response.ok) {
        return "Remote error, please reload";
    }

    const text = await response.text();
    const parsed = parser.parseFromString(text, "text/html");

    const newContent = parsed.getElementById("content").innerHTML;
    document.getElementById("content").innerHTML = newContent;

    return undefined;
}

window.onload = (_event) => {
    initApp();
}
