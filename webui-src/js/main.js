let eventSource = new EventSource("/event-stream/");

eventSource.onmessage = function (evt) {
    console.log("Received Server Sent Event data", evt.data);
    let data = JSON.parse(evt.data);
    console.log("Received Server Sent Event data", data);
};
