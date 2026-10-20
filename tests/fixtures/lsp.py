"""Synthetic stdio language server; never reads project code."""
import json
import pathlib
import os
import sys
import time
import threading
import urllib.parse

mode = sys.argv[1]
documents = {}
counts = {"initialize": 0, "open": 0, "change": 0, "definition": 0}
root = None
ready = threading.Event()
write_lock = threading.Lock()


def send(message):
    body = json.dumps({"jsonrpc": "2.0", **message}).encode()
    with write_lock:
        sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        sys.stdout.buffer.flush()


while True:
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            sys.exit(0)
        if line == b"\r\n":
            break
        key, value = line.decode().split(":", 1)
        headers[key.lower()] = value.strip()
    message = json.loads(sys.stdin.buffer.read(int(headers["content-length"])))
    method = message.get("method")
    params = message.get("params", {})
    if method == "initialize":
        if mode == "local-toolchain":
            assert os.environ.get("GOTOOLCHAIN") == "local"
        counts["initialize"] += 1
        assert params["capabilities"]["general"]["positionEncodings"] == ["utf-16"]
        assert params["processId"] > 0
        uri = urllib.parse.urlparse(params["rootUri"])
        root_path = urllib.parse.unquote(uri.path)
        if sys.platform == "win32":
            root_path = root_path.lstrip("/")
        root = pathlib.Path(root_path)
        counts["pid"] = os.getpid()
        (root / "mock-log.json").write_text(json.dumps(counts))
        if mode == "hang":
            time.sleep(30)
        capabilities = {"definitionProvider": mode != "unsupported", "textDocumentSync": 2}
        if mode == "utf8":
            capabilities["positionEncoding"] = "utf-8"
        result = {"capabilities": capabilities}
        if mode == "indexing":
            assert params["capabilities"]["experimental"]["serverStatusNotification"] is True
            result["serverInfo"] = {"name": "rust-analyzer"}
        send({"id": message["id"], "result": result})
    elif method == "initialized":
        if mode == "indexing":
            send({"method": "experimental/serverStatus", "params": {"quiescent": False, "health": "ok"}})
            def finish_indexing():
                ready.set()
                send({"method": "experimental/serverStatus", "params": {"quiescent": True, "health": "ok"}})
            timer = threading.Timer(0.1, finish_indexing)
            timer.daemon = True
            timer.start()
        send({"method": "textDocument/publishDiagnostics", "params": {}})
        send({"id": "config", "method": "workspace/configuration", "params": {"items": [{"section": "mock"}]}})
        send({"id": "edit", "method": "workspace/applyEdit", "params": {"edit": {"changes": {}}}})
    elif message.get("id") == "config":
        counts["configuration"] = message["result"] == [None]
    elif message.get("id") == "edit":
        counts["refused_edit"] = message["result"]["applied"] is False
    elif method in ("textDocument/didOpen", "textDocument/didChange"):
        doc = params["textDocument"]
        if method.endswith("didOpen"):
            assert doc["uri"] not in documents
            if mode == "jsx":
                assert doc["languageId"] == "javascriptreact"
            counts["open"] += 1
            text = doc["text"]
        else:
            assert doc["version"] > documents[doc["uri"]][0]
            counts["change"] += 1
            text = params["contentChanges"][0]["text"]
        documents[doc["uri"]] = (doc["version"], text)
    elif method == "textDocument/didClose":
        del documents[params["textDocument"]["uri"]]
    elif method == "textDocument/definition":
        counts["definition"] += 1
        assert params["textDocument"]["uri"] in documents
        text = documents[params["textDocument"]["uri"]][1]
        assert params["position"]["line"] == text[:text.index("mock")].count("\n")
        if mode == "error":
            send({"id": message["id"], "error": {"code": -32801, "message": "synthetic content modified"}})
            continue
        target = (root / ("target.json" if mode == "json" else "target.ts")).as_uri()
        target_text = documents.get(target, (1, "export function mock() {}"))[1]
        line = 1 if target_text.startswith("\n") else 0
        column = 16
        if mode == "samefile":
            target = params["textDocument"]["uri"]
            line, column = 1, 9
        span = {"start": {"line": line, "character": column}, "end": {"line": line, "character": column + 4}}
        location = {"uri": target, "range": span}
        link = {"targetUri": target, "targetRange": span, "targetSelectionRange": span}
        result = location if mode == "location" else [link] if mode == "links" else [location]
        if mode == "empty":
            result = None
        if mode == "indexing" and not ready.is_set():
            result = None
        if mode == "malformed":
            result = {"synthetic": "bad result"}
        (root / "mock-log.json").write_text(json.dumps(counts))
        send({"id": message["id"], "result": result})
