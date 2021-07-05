"use strict";

(function() {
    var update_todo = async function() {
        var result = await fetch("/get");
        var decoded = await result.json();
        var dom = document.getElementById("todo_list");

        // Clear previous list
        dom.innerHTML = '';

        // Add the lines to the dom
        for (const message of decoded) {
            var line = document.createElement("div");
            line.appendChild(document.createTextNode(message));
            dom.appendChild(line);
        }
    };

    var add_todo = async function(message) {
        await fetch("/add", {
            method: "POST",
            body: JSON.stringify({
                message
            }),
            headers: {
                "Content-Type": "application/json",
            },
        });

        /// At this point we should probably call update_todo() in case we're disconnected.
        /// However, for demonstration purposes, let's handle the updating entirely via SSE.
        // update_todo();
    };

    document.getElementById("todo-submit").onsubmit = function(_event) {
        var input = document.getElementById("todo-message");
        var text = input.value;
        input.value = '';
        add_todo(text);
        return false;
    };

    var event_stream = new EventSource("/events");
    event_stream.addEventListener("todo-update", function(_event) {
        update_todo();
    });

    event_stream.onopen = function(_event) {
        // We just (re-)connected, so pull in the latest information. If we don't do this,
        // it can happen that we lose updates which happened while we were disconnected.
        update_todo();
    };
})();