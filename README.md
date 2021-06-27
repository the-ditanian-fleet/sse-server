# sse-server

Simple [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events) server that clients can connect to and applications can push to.

## Quick start

    docker run --env SSE_SECRET=`openssl rand -hex 32` tvdw/sse-server

## Origin

I was building a web application in React (JS) and Flask (Python), and had a use case for sending events from the server to the client, which meant using WebSockets or SSE (or (long-)polling). On the client side SSE is great, but for servers it's a bit of work to handle the many concurrent connections involved in doing this. In the case of my web application I had to use Flask with `gevent`, a hack that makes Python less thread-bound, and in a way that routed all connections to the same webserver which prevents zero-downtime deployments (or events would be lost) and prevents using a highly available configuration behind a load balancer.

So after a while I wrote this little server to split off the SSE-specific logic, so that the main backend application doesn't need to deal with the hacks required to make SSE work. The Python application can now be highly available with zero downtime during deployments, I no longer have to use libraries that significantly change how the interpreter works, and my code is much cleaner.

`sse-server` is meant as a very low-complexity drop-in solution for using Server-Sent Events, without requiring significant application-level changes to work.

## High-level overview

The server has a concept of a *topic* that can receive events. Clients can subscribe to one or more topics, and events can be submitted to a topic. Once an event is submitted to a topic, all subscribed clients will receive it.

## Usage

Run the `sse-server` somewhere. You will need a `SSE_SECRET` environment variable containing a hex-encoded 32-byte secret used for encryption and authentication.

### GET `/events?token=...`

Primary `EventSource` endpoint. The token is a [Branca](https://branca.io/)-encrypted `msgpack`-serialized request containing a list of topics.

Since a client should not be able to sign the token itself, it's recommended that the backend server tells the client. A HTTP `307 Temporary Redirect` is great for this.

    from branca import Branca
    import msgpack

    topics = ["weather", "news"]
    encoded_request = msgpack.packb({
        "topics": topics
    })
    signed_token = Branca(bytes.fromhex(os.environ["SSE_SECRET"])).encode(encoded_request)

    url = "http://localhost:8000/events?token=%s" % signed_token
    return redirect(url, 307)

### POST `/submit`

Submits one or more events to a topic.

    encoded_request = msgpack.packb({
        "events": [{
            # Trigger a "weather-update" event for anyone subscribed to the "weather" topic. This
            # can then tell browsers to pull the latest weather from the backend.
            "topic": "weather",
            "event": "weather-update",
            "data": "",
        }]
    })
    
    requests.post("http://127.0.0.1:8000/submit", data = Branca(...).encode(encoded_request))

## Delivery guarantees

Once a connection is established, clients will receive all events for topics they are subscribed to, in the order the events were sent, until the client disconnects. Events are not dropped quietly, and errors will result in the connection being closed. Events submitted to topics with no subscribers will be considered successfully delivered to zero subscribers.

Clients must be aware that once the connection has been established, they may have missed events that happened on previous connections or while the client was not connected. Once the connection has been established clients should ensure that they download the server-side state prior to processing incoming events.

## HA configuration

You can't (beyond an active-standby configuration)! Event submission is an operation that will result in an event being sent only to clients connected to the instance the events are submitted to. In practice however this may not be a problem for most applications. Browsers will automatically reconnect if the connection goes away, and trigger an `EventSource` notification that can be intercepted by code. When this happens, clients should wait a few seconds (backoff/jitter) and then re-establish their local state which may have drifted in the seconds the connection was gone, for example by doing a request to the backend to fetch the latest information. Effectively a disconnect of the event stream should result in a full rebuild of the local state because events may have been missed while the connection was gone.

The tradeoff here is complexity and for most applications (but not all!) it should be a good choice. Allowing clients to reconnect without losing events involves buffering which can quickly fill up memory if events need to be held for more than a few seconds. HA support requires supporting cluster configurations in which multiple instances talk to each other to coordinate event delivery.

## Scaling limits

The server is written in asynchronous Rust, on a webserver framework that can utilize multiple CPU cores for parallel processing. Depending on the event throughput the server should be able to easily handle a hundred thousand concurrent subscriptions.

## Security

This project uses [Branca](https://branca.io/) tokens because I didn't want to deal with JWT. I read the Branca spec and it looks very sensible to me, but I'm no cryptographer. The Rust cryptography library used hasn't been formally verified for correctness either.

All requests to the server are authenticated by these tokens, so it's important that whatever code creates the tokens does so carefully. If an attacker gets access to the secret used to encrypt them, they can send or intercept everything.

The code has no rate limiting in place, and no replay protection. In fact browsers like to retry EventSource requests so replay protection would be bad.

This software was written to tell browsers to pull some data from the backend. Maybe don't use it to send sensitive information over the internet. Up to you.

## License

MIT