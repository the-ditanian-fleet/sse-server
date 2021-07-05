import os
from flask import Flask, redirect, jsonify, request
from branca import Branca
import msgpack
import requests

app = Flask(__name__)
branca = Branca(bytes.fromhex(os.environ["SSE_SECRET"]))

# Private and public SSE urls are different because of docker-compose
private_sse_url = "http://sse-server:8000"
public_sse_url = "http://localhost:8000"

todo_list = []

def _notify_update():
    encoded_request = msgpack.packb({
        "events": [{
            "topic": "todo",
            "event": "todo-update",
            "data": "",
        }]
    })
    requests.post("%s/submit" % private_sse_url, data = branca.encode(encoded_request))

@app.route('/')
def home_page():
    # You can add templates and all that, but for this demo we just need to load some html page
    return redirect('/static/todo.html')

@app.route('/get')
def get_todo_list():
    return jsonify(todo_list)

@app.route('/add', methods=["POST"])
def add_todo_item():
    todo_list.append(request.json["message"])
    _notify_update()
    return 'OK'

@app.route('/events')
def events():
    topics = ["todo"]
    token = branca.encode(msgpack.packb({
        "topics": topics
    }))
    url = "%s/events?token=%s" % (public_sse_url, token)
    return redirect(url, 307)
