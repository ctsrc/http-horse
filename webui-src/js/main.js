let eventSource = new EventSource("/event-stream/");

eventSource.onmessage = function (evt) {
    let data = JSON.parse(evt.data);
    console.log("Received Server Sent Event data", data);
};
