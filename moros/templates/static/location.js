function getPosition() {
    navigator.geolocation.getCurrentPosition(setLocation, setError, { maximumAge: 10000 });
}

function setLocation(pos) {
    const lat = pos.coords.latitude;
    const lon = pos.coords.longitude;

    if (!withinBounds(lat, lon)) {
        setError("Doesn't look like you're in The Netherlands");
        return;
    }
    const path = window.location.pathname;
    const locpath = `/@${lat},${lon}`;

    if (path !== locpath) {
        window.location.href = locpath;
    } else {
        window.location.reload();
    }
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
